//! Tool call extraction and formatting for ACP providers.
//!
//! ACP agents run in plan mode (no tool execution). Tool definitions are
//! injected into the system prompt, and the model outputs `<tool_call>` XML
//! markers in its text response. This module extracts those markers and
//! converts them to structured tool calls for the daemon to execute.

use rig::completion::ToolDefinition;
use serde::Deserialize;

/// A tool call extracted from `<tool_call>...</tool_call>` XML in text.
#[derive(Debug, Clone)]
pub struct ExtractedToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

/// Format tool definitions as XML for injection into the system prompt.
///
/// Returns an empty string if `tools` is empty.
pub fn format_tool_definitions_prompt(tools: &[ToolDefinition]) -> String {
    if tools.is_empty() {
        return String::new();
    }

    let mut out = String::from(
        "\n\n# External Tool Protocol\n\n\
         CRITICAL: The following tools are executed by an EXTERNAL system (NevoFlux browser engine), NOT by you.\n\
         You do NOT need to check if you \"have\" these tools. You do NOT execute them.\n\
         Your ONLY job is to OUTPUT the <tool_call> XML block. The external system intercepts it, \
         executes the tool, and returns the result in the next message.\n\n\
         Available external tools:\n\n<tools>\n",
    );
    for tool in tools {
        let params = serde_json::to_string(&tool.parameters).unwrap_or_default();
        out.push_str(&format!(
            "<tool name=\"{}\" description=\"{}\">\n{}\n</tool>\n",
            tool.name, tool.description, params
        ));
    }
    out.push_str("</tools>\n\n");
    out.push_str("To use a tool, output EXACTLY this XML (nothing else around it):\n");
    out.push_str("<tool_call>\n");
    out.push_str("{\"id\":\"call_1\",\"name\":\"tool_name\",\"arguments\":{...}}\n");
    out.push_str("</tool_call>\n\n");
    out.push_str("Rules:\n");
    out.push_str("- After outputting <tool_call>, STOP immediately. Do NOT continue writing.\n");
    out.push_str("- The external system will execute the tool and give you the result.\n");
    out.push_str("- Generate a unique id for each call (\"call_1\", \"call_2\", etc.).\n");
    out.push_str("- Do NOT wrap <tool_call> in markdown code blocks.\n");
    out.push_str("- Do NOT say \"I don't have this tool\" — you DO have them via this protocol.\n");
    out.push_str("- Do NOT use your own built-in tools (bash, read, write, etc.). ONLY use <tool_call> XML.\n");
    out.push_str("- Do NOT hallucinate tool results. Wait for the external system to respond.");
    out
}

/// Extract tool calls from text containing `<tool_call>...</tool_call>` markers.
///
/// Returns `(cleaned_text, extracted_tool_calls)` where `cleaned_text` is the
/// original text with all tool call markers removed.
///
/// Also strips any `<tool_result>...</tool_result>` markers that the model may
/// have hallucinated (instead of waiting for actual tool results from the daemon).
pub fn extract_tool_calls_from_text(text: &str) -> (String, Vec<ExtractedToolCall>) {
    let mut tool_calls = Vec::new();
    let mut cleaned = String::new();
    let mut remaining = text;

    loop {
        let Some(start_idx) = remaining.find("<tool_call>") else {
            // No more tool_call markers.
            // If we already extracted tool calls, discard any trailing text —
            // the model should have stopped after </tool_call> but may have
            // hallucinated tool results, explanations, or other garbage.
            if tool_calls.is_empty() {
                cleaned.push_str(remaining);
            }
            break;
        };

        // Add text before the marker
        cleaned.push_str(&remaining[..start_idx]);

        let after_start = &remaining[start_idx + "<tool_call>".len()..];

        let Some(end_idx) = after_start.find("</tool_call>") else {
            // No closing tag — keep the raw text as-is
            cleaned.push_str(&remaining[start_idx..]);
            break;
        };

        let json_str = after_start[..end_idx].trim();

        // Use streaming deserializer to tolerate trailing garbage (e.g. extra `}`)
        // that LLMs sometimes produce.
        let parsed_result = serde_json::from_str::<serde_json::Value>(json_str).or_else(|_| {
            let mut de = serde_json::Deserializer::from_str(json_str);
            serde_json::Value::deserialize(&mut de)
        });

        if let Ok(parsed) = parsed_result {
            let id = parsed
                .get("id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let name = parsed
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let arguments = parsed
                .get("arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
            tool_calls.push(ExtractedToolCall {
                id,
                name,
                arguments,
            });
        } else {
            // Malformed JSON — keep the raw text
            cleaned.push_str(
                &remaining
                    [start_idx..start_idx + "<tool_call>".len() + end_idx + "</tool_call>".len()],
            );
        }

        remaining = &after_start[end_idx + "</tool_call>".len()..];
    }

    let cleaned = cleaned.trim().to_string();
    (cleaned, tool_calls)
}

/// Format a tool call as XML for inclusion in conversation history.
pub fn format_tool_call_as_xml(id: &str, name: &str, arguments: &serde_json::Value) -> String {
    let args_str = serde_json::to_string(arguments).unwrap_or_default();
    format!(
        "<tool_call>\n{{\"id\":\"{}\",\"name\":\"{}\",\"arguments\":{}}}\n</tool_call>",
        id, name, args_str
    )
}

/// Format a tool result as XML for inclusion in conversation history.
pub fn format_tool_result_as_xml(tool_call_id: &str, content: &str) -> String {
    format!(
        "<tool_result call_id=\"{}\">\n{}\n</tool_result>",
        tool_call_id, content
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_tool_calls_simple() {
        let text = r#"<tool_call>
{"id":"call_1","name":"screenshot","arguments":{"url":"https://example.com"}}
</tool_call>"#;
        let (cleaned, calls) = extract_tool_calls_from_text(text);
        assert!(cleaned.is_empty());
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].id, "call_1");
        assert_eq!(calls[0].name, "screenshot");
    }

    #[test]
    fn test_extract_tool_calls_with_text() {
        let text = r#"Let me take a screenshot.
<tool_call>
{"id":"call_1","name":"screenshot","arguments":{}}
</tool_call>
Done!"#;
        let (cleaned, calls) = extract_tool_calls_from_text(text);
        assert_eq!(calls.len(), 1);
        assert!(cleaned.contains("Let me take a screenshot"));
        assert!(!cleaned.contains("Done!"));
    }

    #[test]
    fn test_extract_tool_calls_none() {
        let text = "Just a regular response.";
        let (cleaned, calls) = extract_tool_calls_from_text(text);
        assert!(calls.is_empty());
        assert_eq!(cleaned, text);
    }

    #[test]
    fn test_extract_tool_calls_trailing_brace() {
        let text = r#"<tool_call>
{"id":"call_1","name":"create_artifact","arguments":{"title":"Test","files":{"index.html":"<h1>Hi</h1>"}}}
}
</tool_call>"#;
        let (_, calls) = extract_tool_calls_from_text(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "create_artifact");
    }

    #[test]
    fn test_extract_discards_hallucinated_tool_result() {
        let text = r#"I'll check.
<tool_call>
{"id":"call_1","name":"browser_get_markdown","arguments":{"tab_id":2}}
</tool_call><tool_result id="call_1">
Fake content
</tool_result>"#;
        let (cleaned, calls) = extract_tool_calls_from_text(text);
        assert_eq!(calls.len(), 1);
        assert!(!cleaned.contains("Fake content"));
    }

    #[test]
    fn test_format_tool_definitions() {
        let tools = vec![ToolDefinition {
            name: "screenshot".to_string(),
            description: "Take a screenshot".to_string(),
            parameters: serde_json::json!({"type": "object"}),
        }];
        let prompt = format_tool_definitions_prompt(&tools);
        assert!(prompt.contains("<tools>"));
        assert!(prompt.contains("screenshot"));
        assert!(prompt.contains("<tool_call>"));
    }

    #[test]
    fn test_format_tool_definitions_empty() {
        assert!(format_tool_definitions_prompt(&[]).is_empty());
    }
}
