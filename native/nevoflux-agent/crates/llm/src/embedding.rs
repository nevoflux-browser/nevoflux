//! Embedding provider abstraction and FastEmbed implementation.
//!
//! Provides the [`EmbeddingProvider`] trait for generating text embeddings
//! and a [`FastEmbedProvider`] implementation using the fastembed crate
//! with local CPU-based inference.
//!
//! # Example
//!
//! ```no_run
//! use nevoflux_llm::embedding::{EmbeddingConfig, FastEmbedProvider, EmbeddingProvider};
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let config = EmbeddingConfig::default();
//! let provider = FastEmbedProvider::new(config)?;
//! let embedding = provider.embed("Hello, world!").await?;
//! println!("Embedding dimensions: {}", embedding.len());
//! # Ok(())
//! # }
//! ```

#[cfg(feature = "embedding")]
use std::path::PathBuf;
#[cfg(feature = "embedding")]
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Errors that can occur during embedding operations.
#[derive(Error, Debug)]
pub enum EmbeddingError {
    /// Failed to initialize the embedding model.
    #[error("Failed to initialize embedding model: {0}")]
    InitError(String),

    /// Failed to generate embeddings.
    #[error("Failed to generate embedding: {0}")]
    GenerationError(String),
}

/// Supported embedding models.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum EmbeddingModel {
    /// intfloat/multilingual-e5-small — 384 dimensions, multilingual support.
    MultilingualE5Small,
    /// A custom model specified by name (not yet supported).
    Custom(String),
}

impl Default for EmbeddingModel {
    fn default() -> Self {
        Self::MultilingualE5Small
    }
}

impl EmbeddingModel {
    /// Returns the number of output dimensions for the model.
    pub fn dimensions(&self) -> usize {
        match self {
            Self::MultilingualE5Small => 384,
            Self::Custom(_) => 0, // Unknown until loaded
        }
    }
}

/// Configuration for embedding model initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmbeddingConfig {
    /// Which embedding model to use.
    pub model: EmbeddingModel,
    /// Whether to show download progress when fetching model files.
    #[serde(default)]
    pub show_download_progress: bool,
}

impl Default for EmbeddingConfig {
    fn default() -> Self {
        Self {
            model: EmbeddingModel::default(),
            show_download_progress: false,
        }
    }
}

/// Trait for generating text embeddings.
///
/// Implementations must be Send + Sync so they can be shared across
/// async tasks and threads.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Generate an embedding vector for a single text.
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError>;

    /// Generate embedding vectors for a batch of texts.
    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError>;

    /// Returns the number of dimensions in the embedding vectors.
    fn dimensions(&self) -> usize;
}

/// Resolves the model cache directory with the following priority:
///
/// 1. `{data_dir}/models/fastembed/` — NEVOFLUX_DATA_DIR override
/// 2. `%LOCALAPPDATA%/nevoflux/models/fastembed/` (Windows) or `~/.cache/fastembed/` (Unix)
///    - On Windows, if bundled models exist at `{exe_dir}/models/fastembed/`, they are
///      automatically copied to the writable location on first run.
/// 3. `{exe_dir}/models/fastembed/` — direct use on Unix (typically writable)
///
/// Returns the first usable directory found.
#[cfg(feature = "embedding")]
pub fn resolve_cache_dir() -> PathBuf {
    // Priority 1: NEVOFLUX_DATA_DIR override
    if let Some(data_dir) = std::env::var_os("NEVOFLUX_DATA_DIR") {
        let data_path = PathBuf::from(data_dir).join("models").join("fastembed");
        if data_path.exists() {
            tracing::info!(path = %data_path.display(), "Using data dir model directory");
            return data_path;
        }
    }

    // Priority 2 (Windows): writable %LOCALAPPDATA% location.
    // If bundled models exist next to exe, copy them here on first run.
    #[cfg(windows)]
    {
        if let Some(local_appdata) = std::env::var_os("LOCALAPPDATA") {
            let writable_dir = PathBuf::from(local_appdata)
                .join("nevoflux")
                .join("models")
                .join("fastembed");

            // Already copied — use it directly
            if writable_dir.exists() {
                tracing::info!(path = %writable_dir.display(), "Using local model cache");
                return writable_dir;
            }

            // Check for bundled models next to the executable
            if let Ok(exe_path) = std::env::current_exe() {
                if let Some(exe_dir) = exe_path.parent() {
                    let bundled_dir = exe_dir.join("models").join("fastembed");
                    if bundled_dir.exists() {
                        // First run: copy bundled models to writable location
                        tracing::info!(
                            src = %bundled_dir.display(),
                            dst = %writable_dir.display(),
                            "Copying bundled models to writable location (first run)"
                        );
                        if let Err(e) = copy_dir_recursive(&bundled_dir, &writable_dir) {
                            tracing::warn!(
                                error = %e,
                                "Failed to copy models, falling back to bundled directory"
                            );
                            return bundled_dir;
                        }
                        tracing::info!(path = %writable_dir.display(), "Models copied successfully");
                        return writable_dir;
                    }
                }
            }

            // No bundled models — use writable dir as download target
            tracing::info!(path = %writable_dir.display(), "Using local model cache (download target)");
            return writable_dir;
        }
    }

    // Priority 2 (Unix): use portable dir directly if it exists
    #[cfg(not(windows))]
    if let Ok(exe_path) = std::env::current_exe() {
        if let Some(exe_dir) = exe_path.parent() {
            let portable_dir = exe_dir.join("models").join("fastembed");
            if portable_dir.exists() {
                tracing::info!(path = %portable_dir.display(), "Using portable model directory");
                return portable_dir;
            }
        }
    }

    // Priority 3: User cache directory
    if let Some(cache_dir) = dirs::cache_dir() {
        let cache = cache_dir.join("fastembed");
        tracing::info!(path = %cache.display(), "Using user cache model directory");
        return cache;
    }

    // Fallback: current directory
    PathBuf::from("fastembed")
}

/// Recursively copy a directory tree.
#[cfg(windows)]
#[cfg(feature = "embedding")]
fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// Embedding provider using the fastembed crate for local CPU-based inference.
///
/// Wraps `fastembed::TextEmbedding` in an `Arc` so the provider can be
/// cloned and shared across async tasks. Embedding generation is offloaded
/// to a blocking thread pool via `tokio::task::spawn_blocking`.
#[cfg(feature = "embedding")]
pub struct FastEmbedProvider {
    model: Arc<fastembed::TextEmbedding>,
    dims: usize,
}

#[cfg(feature = "embedding")]
impl std::fmt::Debug for FastEmbedProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FastEmbedProvider")
            .field("dims", &self.dims)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "embedding")]
impl FastEmbedProvider {
    /// Create a new FastEmbedProvider with the given configuration.
    ///
    /// This will download the model if it is not already cached locally.
    /// The model files are stored in the resolved cache directory
    /// (see [`resolve_cache_dir`]).
    pub fn new(config: EmbeddingConfig) -> Result<Self, EmbeddingError> {
        let fastembed_model = match &config.model {
            EmbeddingModel::MultilingualE5Small => fastembed::EmbeddingModel::MultilingualE5Small,
            EmbeddingModel::Custom(name) => {
                return Err(EmbeddingError::InitError(format!(
                    "Custom embedding model '{}' is not yet supported",
                    name
                )));
            }
        };

        let dims = config.model.dimensions();
        let cache_dir = resolve_cache_dir();

        tracing::info!(
            cache_dir = %cache_dir.display(),
            model = ?config.model,
            "Initializing embedding model"
        );

        // If local model files exist, set HF_HUB_OFFLINE=1 to prevent
        // fastembed from trying to download from huggingface.co (which may
        // be unreachable and causes a 20+ second timeout).
        if cache_dir.exists() {
            std::env::set_var("HF_HUB_OFFLINE", "1");
            tracing::debug!("Set HF_HUB_OFFLINE=1 (local cache exists)");
        }

        let options = fastembed::InitOptions::new(fastembed_model)
            .with_cache_dir(cache_dir.clone())
            .with_show_download_progress(config.show_download_progress);

        let text_embedding = fastembed::TextEmbedding::try_new(options).map_err(|e| {
            EmbeddingError::InitError(format!("{} (cache_dir: {})", e, cache_dir.display()))
        })?;

        Ok(Self {
            model: Arc::new(text_embedding),
            dims,
        })
    }
}

#[cfg(feature = "embedding")]
#[async_trait]
impl EmbeddingProvider for FastEmbedProvider {
    async fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let model = Arc::clone(&self.model);
        let text = text.to_string();

        let result = tokio::task::spawn_blocking(move || model.embed(vec![text], None))
            .await
            .map_err(|e| EmbeddingError::GenerationError(format!("Task join error: {}", e)))?
            .map_err(|e| EmbeddingError::GenerationError(e.to_string()))?;

        result
            .into_iter()
            .next()
            .ok_or_else(|| EmbeddingError::GenerationError("Empty embedding result".to_string()))
    }

    async fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        let model = Arc::clone(&self.model);
        let texts = texts.to_vec();

        tokio::task::spawn_blocking(move || model.embed(texts, None))
            .await
            .map_err(|e| EmbeddingError::GenerationError(format!("Task join error: {}", e)))?
            .map_err(|e| EmbeddingError::GenerationError(e.to_string()))
    }

    fn dimensions(&self) -> usize {
        self.dims
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = EmbeddingConfig::default();
        assert_eq!(config.model, EmbeddingModel::MultilingualE5Small);
        assert!(!config.show_download_progress);
    }

    #[cfg(feature = "embedding")]
    #[test]
    fn test_custom_model_rejected() {
        let config = EmbeddingConfig {
            model: EmbeddingModel::Custom("my-custom-model".to_string()),
            show_download_progress: false,
        };
        let result = FastEmbedProvider::new(config);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("not yet supported"),
            "Expected 'not yet supported' in error message, got: {}",
            err
        );
    }

    #[cfg(feature = "embedding")]
    #[test]
    fn test_resolve_cache_dir_returns_valid_path() {
        let cache_dir = resolve_cache_dir();
        let path_str = cache_dir.to_string_lossy();
        assert!(
            path_str.ends_with("fastembed"),
            "Expected cache dir to end with 'fastembed', got: {}",
            path_str
        );
    }

    #[test]
    fn test_embedding_model_dimensions() {
        assert_eq!(EmbeddingModel::MultilingualE5Small.dimensions(), 384);
        assert_eq!(
            EmbeddingModel::Custom("unknown".to_string()).dimensions(),
            0
        );
    }

    #[test]
    fn test_embedding_error_display() {
        let init_err = EmbeddingError::InitError("bad model".to_string());
        assert_eq!(
            init_err.to_string(),
            "Failed to initialize embedding model: bad model"
        );

        let gen_err = EmbeddingError::GenerationError("out of memory".to_string());
        assert_eq!(
            gen_err.to_string(),
            "Failed to generate embedding: out of memory"
        );
    }

    #[test]
    fn test_embedding_config_serialization() {
        let config = EmbeddingConfig::default();
        let json = serde_json::to_string(&config).expect("serialize");
        let deserialized: EmbeddingConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(deserialized.model, EmbeddingModel::MultilingualE5Small);
    }

    /// This test requires the model to be downloaded (~100MB).
    /// Run with: cargo test -p nevoflux-llm test_embed_generates_vector -- --ignored
    #[cfg(feature = "embedding")]
    #[tokio::test]
    #[ignore]
    async fn test_embed_generates_vector() {
        let config = EmbeddingConfig {
            model: EmbeddingModel::MultilingualE5Small,
            show_download_progress: true,
        };
        let provider =
            FastEmbedProvider::new(config).expect("Failed to initialize embedding model");

        assert_eq!(provider.dimensions(), 384);

        let embedding = provider
            .embed("Hello, world!")
            .await
            .expect("Failed to generate embedding");

        assert_eq!(
            embedding.len(),
            384,
            "Expected 384-dim embedding, got {}",
            embedding.len()
        );

        // Verify the embedding is not all zeros
        let sum: f32 = embedding.iter().map(|x| x.abs()).sum();
        assert!(sum > 0.0, "Embedding should not be all zeros");
    }

    /// This test requires the model to be downloaded (~100MB).
    /// Run with: cargo test -p nevoflux-llm test_similar_texts_have_high_similarity -- --ignored
    #[cfg(feature = "embedding")]
    #[tokio::test]
    #[ignore]
    async fn test_similar_texts_have_high_similarity() {
        let config = EmbeddingConfig {
            model: EmbeddingModel::MultilingualE5Small,
            show_download_progress: true,
        };
        let provider =
            FastEmbedProvider::new(config).expect("Failed to initialize embedding model");

        let texts = vec![
            "The cat sat on the mat".to_string(),
            "A kitten was sitting on the rug".to_string(),
            "The stock market crashed yesterday".to_string(),
        ];

        let embeddings = provider
            .embed_batch(&texts)
            .await
            .expect("Failed to generate batch embeddings");

        assert_eq!(embeddings.len(), 3);

        // Cosine similarity helper
        let cosine_similarity = |a: &[f32], b: &[f32]| -> f32 {
            let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
            let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
            let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
            if norm_a == 0.0 || norm_b == 0.0 {
                return 0.0;
            }
            dot / (norm_a * norm_b)
        };

        // "cat on mat" should be more similar to "kitten on rug" than to "stock market"
        let sim_cat_kitten = cosine_similarity(&embeddings[0], &embeddings[1]);
        let sim_cat_stock = cosine_similarity(&embeddings[0], &embeddings[2]);

        assert!(
            sim_cat_kitten > sim_cat_stock,
            "Expected 'cat on mat' to be more similar to 'kitten on rug' ({}) \
             than to 'stock market' ({})",
            sim_cat_kitten,
            sim_cat_stock
        );
    }
}
