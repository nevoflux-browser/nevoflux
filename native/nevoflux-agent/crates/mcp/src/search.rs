//! Tool search with BM25 ranking.
//!
//! Provides text-based search over tool definitions using the BM25 algorithm
//! for relevance ranking.

use crate::types::ToolDefinition;
use std::collections::HashMap;

/// BM25 parameters.
#[derive(Debug, Clone)]
pub struct Bm25Config {
    /// Term frequency saturation parameter (default: 1.2).
    pub k1: f64,
    /// Length normalization parameter (default: 0.75).
    pub b: f64,
}

impl Default for Bm25Config {
    fn default() -> Self {
        Self { k1: 1.2, b: 0.75 }
    }
}

/// A searchable document in the index.
#[derive(Debug, Clone)]
struct IndexedDocument {
    /// Original tool definition.
    tool: ToolDefinition,
    /// Term frequency map.
    term_freqs: HashMap<String, u32>,
    /// Document length (total term count).
    doc_len: usize,
}

/// Search result with relevance score.
#[derive(Debug, Clone)]
pub struct SearchResult {
    /// The matching tool definition.
    pub tool: ToolDefinition,
    /// BM25 relevance score.
    pub score: f64,
}

/// Tool search index using BM25 ranking.
#[derive(Debug, Clone)]
pub struct ToolSearchIndex {
    /// Indexed documents.
    documents: Vec<IndexedDocument>,
    /// Document frequency for each term.
    doc_freqs: HashMap<String, u32>,
    /// Average document length.
    avg_doc_len: f64,
    /// BM25 configuration.
    config: Bm25Config,
}

impl Default for ToolSearchIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl ToolSearchIndex {
    /// Create a new empty search index.
    pub fn new() -> Self {
        Self::with_config(Bm25Config::default())
    }

    /// Create a new search index with custom BM25 configuration.
    pub fn with_config(config: Bm25Config) -> Self {
        Self {
            documents: Vec::new(),
            doc_freqs: HashMap::new(),
            avg_doc_len: 0.0,
            config,
        }
    }

    /// Index a collection of tools.
    pub fn index(&mut self, tools: &[ToolDefinition]) {
        self.documents.clear();
        self.doc_freqs.clear();

        for tool in tools {
            let indexed = self.index_tool(tool);

            // Update document frequencies
            for term in indexed.term_freqs.keys() {
                *self.doc_freqs.entry(term.clone()).or_insert(0) += 1;
            }

            self.documents.push(indexed);
        }

        // Calculate average document length
        let total_len: usize = self.documents.iter().map(|d| d.doc_len).sum();
        self.avg_doc_len = if self.documents.is_empty() {
            0.0
        } else {
            total_len as f64 / self.documents.len() as f64
        };
    }

    /// Add a single tool to the index.
    pub fn add(&mut self, tool: &ToolDefinition) {
        let indexed = self.index_tool(tool);

        // Update document frequencies
        for term in indexed.term_freqs.keys() {
            *self.doc_freqs.entry(term.clone()).or_insert(0) += 1;
        }

        self.documents.push(indexed);

        // Recalculate average document length
        let total_len: usize = self.documents.iter().map(|d| d.doc_len).sum();
        self.avg_doc_len = total_len as f64 / self.documents.len() as f64;
    }

    /// Search for tools matching the query.
    ///
    /// Returns results sorted by relevance score (highest first).
    /// Special queries like "*", "all", "mcp", "list" return all indexed tools.
    pub fn search(&self, query: &str) -> Vec<SearchResult> {
        if self.documents.is_empty() {
            return Vec::new();
        }

        // Generic queries that mean "show me everything"
        let normalized = query.trim().to_lowercase();
        if matches!(
            normalized.as_str(),
            "*" | "all" | "mcp" | "list" | "tools" | ""
        ) {
            return self
                .documents
                .iter()
                .map(|doc| SearchResult {
                    tool: doc.tool.clone(),
                    score: 1.0,
                })
                .collect();
        }

        let query_terms = tokenize(query);
        if query_terms.is_empty() {
            return Vec::new();
        }

        let mut results: Vec<SearchResult> = self
            .documents
            .iter()
            .map(|doc| {
                let score = self.calculate_bm25(doc, &query_terms);
                SearchResult {
                    tool: doc.tool.clone(),
                    score,
                }
            })
            .filter(|r| r.score > 0.0)
            .collect();

        // Sort by score descending
        results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        results
    }

    /// Search and return up to `limit` results.
    pub fn search_limit(&self, query: &str, limit: usize) -> Vec<SearchResult> {
        let results = self.search(query);
        results.into_iter().take(limit).collect()
    }

    /// Get the number of indexed tools.
    pub fn len(&self) -> usize {
        self.documents.len()
    }

    /// Check if the index is empty.
    pub fn is_empty(&self) -> bool {
        self.documents.is_empty()
    }

    /// Return all indexed tool definitions.
    pub fn all_tools(&self) -> Vec<ToolDefinition> {
        self.documents.iter().map(|d| d.tool.clone()).collect()
    }

    /// Clear the index.
    pub fn clear(&mut self) {
        self.documents.clear();
        self.doc_freqs.clear();
        self.avg_doc_len = 0.0;
    }

    /// Index a single tool.
    fn index_tool(&self, tool: &ToolDefinition) -> IndexedDocument {
        let mut terms = Vec::new();

        // Index tool name (higher weight by including multiple times)
        let name_terms = tokenize(&tool.name);
        terms.extend(name_terms.iter().cloned());
        terms.extend(name_terms.iter().cloned()); // Double weight for name

        // Index description
        terms.extend(tokenize(&tool.description));

        // Index parameter names from input schema
        if let Some(props) = tool.input_schema.get("properties") {
            if let Some(obj) = props.as_object() {
                for (param_name, param_value) in obj {
                    terms.extend(tokenize(param_name));

                    // Index parameter description if available
                    if let Some(desc) = param_value.get("description").and_then(|v| v.as_str()) {
                        terms.extend(tokenize(desc));
                    }
                }
            }
        }

        // Build term frequency map
        let mut term_freqs = HashMap::new();
        for term in &terms {
            *term_freqs.entry(term.clone()).or_insert(0) += 1;
        }

        let doc_len = terms.len();

        IndexedDocument {
            tool: tool.clone(),
            term_freqs,
            doc_len,
        }
    }

    /// Calculate BM25 score for a document given query terms.
    fn calculate_bm25(&self, doc: &IndexedDocument, query_terms: &[String]) -> f64 {
        let n = self.documents.len() as f64;
        let k1 = self.config.k1;
        let b = self.config.b;

        let mut score = 0.0;

        for term in query_terms {
            // Term frequency in this document
            let tf = doc.term_freqs.get(term).copied().unwrap_or(0) as f64;
            if tf == 0.0 {
                continue;
            }

            // Document frequency
            let df = self.doc_freqs.get(term).copied().unwrap_or(0) as f64;
            if df == 0.0 {
                continue;
            }

            // IDF (Inverse Document Frequency)
            let idf = ((n - df + 0.5) / (df + 0.5) + 1.0).ln();

            // Document length normalization
            let doc_len = doc.doc_len as f64;
            let length_norm = 1.0 - b + b * (doc_len / self.avg_doc_len);

            // BM25 term score
            let term_score = idf * (tf * (k1 + 1.0)) / (tf + k1 * length_norm);
            score += term_score;
        }

        score
    }
}

/// Tokenize text into lowercase terms.
///
/// Splits on non-alphanumeric characters including underscores,
/// making both "browser" and "navigate" searchable from "browser_navigate".
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty() && s.len() > 1)
        .map(|s| s.to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_tool(name: &str, description: &str) -> ToolDefinition {
        ToolDefinition {
            name: name.to_string(),
            description: description.to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {}
            }),
        }
    }

    fn create_tool_with_params(
        name: &str,
        description: &str,
        params: &[(&str, &str)],
    ) -> ToolDefinition {
        let mut properties = serde_json::Map::new();
        for (param_name, param_desc) in params {
            properties.insert(
                param_name.to_string(),
                serde_json::json!({
                    "type": "string",
                    "description": param_desc
                }),
            );
        }

        ToolDefinition {
            name: name.to_string(),
            description: description.to_string(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": properties
            }),
        }
    }

    #[test]
    fn test_tokenize() {
        let tokens = tokenize("Hello_World, this is a Test!");
        // Underscores split tokens
        assert!(tokens.contains(&"hello".to_string()));
        assert!(tokens.contains(&"world".to_string()));
        assert!(tokens.contains(&"this".to_string()));
        assert!(tokens.contains(&"is".to_string()));
        assert!(tokens.contains(&"test".to_string()));
        // Single char 'a' should be filtered out
        assert!(!tokens.contains(&"a".to_string()));
    }

    #[test]
    fn test_empty_index() {
        let index = ToolSearchIndex::new();

        assert!(index.is_empty());
        assert_eq!(index.len(), 0);

        let results = index.search("test");
        assert!(results.is_empty());
    }

    #[test]
    fn test_index_tools() {
        let mut index = ToolSearchIndex::new();

        let tools = vec![
            create_tool("read_file", "Read the contents of a file"),
            create_tool("write_file", "Write content to a file"),
            create_tool("list_directory", "List files in a directory"),
        ];

        index.index(&tools);

        assert_eq!(index.len(), 3);
        assert!(!index.is_empty());
    }

    #[test]
    fn test_search_by_name() {
        let mut index = ToolSearchIndex::new();

        let tools = vec![
            create_tool("read_file", "Read the contents of a file"),
            create_tool("write_file", "Write content to a file"),
            create_tool("browser_navigate", "Navigate to a URL"),
        ];

        index.index(&tools);

        let results = index.search("file");
        assert_eq!(results.len(), 2);
        // Both file-related tools should match
        let names: Vec<&str> = results.iter().map(|r| r.tool.name.as_str()).collect();
        assert!(names.contains(&"read_file"));
        assert!(names.contains(&"write_file"));
    }

    #[test]
    fn test_search_by_description() {
        let mut index = ToolSearchIndex::new();

        let tools = vec![
            create_tool("browser_navigate", "Navigate to a URL in the browser"),
            create_tool("browser_click", "Click an element on the page"),
            create_tool("read_file", "Read a file from disk"),
        ];

        index.index(&tools);

        let results = index.search("browser");
        assert_eq!(results.len(), 2);
        let names: Vec<&str> = results.iter().map(|r| r.tool.name.as_str()).collect();
        assert!(names.contains(&"browser_navigate"));
        assert!(names.contains(&"browser_click"));
    }

    #[test]
    fn test_search_by_parameter() {
        let mut index = ToolSearchIndex::new();

        let tools = vec![
            create_tool_with_params(
                "read_file",
                "Read a file",
                &[("path", "The file path to read")],
            ),
            create_tool_with_params(
                "make_request",
                "Make HTTP request",
                &[("url", "The URL to request")],
            ),
        ];

        index.index(&tools);

        let results = index.search("path");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].tool.name, "read_file");
    }

    #[test]
    fn test_search_ranking() {
        let mut index = ToolSearchIndex::new();

        // Tool with "file" in name should rank higher than description-only match
        let tools = vec![
            create_tool("read_file", "Read contents"),
            create_tool("browser_action", "Do something with a file in the browser"),
        ];

        index.index(&tools);

        let results = index.search("file");
        assert_eq!(results.len(), 2);
        // read_file should rank higher due to name match (double weighted)
        assert_eq!(results[0].tool.name, "read_file");
        assert!(results[0].score > results[1].score);
    }

    #[test]
    fn test_search_no_match() {
        let mut index = ToolSearchIndex::new();

        let tools = vec![
            create_tool("read_file", "Read the contents of a file"),
            create_tool("write_file", "Write content to a file"),
        ];

        index.index(&tools);

        let results = index.search("database");
        assert!(results.is_empty());
    }

    #[test]
    fn test_search_limit() {
        let mut index = ToolSearchIndex::new();

        let tools = vec![
            create_tool("file_read", "Read file"),
            create_tool("file_write", "Write file"),
            create_tool("file_delete", "Delete file"),
            create_tool("file_copy", "Copy file"),
        ];

        index.index(&tools);

        let results = index.search_limit("file", 2);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_add_tool() {
        let mut index = ToolSearchIndex::new();

        index.add(&create_tool("read_file", "Read a file"));
        assert_eq!(index.len(), 1);

        index.add(&create_tool("write_file", "Write a file"));
        assert_eq!(index.len(), 2);

        let results = index.search("file");
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn test_clear_index() {
        let mut index = ToolSearchIndex::new();

        index.add(&create_tool("read_file", "Read a file"));
        index.add(&create_tool("write_file", "Write a file"));

        index.clear();

        assert!(index.is_empty());
        assert_eq!(index.len(), 0);
    }

    #[test]
    fn test_custom_bm25_config() {
        let config = Bm25Config { k1: 2.0, b: 0.5 };
        let mut index = ToolSearchIndex::with_config(config);

        index.add(&create_tool("test", "Test tool"));

        let results = index.search("test");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_empty_query() {
        let mut index = ToolSearchIndex::new();

        index.add(&create_tool("read_file", "Read a file"));

        // Empty/generic queries return all tools
        let results = index.search("");
        assert_eq!(results.len(), 1);

        let results = index.search("   ");
        assert_eq!(results.len(), 1);

        let results = index.search("*");
        assert_eq!(results.len(), 1);

        let results = index.search("mcp");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_case_insensitive_search() {
        let mut index = ToolSearchIndex::new();

        index.add(&create_tool("ReadFile", "Read a FILE"));

        let results = index.search("file");
        assert_eq!(results.len(), 1);

        let results = index.search("FILE");
        assert_eq!(results.len(), 1);

        let results = index.search("File");
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn test_multi_word_query() {
        let mut index = ToolSearchIndex::new();

        let tools = vec![
            create_tool("read_file", "Read the contents of a file"),
            create_tool("read_directory", "Read directory listing"),
            create_tool("write_file", "Write to a file"),
        ];

        index.index(&tools);

        // "read file" should match read_file better than others
        let results = index.search("read file");
        assert!(!results.is_empty());
        assert_eq!(results[0].tool.name, "read_file");
    }
}
