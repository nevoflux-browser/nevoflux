//! Tool signature generation for Code Mode orchestrate tool.
//!
//! Generates Python function signatures from ToolDefinition JSON Schema,
//! used in the orchestrate tool description (compact) and Monty type checker (full).

use std::collections::HashMap;

use nevoflux_builtin_wasm::{ToolDefinition, ASYNC_SAFE_TOOLS};
use serde_json::Value;

/// Returns `true` if a tool must run sequentially (not in the ASYNC_SAFE_TOOLS list).
///
/// Uses the canonical list from `nevoflux_builtin_wasm::ASYNC_SAFE_TOOLS`.
pub fn is_sequential(name: &str) -> bool {
    !ASYNC_SAFE_TOOLS.contains(&name)
}

/// Maps a JSON Schema type definition to a Python type annotation string.
///
/// Handles primitive types, arrays with typed items, bare objects, enums,
/// `anyOf` unions (including nullable types), and falls back to `Any` for
/// unrecognized or empty schemas.
pub fn json_schema_to_python_type(schema: &Value) -> String {
    // Const takes priority — a single fixed value
    if let Some(const_val) = schema.get("const") {
        return match const_val {
            Value::String(s) => format!("Literal[\"{s}\"]"),
            other => format!("Literal[{other}]"),
        };
    }

    // Enum takes priority — even if "type" is also present
    if let Some(enum_values) = schema.get("enum") {
        if let Some(arr) = enum_values.as_array() {
            let literals: Vec<String> = arr
                .iter()
                .map(|v| match v {
                    Value::String(s) => format!("\"{s}\""),
                    other => other.to_string(),
                })
                .collect();
            return format!("Literal[{}]", literals.join(", "));
        }
    }

    // anyOf union types
    if let Some(any_of) = schema.get("anyOf") {
        if let Some(variants) = any_of.as_array() {
            let types: Vec<String> = variants
                .iter()
                .map(|v| json_schema_to_python_type(v))
                .collect();
            return types.join(" | ");
        }
    }

    // Type-based mapping
    match schema.get("type").and_then(|t| t.as_str()) {
        Some("string") => "str".to_string(),
        Some("integer") => "int".to_string(),
        Some("number") => "float".to_string(),
        Some("boolean") => "bool".to_string(),
        Some("null") => "None".to_string(),
        Some("array") => {
            let item_type = schema
                .get("items")
                .map(|items| json_schema_to_python_type(items))
                .unwrap_or_else(|| "Any".to_string());
            format!("list[{item_type}]")
        }
        Some("object") => "dict[str, Any]".to_string(),
        _ => "Any".to_string(),
    }
}

/// Format a JSON value as a Python literal (for default parameter values).
pub fn format_default(val: &Value) -> String {
    match val {
        Value::Null => "None".to_string(),
        Value::Bool(b) => {
            if *b {
                "True".to_string()
            } else {
                "False".to_string()
            }
        }
        Value::String(s) => {
            let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
            format!("\"{escaped}\"")
        }
        Value::Number(n) => n.to_string(),
        Value::Array(arr) => {
            let items: Vec<String> = arr.iter().map(format_default).collect();
            format!("[{}]", items.join(", "))
        }
        Value::Object(map) => {
            let entries: Vec<String> = map
                .iter()
                .map(|(k, v)| format!("\"{k}\": {}", format_default(v)))
                .collect();
            format!("{{{}}}", entries.join(", "))
        }
    }
}

/// Extract ordered parameter names from schema: required first, then optional (sorted).
fn extract_param_names(schema: &Value) -> (Vec<String>, Vec<String>) {
    let props = match schema.get("properties").and_then(|p| p.as_object()) {
        Some(p) => p,
        None => return (vec![], vec![]),
    };

    let required: Vec<String> = schema
        .get("required")
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();

    let mut required_names: Vec<String> = Vec::new();
    let mut optional_names: Vec<String> = Vec::new();

    for key in props.keys() {
        if required.contains(key) {
            required_names.push(key.clone());
        } else {
            optional_names.push(key.clone());
        }
    }

    // Sort each group for deterministic output
    required_names.sort();
    optional_names.sort();

    (required_names, optional_names)
}

/// Build the parameter list string for a Python function signature from JSON Schema.
pub fn generate_params_string(schema: &Value) -> String {
    let props = match schema.get("properties").and_then(|p| p.as_object()) {
        Some(p) => p,
        None => return String::new(),
    };

    let (required_names, optional_names) = extract_param_names(schema);

    let mut params: Vec<String> = Vec::new();

    // Required params first (no default)
    for name in &required_names {
        if let Some(prop_schema) = props.get(name) {
            let ty = json_schema_to_python_type(prop_schema);
            params.push(format!("{name}: {ty}"));
        }
    }

    // Optional params with defaults
    for name in &optional_names {
        if let Some(prop_schema) = props.get(name) {
            let ty = json_schema_to_python_type(prop_schema);
            let default = if let Some(def_val) = prop_schema.get("default") {
                format_default(def_val)
            } else {
                "None".to_string()
            };
            params.push(format!("{name}: {ty} = {default}"));
        }
    }

    params.join(", ")
}

/// Build the Args docstring section from JSON Schema property descriptions.
pub fn generate_param_docs(schema: &Value) -> String {
    let props = match schema.get("properties").and_then(|p| p.as_object()) {
        Some(p) => p,
        None => return String::new(),
    };

    let (required_names, optional_names) = extract_param_names(schema);
    let all_names: Vec<&String> = required_names.iter().chain(optional_names.iter()).collect();

    if all_names.is_empty() {
        return String::new();
    }

    let mut lines = vec!["    Args:".to_string()];

    for name in &all_names {
        let desc = props
            .get(*name)
            .and_then(|s| s.get("description"))
            .and_then(|d| d.as_str())
            .unwrap_or("No description");
        lines.push(format!("        {name}: {desc}"));
    }

    lines.join("\n")
}

/// Generate a compact one-line signature for a tool.
///
/// Format: `async def web_fetch(url: str) -> Any  # Fetch URL content`
/// Sequential tools use `def` and get `[sequential]` suffix in the comment.
pub fn generate_compact_signature(tool: &ToolDefinition) -> String {
    let is_seq = is_sequential(&tool.name);
    let keyword = if is_seq { "def" } else { "async def" };
    let params = generate_params_string(&tool.input_schema);
    let suffix = if is_seq { "  [sequential]" } else { "" };
    format!(
        "{keyword} {}({params}) -> Any  # {}{}",
        tool.name, tool.description, suffix
    )
}

/// Generate a full Python stub with docstring for a tool.
///
/// ```python
/// async def web_fetch(url: str) -> Any:
///     """Fetch URL content
///
///     Args:
///         url: The URL to fetch
///     """
///     ...
/// ```
pub fn generate_full_stub(tool: &ToolDefinition) -> String {
    let is_seq = is_sequential(&tool.name);
    let keyword = if is_seq { "def" } else { "async def" };
    let params = generate_params_string(&tool.input_schema);
    let param_docs = generate_param_docs(&tool.input_schema);

    let mut stub = format!("{keyword} {}({params}) -> Any:\n", tool.name);
    stub.push_str(&format!("    \"\"\"{}", tool.description));

    if !param_docs.is_empty() {
        stub.push_str(&format!("\n\n{param_docs}\n"));
    } else {
        stub.push('\n');
    }

    stub.push_str("    \"\"\"\n");
    stub.push_str("    ...\n");
    stub
}

/// Extract parameter names from a tool definition's input schema.
/// Required parameters come first, then optional (both groups sorted).
pub fn build_param_mapping(tool: &ToolDefinition) -> Vec<String> {
    let (required, optional) = extract_param_names(&tool.input_schema);
    let mut result = required;
    result.extend(optional);
    result
}

/// Cached signature data for a set of tools.
///
/// Pre-computes compact signatures, full stubs, and parameter mappings
/// so they can be reused without re-parsing JSON schemas.
pub struct SignatureCache {
    compact: String,
    full_stubs: String,
    param_mappings: HashMap<String, Vec<String>>,
    tool_names: Vec<String>,
}

impl SignatureCache {
    /// Build a `SignatureCache` from a slice of tool definitions.
    pub fn build(tools: &[ToolDefinition]) -> Self {
        let mut compact_lines: Vec<String> = Vec::with_capacity(tools.len());
        let mut full_parts: Vec<String> = Vec::with_capacity(tools.len());
        let mut param_mappings: HashMap<String, Vec<String>> = HashMap::new();
        let mut tool_names: Vec<String> = Vec::with_capacity(tools.len());

        for tool in tools {
            compact_lines.push(generate_compact_signature(tool));
            full_parts.push(generate_full_stub(tool));
            param_mappings.insert(tool.name.clone(), build_param_mapping(tool));
            tool_names.push(tool.name.clone());
        }

        SignatureCache {
            compact: compact_lines.join("\n"),
            full_stubs: full_parts.join("\n"),
            param_mappings,
            tool_names,
        }
    }

    /// Get the compact signatures block (one per line).
    pub fn compact_signatures(&self) -> &str {
        &self.compact
    }

    /// Get the full stubs block (all stubs concatenated).
    pub fn full_stubs(&self) -> &str {
        &self.full_stubs
    }

    /// Look up parameter mapping for a specific tool by name.
    pub fn param_mapping(&self, tool_name: &str) -> Option<&Vec<String>> {
        self.param_mappings.get(tool_name)
    }

    /// Return the list of all tool (external function) names.
    pub fn external_function_names(&self) -> Vec<String> {
        self.tool_names.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ---- Existing json_schema_to_python_type tests ----

    #[test]
    fn test_primitive_types() {
        assert_eq!(
            json_schema_to_python_type(&json!({"type": "string"})),
            "str"
        );
        assert_eq!(
            json_schema_to_python_type(&json!({"type": "integer"})),
            "int"
        );
        assert_eq!(
            json_schema_to_python_type(&json!({"type": "number"})),
            "float"
        );
        assert_eq!(
            json_schema_to_python_type(&json!({"type": "boolean"})),
            "bool"
        );
    }

    #[test]
    fn test_null_type() {
        assert_eq!(json_schema_to_python_type(&json!({"type": "null"})), "None");
    }

    #[test]
    fn test_array_type() {
        assert_eq!(
            json_schema_to_python_type(&json!({"type": "array", "items": {"type": "string"}})),
            "list[str]"
        );
    }

    #[test]
    fn test_array_without_items() {
        assert_eq!(
            json_schema_to_python_type(&json!({"type": "array"})),
            "list[Any]"
        );
    }

    #[test]
    fn test_bare_object() {
        assert_eq!(
            json_schema_to_python_type(&json!({"type": "object"})),
            "dict[str, Any]"
        );
    }

    #[test]
    fn test_object_with_properties() {
        assert_eq!(
            json_schema_to_python_type(
                &json!({"type": "object", "properties": {"name": {"type": "string"}}})
            ),
            "dict[str, Any]"
        );
    }

    #[test]
    fn test_enum_type() {
        assert_eq!(
            json_schema_to_python_type(
                &json!({"type": "string", "enum": ["left", "right", "middle"]})
            ),
            "Literal[\"left\", \"right\", \"middle\"]"
        );
    }

    #[test]
    fn test_any_of() {
        assert_eq!(
            json_schema_to_python_type(
                &json!({"anyOf": [{"type": "string"}, {"type": "integer"}]})
            ),
            "str | int"
        );
    }

    #[test]
    fn test_nullable() {
        assert_eq!(
            json_schema_to_python_type(&json!({"anyOf": [{"type": "string"}, {"type": "null"}]})),
            "str | None"
        );
    }

    #[test]
    fn test_format_default_escapes_special_chars() {
        // String with quotes should be escaped
        assert_eq!(
            format_default(&serde_json::json!("hello \"world\"")),
            "\"hello \\\"world\\\"\""
        );
        // String with backslash should be escaped
        assert_eq!(
            format_default(&serde_json::json!("path\\to\\file")),
            "\"path\\\\to\\\\file\""
        );
    }

    #[test]
    fn test_const_string() {
        assert_eq!(
            json_schema_to_python_type(&json!({"const": "fixed_value"})),
            "Literal[\"fixed_value\"]"
        );
    }

    #[test]
    fn test_const_number() {
        assert_eq!(
            json_schema_to_python_type(&json!({"const": 42})),
            "Literal[42]"
        );
    }

    #[test]
    fn test_no_type() {
        assert_eq!(json_schema_to_python_type(&json!({})), "Any");
    }

    #[test]
    fn test_nested_array() {
        assert_eq!(
            json_schema_to_python_type(
                &json!({"type": "array", "items": {"type": "array", "items": {"type": "integer"}}})
            ),
            "list[list[int]]"
        );
    }

    // ---- New tests for signature generation, ASYNC_SAFE, SignatureCache ----

    fn tool_def(name: &str, desc: &str, schema: serde_json::Value) -> ToolDefinition {
        ToolDefinition {
            name: name.into(),
            description: desc.into(),
            input_schema: schema,
        }
    }

    #[test]
    fn test_is_sequential() {
        assert!(!is_sequential("web_fetch")); // async safe
        assert!(!is_sequential("browser_screenshot")); // async safe
        assert!(is_sequential("browser_click")); // sequential
        assert!(is_sequential("bash")); // sequential
        assert!(is_sequential("unknown_tool")); // default sequential
    }

    #[test]
    fn test_compact_signature_async() {
        let tool = tool_def(
            "web_fetch",
            "Fetch URL content",
            json!({
                "type": "object",
                "properties": {"url": {"type": "string", "description": "The URL to fetch"}},
                "required": ["url"]
            }),
        );
        let sig = generate_compact_signature(&tool);
        assert_eq!(
            sig,
            "async def web_fetch(url: str) -> Any  # Fetch URL content"
        );
    }

    #[test]
    fn test_compact_signature_sequential() {
        let tool = tool_def(
            "browser_click",
            "Click an element",
            json!({
                "type": "object",
                "properties": {
                    "selector": {"type": "string"},
                    "button": {"type": "string", "enum": ["left", "right", "middle"], "default": "left"}
                },
                "required": ["selector"]
            }),
        );
        let sig = generate_compact_signature(&tool);
        assert!(sig.starts_with("def browser_click("));
        assert!(sig.contains("[sequential]"));
        assert!(!sig.contains("async"));
    }

    #[test]
    fn test_compact_signature_no_params() {
        let tool = tool_def(
            "browser_get_tabs",
            "Get all open tabs",
            json!({"type": "object", "properties": {}}),
        );
        let sig = generate_compact_signature(&tool);
        assert_eq!(
            sig,
            "async def browser_get_tabs() -> Any  # Get all open tabs"
        );
    }

    #[test]
    fn test_full_stub_has_docstring() {
        let tool = tool_def(
            "web_fetch",
            "Fetch URL content",
            json!({
                "type": "object",
                "properties": {"url": {"type": "string", "description": "The URL to fetch"}},
                "required": ["url"]
            }),
        );
        let stub = generate_full_stub(&tool);
        assert!(stub.contains("async def web_fetch(url: str) -> Any:"));
        assert!(stub.contains("\"\"\"Fetch URL content"));
        assert!(stub.contains("url: The URL to fetch"));
        assert!(stub.contains("..."));
    }

    #[test]
    fn test_build_param_mapping() {
        let tool = tool_def(
            "browser_click",
            "Click",
            json!({
                "type": "object",
                "properties": {
                    "selector": {"type": "string"},
                    "button": {"type": "string", "default": "left"}
                },
                "required": ["selector"]
            }),
        );
        let mapping = build_param_mapping(&tool);
        assert_eq!(mapping[0], "selector"); // required first
        assert!(mapping.contains(&"button".to_string()));
    }

    #[test]
    fn test_build_param_mapping_empty() {
        let tool = tool_def(
            "get_tabs",
            "Get tabs",
            json!({"type": "object", "properties": {}}),
        );
        assert!(build_param_mapping(&tool).is_empty());
    }

    #[test]
    fn test_signature_cache() {
        let tools = vec![
            tool_def(
                "web_fetch",
                "Fetch URL",
                json!({
                    "type": "object",
                    "properties": {"url": {"type": "string"}},
                    "required": ["url"]
                }),
            ),
            tool_def(
                "browser_click",
                "Click element",
                json!({
                    "type": "object",
                    "properties": {"selector": {"type": "string"}},
                    "required": ["selector"]
                }),
            ),
        ];
        let cache = SignatureCache::build(&tools);

        let compact = cache.compact_signatures();
        assert!(compact.contains("async def web_fetch"));
        assert!(compact.contains("def browser_click"));

        assert_eq!(
            cache.param_mapping("web_fetch"),
            Some(&vec!["url".to_string()])
        );
        assert_eq!(cache.param_mapping("nonexistent"), None);

        let names = cache.external_function_names();
        assert!(names.contains(&"web_fetch".to_string()));
        assert!(names.contains(&"browser_click".to_string()));
    }
}
