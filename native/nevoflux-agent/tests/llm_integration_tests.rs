//! Integration tests for LLM host function types.
//!
//! These tests verify the serialization and parsing of LLM-related
//! data structures used in the Wasm host functions.

use nevoflux_daemon::{LlmChatRequest, LlmChatResponse, LlmMessage, LlmUsage};

#[test]
fn test_llm_request_structure() {
    let request = LlmChatRequest {
        messages: vec![LlmMessage::user("Hello!")],
        system: Some("You are helpful.".into()),
        temperature: Some(0.7),
        max_tokens: Some(1000),
        tools: None,
    };

    let json = serde_json::to_string(&request).unwrap();
    assert!(json.contains("Hello!"));
    assert!(json.contains("You are helpful."));
    assert!(json.contains("0.7"));
    assert!(json.contains("1000"));
}

#[test]
fn test_llm_response_parsing() {
    let json = r#"{"content":"Hi there!","finish_reason":"stop","usage":{"prompt_tokens":10,"completion_tokens":5,"total_tokens":15}}"#;
    let response: LlmChatResponse = serde_json::from_str(json).unwrap();

    assert_eq!(response.content, "Hi there!");
    assert_eq!(response.finish_reason, "stop");
    assert!(response.usage.is_some());

    let usage = response.usage.unwrap();
    assert_eq!(usage.prompt_tokens, 10);
    assert_eq!(usage.completion_tokens, 5);
    assert_eq!(usage.total_tokens, 15);
}

#[test]
fn test_llm_usage_structure() {
    let usage = LlmUsage {
        prompt_tokens: 100,
        completion_tokens: 50,
        total_tokens: 150,
    };

    let json = serde_json::to_string(&usage).unwrap();
    assert!(json.contains("100"));
    assert!(json.contains("50"));
    assert!(json.contains("150"));

    let parsed: LlmUsage = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.prompt_tokens, 100);
    assert_eq!(parsed.completion_tokens, 50);
    assert_eq!(parsed.total_tokens, 150);
}

#[test]
fn test_llm_message_serialization() {
    let message = LlmMessage::assistant("How can I help you?");

    let json = serde_json::to_string(&message).unwrap();
    assert!(json.contains("assistant"));
    assert!(json.contains("How can I help you?"));

    let parsed: LlmMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.role, "assistant");
    assert_eq!(parsed.content, "How can I help you?");
}

#[test]
fn test_llm_request_minimal() {
    // Test with only required fields
    let request = LlmChatRequest {
        messages: vec![LlmMessage::user("Hi")],
        ..Default::default()
    };

    let json = serde_json::to_string(&request).unwrap();
    let parsed: LlmChatRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.messages.len(), 1);
    assert!(parsed.system.is_none());
    assert!(parsed.temperature.is_none());
    assert!(parsed.max_tokens.is_none());
}

#[test]
fn test_llm_request_multiple_messages() {
    let request = LlmChatRequest {
        messages: vec![
            LlmMessage::user("Hello"),
            LlmMessage::assistant("Hi there!"),
            LlmMessage::user("How are you?"),
        ],
        system: Some("Be friendly".into()),
        temperature: Some(0.5),
        max_tokens: Some(500),
        tools: None,
    };

    let json = serde_json::to_string(&request).unwrap();
    let parsed: LlmChatRequest = serde_json::from_str(&json).unwrap();

    assert_eq!(parsed.messages.len(), 3);
    assert_eq!(parsed.messages[0].role, "user");
    assert_eq!(parsed.messages[1].role, "assistant");
    assert_eq!(parsed.messages[2].role, "user");
    assert_eq!(parsed.system, Some("Be friendly".into()));
}

#[test]
fn test_llm_response_without_usage() {
    let json = r#"{"content":"Hello!","finish_reason":"stop","usage":null}"#;
    let response: LlmChatResponse = serde_json::from_str(json).unwrap();

    assert_eq!(response.content, "Hello!");
    assert_eq!(response.finish_reason, "stop");
    assert!(response.usage.is_none());
}

#[test]
fn test_llm_request_from_json_string() {
    let json = r#"{
        "messages": [
            {"role": "user", "content": "What is 2+2?"}
        ],
        "system": "You are a math tutor.",
        "temperature": 0.0,
        "max_tokens": 100
    }"#;

    let request: LlmChatRequest = serde_json::from_str(json).unwrap();

    assert_eq!(request.messages.len(), 1);
    assert_eq!(request.messages[0].content, "What is 2+2?");
    assert_eq!(request.system, Some("You are a math tutor.".into()));
    assert_eq!(request.temperature, Some(0.0));
    assert_eq!(request.max_tokens, Some(100));
}

#[test]
fn test_llm_response_to_json_string() {
    let response = LlmChatResponse {
        content: "The answer is 4.".into(),
        finish_reason: "stop".into(),
        tool_calls: None,
        usage: Some(LlmUsage {
            prompt_tokens: 20,
            completion_tokens: 10,
            total_tokens: 30,
        }),
        images: vec![],
    };

    let json = serde_json::to_string_pretty(&response).unwrap();
    assert!(json.contains("The answer is 4."));
    assert!(json.contains("\"total_tokens\": 30"));
}

#[test]
fn test_llm_types_clone() {
    let message = LlmMessage::user("Test");
    let cloned_message = message.clone();
    assert_eq!(cloned_message.role, message.role);
    assert_eq!(cloned_message.content, message.content);

    let usage = LlmUsage {
        prompt_tokens: 1,
        completion_tokens: 2,
        total_tokens: 3,
    };
    let cloned_usage = usage.clone();
    assert_eq!(cloned_usage.total_tokens, 3);

    let request = LlmChatRequest {
        messages: vec![message],
        system: Some("System".into()),
        temperature: Some(0.5),
        max_tokens: Some(100),
        tools: None,
    };
    let cloned_request = request.clone();
    assert_eq!(cloned_request.messages.len(), 1);

    let response = LlmChatResponse {
        content: "Response".into(),
        finish_reason: "stop".into(),
        tool_calls: None,
        usage: Some(usage),
        images: vec![],
    };
    let cloned_response = response.clone();
    assert_eq!(cloned_response.content, "Response");
}

#[test]
fn test_llm_types_debug() {
    let message = LlmMessage::user("Debug test");
    let debug = format!("{:?}", message);
    assert!(debug.contains("LlmMessage"));

    let usage = LlmUsage {
        prompt_tokens: 10,
        completion_tokens: 5,
        total_tokens: 15,
    };
    let debug = format!("{:?}", usage);
    assert!(debug.contains("LlmUsage"));

    let request = LlmChatRequest {
        messages: vec![],
        ..Default::default()
    };
    let debug = format!("{:?}", request);
    assert!(debug.contains("LlmChatRequest"));

    let response = LlmChatResponse {
        content: "Test".into(),
        finish_reason: "stop".into(),
        ..Default::default()
    };
    let debug = format!("{:?}", response);
    assert!(debug.contains("LlmChatResponse"));
}
