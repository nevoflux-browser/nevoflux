//! Vector search utilities.

/// Compute cosine similarity between two vectors.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let dot_product: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if norm_a == 0.0 || norm_b == 0.0 {
        return 0.0;
    }

    dot_product / (norm_a * norm_b)
}

/// Compute euclidean distance between two vectors.
pub fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return f32::MAX;
    }

    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
}

/// Vector search result.
#[derive(Debug, Clone)]
pub struct VectorSearchResult {
    /// The ID of the matching item.
    pub id: String,
    /// Similarity score (higher is better for cosine, lower for distance).
    pub score: f32,
}

/// In-memory vector index for simple similarity search.
#[derive(Default)]
pub struct SimpleVectorIndex {
    vectors: Vec<(String, Vec<f32>)>,
}

impl SimpleVectorIndex {
    /// Create a new empty index.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a vector to the index.
    pub fn add(&mut self, id: impl Into<String>, vector: Vec<f32>) {
        self.vectors.push((id.into(), vector));
    }

    /// Remove a vector by ID.
    pub fn remove(&mut self, id: &str) {
        self.vectors.retain(|(i, _)| i != id);
    }

    /// Search for similar vectors using cosine similarity.
    pub fn search(&self, query: &[f32], limit: usize) -> Vec<VectorSearchResult> {
        let mut results: Vec<_> = self
            .vectors
            .iter()
            .map(|(id, vec)| VectorSearchResult {
                id: id.clone(),
                score: cosine_similarity(query, vec),
            })
            .collect();

        // Sort by score descending
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results.truncate(limit);
        results
    }

    /// Get the number of vectors in the index.
    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    /// Check if the index is empty.
    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 0.001);
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - (-1.0)).abs() < 0.001);
    }

    #[test]
    fn test_euclidean_distance() {
        let a = vec![0.0, 0.0, 0.0];
        let b = vec![3.0, 4.0, 0.0];
        let dist = euclidean_distance(&a, &b);
        assert!((dist - 5.0).abs() < 0.001);
    }

    #[test]
    fn test_simple_vector_index() {
        let mut index = SimpleVectorIndex::new();

        index.add("vec1", vec![1.0, 0.0, 0.0]);
        index.add("vec2", vec![0.9, 0.1, 0.0]);
        index.add("vec3", vec![0.0, 1.0, 0.0]);

        let results = index.search(&[1.0, 0.0, 0.0], 2);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].id, "vec1");
        assert_eq!(results[1].id, "vec2");
    }

    #[test]
    fn test_simple_vector_index_remove() {
        let mut index = SimpleVectorIndex::new();
        index.add("vec1", vec![1.0, 0.0]);
        index.add("vec2", vec![0.0, 1.0]);

        assert_eq!(index.len(), 2);
        index.remove("vec1");
        assert_eq!(index.len(), 1);
    }
}
