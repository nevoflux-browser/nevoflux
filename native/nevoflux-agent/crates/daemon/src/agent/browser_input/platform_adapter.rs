// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Platform adapter — recipe types, loader, and hostname lookup registry.
//!
//! Recipes are YAML files describing per-site knowledge (selectors,
//! mention flows, upload constraints) that the strategy engine
//! consumes without hard-coding site-specific logic. See spec §7.
//!
//! Load order (first match wins):
//!   1. `~/.config/nevoflux/recipes/*.yaml` (user overrides)
//!   2. `$SHARE/nevoflux/recipes/*.yaml`    (release-bundled, optional)
//!   3. `include_str!(..recipes/x_com.yaml)` (compiled-in fallback)
//!
//! Recipes are loaded once at daemon startup. No hot reload in v1.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use regex::Regex;
use serde::Deserialize;

/// A single platform recipe. Deserialized from a YAML file.
///
/// Field names mirror the YAML schema in the spec exactly.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Recipe {
    pub name: String,
    pub hostname_patterns: Vec<String>,
    pub version: u32,
    #[serde(default = "default_true")]
    pub enabled: bool,

    pub compose: ComposeConfig,
    pub submit: SubmitConfig,

    #[serde(default)]
    pub mention: Option<MentionConfig>,

    #[serde(default)]
    pub hashtag: Option<HashtagConfig>,

    #[serde(default)]
    pub upload: Option<UploadConfig>,

    #[serde(default)]
    pub notes: Option<String>,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ComposeConfig {
    pub selector: String,
    #[serde(default)]
    pub fallback_selectors: Vec<String>,
    #[serde(default)]
    pub expected_framework: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitConfig {
    pub selector: String,
    #[serde(default)]
    pub fallback_selectors: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MentionConfig {
    pub trigger_char: String,
    pub pattern: String,
    pub candidate_list_selector: String,
    pub candidate_list_timeout_ms: u64,
    pub confirm_method: ConfirmMethod,
    #[serde(default)]
    pub candidate_item_selector: Option<String>,
    #[serde(default = "default_mention_pause")]
    pub pause_between_segments_ms: u64,
}

fn default_mention_pause() -> u64 {
    150
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmMethod {
    EnterKey,
    ClickFirst,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HashtagConfig {
    pub trigger_char: String,
    pub pattern: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UploadConfig {
    pub file_input_selector: String,
    #[serde(default)]
    pub upload_complete_indicator: Option<String>,
    #[serde(default)]
    pub upload_complete_timeout_ms: Option<u64>,
    #[serde(default)]
    pub accepted_mime_types: Vec<String>,
    #[serde(default)]
    pub max_size_bytes: Option<u64>,
}

/// Error type for recipe loading + validation.
#[derive(Debug, thiserror::Error)]
pub enum RecipeError {
    #[error("YAML parse error in {path}: {source}")]
    Yaml {
        path: String,
        #[source]
        source: serde_yaml::Error,
    },

    #[error("Invalid regex in recipe '{name}' mention.pattern: {source}")]
    InvalidRegex {
        name: String,
        #[source]
        source: regex::Error,
    },

    #[error("Timeout value {value}ms exceeds cap {cap}ms in recipe '{name}', field {field}")]
    TimeoutTooLarge {
        name: String,
        field: &'static str,
        value: u64,
        cap: u64,
    },

    #[error("I/O error reading {path}: {source}")]
    Io {
        path: String,
        #[source]
        source: std::io::Error,
    },
}

/// Caps enforced on recipe numeric fields at load time (spec §7.5).
pub const CANDIDATE_LIST_TIMEOUT_CAP_MS: u64 = 10_000;
pub const UPLOAD_COMPLETE_TIMEOUT_CAP_MS: u64 = 300_000;

impl Recipe {
    /// Parse a recipe from YAML text, validate it, and return a
    /// ready-to-use `Recipe` value.
    ///
    /// `source_path` is used only in error messages.
    pub fn from_yaml(source_path: &str, yaml: &str) -> Result<Self, RecipeError> {
        let recipe: Recipe = serde_yaml::from_str(yaml).map_err(|e| RecipeError::Yaml {
            path: source_path.to_string(),
            source: e,
        })?;
        recipe.validate()?;
        Ok(recipe)
    }

    /// Enforce timeout caps and compile regexes. Called once at load time.
    fn validate(&self) -> Result<(), RecipeError> {
        if let Some(mention) = &self.mention {
            if mention.candidate_list_timeout_ms > CANDIDATE_LIST_TIMEOUT_CAP_MS {
                return Err(RecipeError::TimeoutTooLarge {
                    name: self.name.clone(),
                    field: "mention.candidate_list_timeout_ms",
                    value: mention.candidate_list_timeout_ms,
                    cap: CANDIDATE_LIST_TIMEOUT_CAP_MS,
                });
            }
            // Compile the pattern to surface syntax errors early.
            Regex::new(&mention.pattern).map_err(|e| RecipeError::InvalidRegex {
                name: self.name.clone(),
                source: e,
            })?;
        }
        if let Some(upload) = &self.upload {
            if let Some(t) = upload.upload_complete_timeout_ms {
                if t > UPLOAD_COMPLETE_TIMEOUT_CAP_MS {
                    return Err(RecipeError::TimeoutTooLarge {
                        name: self.name.clone(),
                        field: "upload.upload_complete_timeout_ms",
                        value: t,
                        cap: UPLOAD_COMPLETE_TIMEOUT_CAP_MS,
                    });
                }
            }
        }
        Ok(())
    }

    /// True if this recipe applies to the given hostname.
    ///
    /// Exact match or subdomain suffix match on one of the literal
    /// hostname_patterns. NOT regex — prevents ReDoS and accidental
    /// broadening.
    pub fn matches_hostname(&self, hostname: &str) -> bool {
        if !self.enabled {
            return false;
        }
        for pat in &self.hostname_patterns {
            if hostname == pat {
                return true;
            }
            if hostname.ends_with(&format!(".{}", pat)) {
                return true;
            }
        }
        false
    }
}

/// Registry of loaded recipes, searched by hostname.
#[derive(Debug, Default)]
pub struct AdapterRegistry {
    recipes: Vec<Recipe>,
    /// Index for O(1) lookups by exact hostname match. Subdomain
    /// matching still falls back to a linear scan.
    exact_index: HashMap<String, usize>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a recipe into the registry, respecting first-match-wins
    /// semantics: if a recipe with the same `name` is already present,
    /// the new one is ignored (user overrides were loaded first).
    pub fn insert(&mut self, recipe: Recipe) {
        if self.recipes.iter().any(|r| r.name == recipe.name) {
            return;
        }
        let idx = self.recipes.len();
        for pat in &recipe.hostname_patterns {
            self.exact_index.entry(pat.clone()).or_insert(idx);
        }
        self.recipes.push(recipe);
    }

    /// Look up the first recipe that matches a hostname, or `None`.
    pub fn lookup(&self, hostname: &str) -> Option<&Recipe> {
        if let Some(&idx) = self.exact_index.get(hostname) {
            return self.recipes.get(idx);
        }
        self.recipes.iter().find(|r| r.matches_hostname(hostname))
    }

    /// Number of recipes currently loaded. Used by tools.rs logging.
    pub fn len(&self) -> usize {
        self.recipes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }

    /// Build a registry from the standard load chain (spec §7.2).
    ///
    /// 1. user_dir (if present and readable)
    /// 2. share_dir (if present)
    /// 3. compiled-in fallback (always)
    ///
    /// I/O errors on a single file are logged (via `tracing::warn!`)
    /// but do not abort registry construction; the compiled-in
    /// fallback always wins when all dirs are empty.
    pub fn load_standard(user_dir: Option<&Path>, share_dir: Option<&Path>) -> Self {
        let mut registry = Self::new();
        if let Some(dir) = user_dir {
            registry.load_dir(dir);
        }
        if let Some(dir) = share_dir {
            registry.load_dir(dir);
        }
        registry.load_compiled_in();
        registry
    }

    fn load_dir(&mut self, dir: &Path) {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!(
                    dir = %dir.display(),
                    error = %e,
                    "platform_adapter: failed to read recipe dir"
                );
                return;
            }
        };
        for entry in entries.flatten() {
            let path: PathBuf = entry.path();
            if path.extension().and_then(|s| s.to_str()) != Some("yaml") {
                continue;
            }
            match std::fs::read_to_string(&path) {
                Ok(text) => match Recipe::from_yaml(&path.display().to_string(), &text) {
                    Ok(recipe) => self.insert(recipe),
                    Err(e) => {
                        tracing::warn!(
                            path = %path.display(),
                            error = %e,
                            "platform_adapter: rejected recipe"
                        );
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        path = %path.display(),
                        error = %e,
                        "platform_adapter: failed to read recipe"
                    );
                }
            }
        }
    }

    fn load_compiled_in(&mut self) {
        const X_COM: &str = include_str!("../../../recipes/x_com.yaml");
        match Recipe::from_yaml("<compiled-in>/x_com.yaml", X_COM) {
            Ok(recipe) => self.insert(recipe),
            Err(e) => {
                tracing::error!(
                    error = %e,
                    "platform_adapter: compiled-in x_com recipe failed to parse"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_YAML: &str = r#"
name: test_site
hostname_patterns: ["example.com", "mobile.example.com"]
version: 1
enabled: true

compose:
  selector: '[data-testid="compose"]'
  expected_framework: "draft.js"

submit:
  selector: '[data-testid="submit"]'

mention:
  trigger_char: "@"
  pattern: '@([A-Za-z0-9_]{1,15})'
  candidate_list_selector: 'div[role="listbox"]'
  candidate_list_timeout_ms: 2000
  confirm_method: "enter_key"
  pause_between_segments_ms: 150

upload:
  file_input_selector: 'input[type="file"]'
  upload_complete_indicator: '.done'
  upload_complete_timeout_ms: 30000
  max_size_bytes: 5242880
"#;

    #[test]
    fn valid_yaml_parses_and_validates() {
        let r = Recipe::from_yaml("<test>", VALID_YAML).expect("should parse");
        assert_eq!(r.name, "test_site");
        assert_eq!(r.hostname_patterns.len(), 2);
        assert_eq!(r.compose.selector, "[data-testid=\"compose\"]");
        assert_eq!(r.submit.selector, "[data-testid=\"submit\"]");
        assert!(r.mention.is_some());
        assert!(r.upload.is_some());
        assert_eq!(r.mention.as_ref().unwrap().pause_between_segments_ms, 150);
        assert_eq!(
            r.mention.as_ref().unwrap().confirm_method,
            ConfirmMethod::EnterKey
        );
    }

    #[test]
    fn unknown_field_is_rejected() {
        let yaml = r##"
name: bad
hostname_patterns: ["x.com"]
version: 1
compose: {selector: "#a"}
submit: {selector: "#b"}
extra_field_that_does_not_exist: true
"##;
        let err = Recipe::from_yaml("<test>", yaml).unwrap_err();
        assert!(matches!(err, RecipeError::Yaml { .. }), "got {:?}", err);
    }

    #[test]
    fn mention_timeout_over_cap_is_rejected() {
        let yaml = VALID_YAML.replace(
            "candidate_list_timeout_ms: 2000",
            "candidate_list_timeout_ms: 999999",
        );
        let err = Recipe::from_yaml("<test>", &yaml).unwrap_err();
        match err {
            RecipeError::TimeoutTooLarge { field, cap, .. } => {
                assert_eq!(field, "mention.candidate_list_timeout_ms");
                assert_eq!(cap, CANDIDATE_LIST_TIMEOUT_CAP_MS);
            }
            other => panic!("expected TimeoutTooLarge, got {:?}", other),
        }
    }

    #[test]
    fn upload_timeout_over_cap_is_rejected() {
        let yaml = VALID_YAML.replace(
            "upload_complete_timeout_ms: 30000",
            "upload_complete_timeout_ms: 999999999",
        );
        let err = Recipe::from_yaml("<test>", &yaml).unwrap_err();
        assert!(matches!(err, RecipeError::TimeoutTooLarge { .. }));
    }

    #[test]
    fn invalid_regex_in_mention_pattern_is_rejected() {
        let yaml = VALID_YAML.replace("'@([A-Za-z0-9_]{1,15})'", "'@([unclosed'");
        let err = Recipe::from_yaml("<test>", &yaml).unwrap_err();
        assert!(
            matches!(err, RecipeError::InvalidRegex { .. }),
            "got {:?}",
            err
        );
    }

    #[test]
    fn unknown_confirm_method_is_rejected() {
        let yaml = VALID_YAML.replace(
            r#"confirm_method: "enter_key""#,
            r#"confirm_method: "zapier_lol""#,
        );
        let err = Recipe::from_yaml("<test>", &yaml).unwrap_err();
        assert!(matches!(err, RecipeError::Yaml { .. }));
    }

    #[test]
    fn matches_hostname_exact_and_subdomain() {
        let r = Recipe::from_yaml("<t>", VALID_YAML).unwrap();
        assert!(r.matches_hostname("example.com"));
        assert!(r.matches_hostname("mobile.example.com"));
        assert!(
            r.matches_hostname("www.example.com"),
            "subdomain suffix match"
        );
    }

    #[test]
    fn matches_hostname_rejects_lookalike() {
        let r = Recipe::from_yaml("<t>", VALID_YAML).unwrap();
        // CRITICAL: evilexample.com must NOT match example.com (spec §7.3)
        assert!(!r.matches_hostname("evilexample.com"));
        assert!(!r.matches_hostname("notexample.com"));
        assert!(!r.matches_hostname("example.com.evil.com"));
    }

    #[test]
    fn disabled_recipe_matches_nothing() {
        let yaml = VALID_YAML.replace("enabled: true", "enabled: false");
        let r = Recipe::from_yaml("<t>", &yaml).unwrap();
        assert!(!r.matches_hostname("example.com"));
    }

    #[test]
    fn registry_lookup_returns_first_recipe_matching_hostname() {
        let mut reg = AdapterRegistry::new();
        let r = Recipe::from_yaml("<t>", VALID_YAML).unwrap();
        reg.insert(r);
        assert_eq!(reg.len(), 1);
        assert!(reg.lookup("example.com").is_some());
        assert!(reg.lookup("mobile.example.com").is_some());
        assert!(reg.lookup("other.com").is_none());
    }

    #[test]
    fn registry_insert_ignores_duplicate_name() {
        let mut reg = AdapterRegistry::new();
        reg.insert(Recipe::from_yaml("<t>", VALID_YAML).unwrap());
        reg.insert(Recipe::from_yaml("<t>", VALID_YAML).unwrap());
        assert_eq!(reg.len(), 1);
    }

    #[test]
    fn load_standard_user_dir_overrides_compiled_in() {
        use std::io::Write;
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("x_com.yaml");
        let mut f = std::fs::File::create(&path).unwrap();
        // Same recipe name, but with a sentinel submit selector so
        // we can tell the files apart.
        let overriding = VALID_YAML
            .replace("name: test_site", "name: x_com")
            .replace(
                r#"selector: '[data-testid="submit"]'"#,
                r#"selector: '#overridden-by-user'"#,
            );
        f.write_all(overriding.as_bytes()).unwrap();
        drop(f);

        let reg = AdapterRegistry::load_standard(Some(tmp.path()), None);
        let r = reg
            .lookup("example.com")
            .expect("user-provided recipe should load");
        assert_eq!(r.name, "x_com");
        assert_eq!(r.submit.selector, "#overridden-by-user");

        // The compiled-in x_com recipe should have been skipped
        // because a recipe named "x_com" was already inserted.
        // Therefore x.com (the compiled-in pattern) is NOT matched.
        assert!(reg.lookup("x.com").is_none());
    }

    #[test]
    fn load_standard_compiled_in_only_has_x_com() {
        let reg = AdapterRegistry::load_standard(None, None);
        assert!(!reg.is_empty());
        assert!(reg.lookup("x.com").is_some());
        assert!(reg.lookup("twitter.com").is_some());
        assert!(reg.lookup("mobile.x.com").is_some());
        assert!(reg.lookup("random.org").is_none());
    }
}
