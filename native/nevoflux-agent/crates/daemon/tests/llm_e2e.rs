//! E2E tests for LLM providers.
//!
//! These tests require real API keys and are ignored by default.
//! Run with: `cargo test -p nevoflux-daemon --test llm_e2e -- --ignored`
//!
//! Required environment variables:
//! - ANTHROPIC_API_KEY: For Anthropic tests
//! - OPENAI_API_KEY: For OpenAI tests
//! - OPENROUTER_API_KEY: For OpenRouter tests
//! - DEEPSEEK_API_KEY: For DeepSeek tests
//! - DASHSCOPE_API_KEY: For Qwen tests

use nevoflux_daemon::wasm::llm::{
    execute_llm_chat, start_llm_stream, LlmAttachment, LlmChatRequest, LlmMessage,
    LlmStreamRegistry, LlmToolCall, LlmToolDefinition,
};
use nevoflux_llm::ProviderType;
use serde_json::json;
use std::sync::Arc;

fn get_env_key(name: &str) -> Option<String> {
    std::env::var(name).ok().filter(|s| !s.is_empty())
}

#[tokio::test]
#[ignore]
async fn test_anthropic_chat() {
    let api_key = match get_env_key("ANTHROPIC_API_KEY") {
        Some(key) => key,
        None => {
            eprintln!("Skipping: ANTHROPIC_API_KEY not set");
            return;
        }
    };

    let request = LlmChatRequest {
        messages: vec![LlmMessage::user(
            "Say 'Hello from Anthropic!' and nothing else.",
        )],
        system: Some("You are a helpful assistant. Be very concise.".into()),
        temperature: Some(0.0),
        max_tokens: Some(50),
        tools: None,
    };

    let response = execute_llm_chat(
        ProviderType::Anthropic,
        &api_key,
        "claude-3-haiku-20240307",
        request,
        None,
    )
    .await;

    match response {
        Ok(resp) => {
            println!("Anthropic response: {:?}", resp);
            assert!(!resp.content.is_empty());
            assert!(resp.content.to_lowercase().contains("hello"));
        }
        Err(e) => panic!("Anthropic chat failed: {:?}", e),
    }
}

#[tokio::test]
#[ignore]
async fn test_openai_chat() {
    let api_key = match get_env_key("OPENAI_API_KEY") {
        Some(key) => key,
        None => {
            eprintln!("Skipping: OPENAI_API_KEY not set");
            return;
        }
    };

    let request = LlmChatRequest {
        messages: vec![LlmMessage::user(
            "Say 'Hello from OpenAI!' and nothing else.",
        )],
        system: Some("You are a helpful assistant. Be very concise.".into()),
        temperature: Some(0.0),
        max_tokens: Some(50),
        tools: None,
    };

    let response =
        execute_llm_chat(ProviderType::OpenAi, &api_key, "gpt-4o-mini", request, None).await;

    match response {
        Ok(resp) => {
            println!("OpenAI response: {:?}", resp);
            assert!(!resp.content.is_empty());
            assert!(resp.content.to_lowercase().contains("hello"));
        }
        Err(e) => panic!("OpenAI chat failed: {:?}", e),
    }
}

#[tokio::test]
#[ignore]
async fn test_openrouter_chat() {
    let api_key = match get_env_key("OPENROUTER_API_KEY") {
        Some(key) => key,
        None => {
            eprintln!("Skipping: OPENROUTER_API_KEY not set");
            return;
        }
    };

    let request = LlmChatRequest {
        messages: vec![LlmMessage::user(
            "Say 'Hello from OpenRouter!' and nothing else.",
        )],
        system: Some("You are a helpful assistant. Be very concise.".into()),
        temperature: Some(0.0),
        max_tokens: Some(50),
        tools: None,
    };

    let response = execute_llm_chat(
        ProviderType::OpenRouter,
        &api_key,
        "openai/gpt-4o-mini",
        request,
        None,
    )
    .await;

    match response {
        Ok(resp) => {
            println!("OpenRouter response: {:?}", resp);
            assert!(!resp.content.is_empty());
            assert!(resp.content.to_lowercase().contains("hello"));
        }
        Err(e) => panic!("OpenRouter chat failed: {:?}", e),
    }
}

#[tokio::test]
#[ignore]
async fn test_deepseek_chat() {
    let api_key = match get_env_key("DEEPSEEK_API_KEY") {
        Some(key) => key,
        None => {
            eprintln!("Skipping: DEEPSEEK_API_KEY not set");
            return;
        }
    };

    let request = LlmChatRequest {
        messages: vec![LlmMessage::user(
            "Say 'Hello from DeepSeek!' and nothing else.",
        )],
        system: Some("You are a helpful assistant. Be very concise.".into()),
        temperature: Some(0.0),
        max_tokens: Some(50),
        tools: None,
    };

    let response = execute_llm_chat(
        ProviderType::DeepSeek,
        &api_key,
        "deepseek-chat",
        request,
        None,
    )
    .await;

    match response {
        Ok(resp) => {
            println!("DeepSeek response: {:?}", resp);
            assert!(!resp.content.is_empty());
            assert!(resp.content.to_lowercase().contains("hello"));
        }
        Err(e) => panic!("DeepSeek chat failed: {:?}", e),
    }
}

#[tokio::test]
#[ignore]
async fn test_qwen_chat() {
    let api_key = match get_env_key("DASHSCOPE_API_KEY") {
        Some(key) => key,
        None => {
            eprintln!("Skipping: DASHSCOPE_API_KEY not set");
            return;
        }
    };

    let request = LlmChatRequest {
        messages: vec![LlmMessage::user("Say 'Hello from Qwen!' and nothing else.")],
        system: Some("You are a helpful assistant. Be very concise.".into()),
        temperature: Some(0.0),
        max_tokens: Some(50),
        tools: None,
    };

    let response =
        execute_llm_chat(ProviderType::Qwen, &api_key, "qwen-turbo", request, None).await;

    match response {
        Ok(resp) => {
            println!("Qwen response: {:?}", resp);
            assert!(!resp.content.is_empty());
            assert!(resp.content.to_lowercase().contains("hello"));
        }
        Err(e) => panic!("Qwen chat failed: {:?}", e),
    }
}

#[tokio::test]
#[ignore]
async fn test_openai_tool_calling() {
    let api_key = match get_env_key("OPENAI_API_KEY") {
        Some(key) => key,
        None => {
            eprintln!("Skipping: OPENAI_API_KEY not set");
            return;
        }
    };

    // Step 1: Send request with tool definition
    let request = LlmChatRequest {
        messages: vec![LlmMessage::user("What's the weather in Tokyo?")],
        system: Some(
            "You are a helpful assistant. Use the get_weather tool to answer weather questions."
                .into(),
        ),
        temperature: Some(0.0),
        max_tokens: Some(200),
        tools: Some(vec![LlmToolDefinition {
            name: "get_weather".into(),
            description: "Get the current weather in a given location".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "location": {
                        "type": "string",
                        "description": "The city name, e.g. Tokyo, San Francisco"
                    }
                },
                "required": ["location"]
            }),
        }]),
    };

    let response =
        execute_llm_chat(ProviderType::OpenAi, &api_key, "gpt-4o-mini", request, None).await;

    match response {
        Ok(resp) => {
            println!("OpenAI tool call response: {:?}", resp);

            // Should return a tool call
            assert_eq!(resp.finish_reason, "tool_calls");
            assert!(resp.tool_calls.is_some());

            let tool_calls = resp.tool_calls.unwrap();
            assert!(!tool_calls.is_empty());

            let tool_call = &tool_calls[0];
            assert_eq!(tool_call.name, "get_weather");
            println!("Tool call arguments: {}", tool_call.arguments);

            // Step 2: Send tool result back
            // Use call_id for tool results if available (required by OpenAI Responses API)
            let tool_result_id = tool_call.call_id.as_ref().unwrap_or(&tool_call.id);
            let request2 = LlmChatRequest {
                messages: vec![
                    LlmMessage::user("What's the weather in Tokyo?"),
                    LlmMessage::assistant_with_tool_calls(vec![LlmToolCall {
                        id: tool_call.id.clone(),
                        call_id: tool_call.call_id.clone(),
                        name: tool_call.name.clone(),
                        arguments: tool_call.arguments.clone(),
                        signature: tool_call.signature.clone(),
                    }]),
                    LlmMessage::tool_result(
                        tool_result_id,
                        r#"{"temperature": "22°C", "condition": "Sunny", "humidity": "45%"}"#,
                    ),
                ],
                system: Some("You are a helpful assistant.".into()),
                temperature: Some(0.0),
                max_tokens: Some(200),
                tools: None,
            };

            let response2 = execute_llm_chat(
                ProviderType::OpenAi,
                &api_key,
                "gpt-4o-mini",
                request2,
                None,
            )
            .await;

            match response2 {
                Ok(resp2) => {
                    println!("OpenAI final response: {:?}", resp2);
                    assert_eq!(resp2.finish_reason, "stop");
                    assert!(!resp2.content.is_empty());
                    // Should mention the weather info
                    let content_lower = resp2.content.to_lowercase();
                    assert!(
                        content_lower.contains("22") || content_lower.contains("sunny"),
                        "Response should contain weather info"
                    );
                }
                Err(e) => panic!("OpenAI second request failed: {:?}", e),
            }
        }
        Err(e) => panic!("OpenAI tool call failed: {:?}", e),
    }
}

#[tokio::test]
#[ignore]
async fn test_multi_turn_conversation() {
    let api_key = match get_env_key("OPENAI_API_KEY") {
        Some(key) => key,
        None => {
            eprintln!("Skipping: OPENAI_API_KEY not set");
            return;
        }
    };

    // Turn 1
    let request1 = LlmChatRequest {
        messages: vec![LlmMessage::user("My name is Alice.")],
        system: Some("You are a helpful assistant. Remember the user's name.".into()),
        temperature: Some(0.0),
        max_tokens: Some(100),
        tools: None,
    };

    let resp1 = execute_llm_chat(
        ProviderType::OpenAi,
        &api_key,
        "gpt-4o-mini",
        request1,
        None,
    )
    .await
    .expect("Turn 1 failed");

    println!("Turn 1 response: {}", resp1.content);

    // Turn 2 - should remember the name
    let request2 = LlmChatRequest {
        messages: vec![
            LlmMessage::user("My name is Alice."),
            LlmMessage::assistant(&resp1.content),
            LlmMessage::user("What is my name?"),
        ],
        system: Some("You are a helpful assistant. Remember the user's name.".into()),
        temperature: Some(0.0),
        max_tokens: Some(100),
        tools: None,
    };

    let resp2 = execute_llm_chat(
        ProviderType::OpenAi,
        &api_key,
        "gpt-4o-mini",
        request2,
        None,
    )
    .await
    .expect("Turn 2 failed");

    println!("Turn 2 response: {}", resp2.content);
    assert!(
        resp2.content.to_lowercase().contains("alice"),
        "Should remember the name Alice"
    );
}

#[tokio::test]
#[ignore]
async fn test_openai_streaming_tool_calling() {
    let api_key = match get_env_key("OPENAI_API_KEY") {
        Some(key) => key,
        None => {
            eprintln!("Skipping: OPENAI_API_KEY not set");
            return;
        }
    };

    let registry = Arc::new(LlmStreamRegistry::new());

    // Step 1: Send streaming request with tool definition
    let request = LlmChatRequest {
        messages: vec![LlmMessage::user("What's the weather in Tokyo?")],
        system: Some(
            "You are a helpful assistant. Use the get_weather tool to answer weather questions."
                .into(),
        ),
        temperature: Some(0.0),
        max_tokens: Some(200),
        tools: Some(vec![LlmToolDefinition {
            name: "get_weather".into(),
            description: "Get the current weather in a given location".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "location": {
                        "type": "string",
                        "description": "The city name, e.g. Tokyo, San Francisco"
                    }
                },
                "required": ["location"]
            }),
        }]),
    };

    let stream_id = start_llm_stream(
        ProviderType::OpenAi,
        &api_key,
        "gpt-4o-mini",
        request,
        registry.clone(),
        None,
        None,
    )
    .await
    .expect("Failed to start stream");

    println!("Stream started with ID: {}", stream_id);

    // Collect all tool calls from streaming chunks
    let mut all_tool_calls = Vec::new();
    let mut chunk_count = 0;

    loop {
        // Small delay to allow chunks to arrive
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;

        match registry.next_chunk(stream_id) {
            Ok(Some(chunk)) => {
                chunk_count += 1;
                println!(
                    "Chunk {}: text={:?}, tool_calls={}, done={}",
                    chunk_count,
                    chunk.text,
                    chunk.tool_calls.len(),
                    chunk.done
                );

                for tc in &chunk.tool_calls {
                    println!(
                        "  Tool call: id={}, call_id={:?}, name={}",
                        tc.id, tc.call_id, tc.name
                    );
                }

                all_tool_calls.extend(chunk.tool_calls);

                if chunk.done {
                    break;
                }
            }
            Ok(None) => {
                // No chunk available yet, continue waiting
                continue;
            }
            Err(e) => {
                panic!("Stream error: {:?}", e);
            }
        }
    }

    registry.close(stream_id);

    println!("\n=== Summary ===");
    println!("Total chunks received: {}", chunk_count);
    println!("Total tool calls: {}", all_tool_calls.len());

    // Verify we got tool calls
    assert!(
        !all_tool_calls.is_empty(),
        "Should have received tool calls"
    );

    // Find the get_weather tool call
    let weather_call = all_tool_calls
        .iter()
        .find(|tc| tc.name == "get_weather")
        .expect("Should have get_weather tool call");

    println!("\nWeather tool call:");
    println!("  id: {}", weather_call.id);
    println!("  call_id: {:?}", weather_call.call_id);
    println!("  name: {}", weather_call.name);
    println!("  arguments: {}", weather_call.arguments);

    // CRITICAL: Verify call_id is present (this is what caused the original bug)
    assert!(
        weather_call.call_id.is_some(),
        "call_id MUST be present for OpenAI Responses API! Got id={}, call_id={:?}",
        weather_call.id,
        weather_call.call_id
    );

    let call_id = weather_call.call_id.as_ref().unwrap();
    assert!(
        call_id.starts_with("call_"),
        "call_id should start with 'call_', got: {}",
        call_id
    );

    // Step 2: Send tool result back using call_id
    let request2 = LlmChatRequest {
        messages: vec![
            LlmMessage::user("What's the weather in Tokyo?"),
            LlmMessage::assistant_with_tool_calls(vec![LlmToolCall {
                id: weather_call.id.clone(),
                call_id: weather_call.call_id.clone(),
                name: weather_call.name.clone(),
                arguments: weather_call.arguments.clone(),
                signature: weather_call.signature.clone(),
            }]),
            LlmMessage::tool_result(
                call_id, // Use call_id, not id!
                r#"{"temperature": "22°C", "condition": "Sunny", "humidity": "45%"}"#,
            ),
        ],
        system: Some("You are a helpful assistant.".into()),
        temperature: Some(0.0),
        max_tokens: Some(200),
        tools: None,
    };

    // Use non-streaming for the follow-up to verify it works
    let response2 = execute_llm_chat(
        ProviderType::OpenAi,
        &api_key,
        "gpt-4o-mini",
        request2,
        None,
    )
    .await
    .expect("Second request with tool result failed");

    println!("\nFinal response: {}", response2.content);
    assert_eq!(response2.finish_reason, "stop");
    assert!(!response2.content.is_empty());

    // Should mention the weather info
    let content_lower = response2.content.to_lowercase();
    assert!(
        content_lower.contains("22") || content_lower.contains("sunny"),
        "Response should contain weather info"
    );

    println!("\n✅ Streaming tool call test passed!");
}

#[tokio::test]
#[ignore]
async fn test_openai_streaming_tool_calling_full_streaming() {
    // This test uses streaming for BOTH requests (first and follow-up),
    // which matches the real agent scenario.
    let api_key = match get_env_key("OPENAI_API_KEY") {
        Some(key) => key,
        None => {
            eprintln!("Skipping: OPENAI_API_KEY not set");
            return;
        }
    };

    let registry = Arc::new(LlmStreamRegistry::new());

    // Step 1: Send streaming request with tool definition
    let request = LlmChatRequest {
        messages: vec![LlmMessage::user("What's the weather in Tokyo?")],
        system: Some(
            "You are a helpful assistant. Use the get_weather tool to answer weather questions."
                .into(),
        ),
        temperature: Some(0.0),
        max_tokens: Some(200),
        tools: Some(vec![LlmToolDefinition {
            name: "get_weather".into(),
            description: "Get the current weather in a given location".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "location": {
                        "type": "string",
                        "description": "The city name, e.g. Tokyo, San Francisco"
                    }
                },
                "required": ["location"]
            }),
        }]),
    };

    let stream_id = start_llm_stream(
        ProviderType::OpenAi,
        &api_key,
        "gpt-4o-mini",
        request,
        registry.clone(),
        None,
        None,
    )
    .await
    .expect("Failed to start stream");

    // Collect tool call from first stream
    let mut weather_call = None;
    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        match registry.next_chunk(stream_id) {
            Ok(Some(chunk)) => {
                for tc in &chunk.tool_calls {
                    if tc.name == "get_weather" {
                        weather_call = Some(tc.clone());
                    }
                }
                if chunk.done {
                    break;
                }
            }
            Ok(None) => continue,
            Err(e) => panic!("Stream error: {:?}", e),
        }
    }
    registry.close(stream_id);

    let weather_call = weather_call.expect("Should have get_weather tool call");
    let call_id = weather_call
        .call_id
        .as_ref()
        .expect("call_id must be present");

    println!(
        "First stream got tool call: id={}, call_id={}",
        weather_call.id, call_id
    );

    // Step 2: Send tool result back using STREAMING (this is the key difference!)
    let request2 = LlmChatRequest {
        messages: vec![
            LlmMessage::user("What's the weather in Tokyo?"),
            LlmMessage::assistant_with_tool_calls(vec![LlmToolCall {
                id: weather_call.id.clone(),
                call_id: weather_call.call_id.clone(),
                name: weather_call.name.clone(),
                arguments: weather_call.arguments.clone(),
                signature: weather_call.signature.clone(),
            }]),
            LlmMessage::tool_result(
                call_id,
                r#"{"temperature": "22°C", "condition": "Sunny", "humidity": "45%"}"#,
            ),
        ],
        system: Some("You are a helpful assistant.".into()),
        temperature: Some(0.0),
        max_tokens: Some(200),
        tools: None,
    };

    // Use STREAMING for the follow-up (this is what the real agent does)
    let stream_id2 = start_llm_stream(
        ProviderType::OpenAi,
        &api_key,
        "gpt-4o-mini",
        request2,
        registry.clone(),
        None,
        None,
    )
    .await
    .expect("Failed to start second stream");

    println!("Second stream started with ID: {}", stream_id2);

    // Collect response from second stream
    let mut combined_text = String::new();
    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        match registry.next_chunk(stream_id2) {
            Ok(Some(chunk)) => {
                if let Some(text) = &chunk.text {
                    combined_text.push_str(text);
                }
                if chunk.done {
                    break;
                }
            }
            Ok(None) => continue,
            Err(e) => panic!("Second stream error: {:?}", e),
        }
    }
    registry.close(stream_id2);

    println!("\nFinal response (via streaming): {}", combined_text);
    assert!(!combined_text.is_empty(), "Response should not be empty");

    let content_lower = combined_text.to_lowercase();
    assert!(
        content_lower.contains("22") || content_lower.contains("sunny"),
        "Response should contain weather info"
    );

    println!("\n✅ Full streaming tool call test passed!");
}

/// Helper struct to track tool calls and build messages for multi-round tests
struct ToolCallTracker {
    messages: Vec<LlmMessage>,
    tools: Vec<LlmToolDefinition>,
}

impl ToolCallTracker {
    fn new(
        initial_user_message: &str,
        system: Option<&str>,
        tools: Vec<LlmToolDefinition>,
    ) -> Self {
        let mut messages = Vec::new();
        if let Some(sys) = system {
            messages.push(LlmMessage {
                role: "system".into(),
                content: sys.into(),
                tool_calls: None,
                tool_call_id: None,
                attachments: vec![],
                reasoning: None,
            });
        }
        messages.push(LlmMessage::user(initial_user_message));
        Self { messages, tools }
    }

    fn add_assistant_with_tool_calls(&mut self, tool_calls: Vec<LlmToolCall>) {
        self.messages
            .push(LlmMessage::assistant_with_tool_calls(tool_calls));
    }

    fn add_tool_result(&mut self, tool_call_id: &str, content: &str) {
        self.messages
            .push(LlmMessage::tool_result(tool_call_id, content));
    }

    fn build_request(&self) -> LlmChatRequest {
        LlmChatRequest {
            messages: self.messages.clone(),
            system: None, // System is included in messages
            temperature: Some(0.0),
            max_tokens: Some(300),
            tools: Some(self.tools.clone()),
        }
    }

    fn build_request_no_tools(&self) -> LlmChatRequest {
        LlmChatRequest {
            messages: self.messages.clone(),
            system: None,
            temperature: Some(0.0),
            max_tokens: Some(300),
            tools: None,
        }
    }
}

/// Collect all chunks from a stream until done, returning (text, tool_calls)
async fn collect_stream(
    registry: &LlmStreamRegistry,
    stream_id: u64,
) -> (String, Vec<LlmToolCall>) {
    let mut combined_text = String::new();
    let mut all_tool_calls = Vec::new();

    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        match registry.next_chunk(stream_id) {
            Ok(Some(chunk)) => {
                if let Some(text) = &chunk.text {
                    combined_text.push_str(text);
                }
                all_tool_calls.extend(chunk.tool_calls);
                if chunk.done {
                    break;
                }
            }
            Ok(None) => continue,
            Err(e) => panic!("Stream error: {:?}", e),
        }
    }
    registry.close(stream_id);

    (combined_text, all_tool_calls)
}

#[tokio::test]
#[ignore]
async fn test_multi_round_tool_calling() {
    // This test simulates the real agent loop:
    // User question → Tool Call 1 → Result 1 → Tool Call 2 → Result 2 → Final Response
    //
    // Scenario: User asks about weather in two cities
    // Round 1: LLM calls get_weather for first city
    // Round 2: LLM calls get_weather for second city
    // Round 3: LLM synthesizes both results into final answer

    let api_key = match get_env_key("OPENAI_API_KEY") {
        Some(key) => key,
        None => {
            eprintln!("Skipping: OPENAI_API_KEY not set");
            return;
        }
    };

    let registry = Arc::new(LlmStreamRegistry::new());

    let tools = vec![LlmToolDefinition {
        name: "get_weather".into(),
        description: "Get the current weather in a given location. Call this once per city.".into(),
        parameters: json!({
            "type": "object",
            "properties": {
                "location": {
                    "type": "string",
                    "description": "The city name"
                }
            },
            "required": ["location"]
        }),
    }];

    let mut tracker = ToolCallTracker::new(
        "What's the weather in Tokyo and Paris? Use the get_weather tool for each city separately.",
        Some("You are a helpful assistant. When asked about weather in multiple cities, call the get_weather tool once for each city. Do not try to get weather for multiple cities in a single call."),
        tools,
    );

    let mut round = 0;
    const MAX_ROUNDS: usize = 5;

    loop {
        round += 1;
        if round > MAX_ROUNDS {
            panic!("Too many rounds, possible infinite loop");
        }

        println!(
            "\n=== Round {} (messages: {}) ===",
            round,
            tracker.messages.len()
        );

        // Build request (include tools unless we've done multiple rounds)
        let request = if round <= 3 {
            tracker.build_request()
        } else {
            tracker.build_request_no_tools()
        };

        // Start streaming request
        let stream_id = start_llm_stream(
            ProviderType::OpenAi,
            &api_key,
            "gpt-4o-mini",
            request,
            registry.clone(),
            None,
            None,
        )
        .await
        .expect("Failed to start stream");

        let (text, tool_calls) = collect_stream(&registry, stream_id).await;

        println!(
            "Response text: {}",
            if text.is_empty() { "(empty)" } else { &text }
        );
        println!("Tool calls: {}", tool_calls.len());

        if tool_calls.is_empty() {
            // No more tool calls, we have the final response
            println!("\n=== Final Response ===");
            println!("{}", text);

            // Verify the response mentions both cities
            let text_lower = text.to_lowercase();
            assert!(
                text_lower.contains("tokyo") || text_lower.contains("東京"),
                "Response should mention Tokyo"
            );
            assert!(
                text_lower.contains("paris") || text_lower.contains("巴黎"),
                "Response should mention Paris"
            );

            println!(
                "\n✅ Multi-round tool calling test passed! (completed in {} rounds)",
                round
            );
            return;
        }

        // When model returns multiple tool calls, we need to:
        // 1. Add ONE assistant message with ALL tool calls
        // 2. Add ONE tool result for EACH tool call
        tracker.add_assistant_with_tool_calls(tool_calls.clone());

        // Process each tool call and add results
        for tc in &tool_calls {
            println!(
                "  Tool call: {} (id={}, call_id={:?})",
                tc.name, tc.id, tc.call_id
            );

            let call_id = tc.call_id.as_ref().unwrap_or(&tc.id);
            let location = tc.arguments["location"].as_str().unwrap_or("unknown");

            // Simulate tool execution with fake weather data
            let weather_result = match location.to_lowercase().as_str() {
                l if l.contains("tokyo") => {
                    r#"{"temperature": "18°C", "condition": "Cloudy", "humidity": "60%"}"#
                }
                l if l.contains("paris") => {
                    r#"{"temperature": "12°C", "condition": "Rainy", "humidity": "80%"}"#
                }
                _ => r#"{"temperature": "20°C", "condition": "Unknown", "humidity": "50%"}"#,
            };

            tracker.add_tool_result(call_id, weather_result);
        }
    }
}

/// A minimal 8x8 red PNG image encoded as base64.
/// This is a valid PNG that shows a solid red square.
const TEST_RED_PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAAEklEQVR4nGP4z8CAFWEXHbQSACj/P8Fu7N9hAAAAAElFTkSuQmCC";

/// A minimal 8x8 blue PNG image encoded as base64.
const TEST_BLUE_PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAAgAAAAICAIAAABLbSncAAAAEElEQVR4nGNgYPiPAw0pCQCpcD/BFMrqcwAAAABJRU5ErkJggg==";

#[tokio::test]
#[ignore]
async fn test_openai_image_attachment() {
    // This test verifies that image attachments (base64 encoded) are correctly
    // sent to the LLM and processed.
    let api_key = match get_env_key("OPENAI_API_KEY") {
        Some(key) => key,
        None => {
            eprintln!("Skipping: OPENAI_API_KEY not set");
            return;
        }
    };

    // Create a request with an image attachment
    let request = LlmChatRequest {
        messages: vec![LlmMessage::user_with_attachments(
            "What color is this image? Reply with just the color name.",
            vec![LlmAttachment {
                name: "red_square.png".into(),
                mime_type: "image/png".into(),
                data: TEST_RED_PNG_BASE64.into(),
            }],
        )],
        system: Some(
            "You are a helpful assistant. Be very concise, reply with just a single word.".into(),
        ),
        temperature: Some(0.0),
        max_tokens: Some(50),
        tools: None,
    };

    let response =
        execute_llm_chat(ProviderType::OpenAi, &api_key, "gpt-4o-mini", request, None).await;

    match response {
        Ok(resp) => {
            println!("OpenAI image response: {:?}", resp);
            assert!(!resp.content.is_empty(), "Response should not be empty");
            // The response should mention red (the color of the test image)
            let content_lower = resp.content.to_lowercase();
            assert!(
                content_lower.contains("red")
                    || content_lower.contains("pink")
                    || content_lower.contains("maroon"),
                "Response should describe the red image, got: {}",
                resp.content
            );
            println!("✅ OpenAI image attachment test passed!");
        }
        Err(e) => panic!("OpenAI image chat failed: {:?}", e),
    }
}

#[tokio::test]
#[ignore]
async fn test_openai_image_attachment_streaming() {
    // This test verifies that image attachments work correctly with streaming.
    // This matches the real scenario where the sidebar sends an image via streaming API.
    let api_key = match get_env_key("OPENAI_API_KEY") {
        Some(key) => key,
        None => {
            eprintln!("Skipping: OPENAI_API_KEY not set");
            return;
        }
    };

    let registry = Arc::new(LlmStreamRegistry::new());

    // Create a request that mirrors the agent flow:
    // - System message (like agent mode prompt)
    // - User message with image attachment
    let request = LlmChatRequest {
        messages: vec![
            LlmMessage {
                role: "system".into(),
                content: "You are a powerful AI agent with full system access.".into(),
                tool_calls: None,
                tool_call_id: None,
                attachments: vec![],
                reasoning: None,
            },
            LlmMessage::user_with_attachments(
                "描述一下这个图片", // Same as sidebar test
                vec![LlmAttachment {
                    name: "red_square.png".into(),
                    mime_type: "image/png".into(),
                    data: TEST_RED_PNG_BASE64.into(),
                }],
            ),
        ],
        system: None, // System is in messages, not separate
        temperature: Some(0.0),
        max_tokens: Some(50),
        tools: None,
    };

    println!("Request messages count: {}", request.messages.len());
    for (i, msg) in request.messages.iter().enumerate() {
        println!(
            "Message[{}]: role={}, content_len={}, attachments={}",
            i,
            msg.role,
            msg.content.len(),
            msg.attachments.len()
        );
    }

    let stream_id = start_llm_stream(
        ProviderType::OpenAi,
        &api_key,
        "gpt-4o-mini",
        request,
        registry.clone(),
        None,
        None,
    )
    .await
    .expect("Failed to start stream");

    println!("Stream started with ID: {}", stream_id);

    // Collect response
    let mut combined_text = String::new();
    let mut chunk_count = 0;

    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        match registry.next_chunk(stream_id) {
            Ok(Some(chunk)) => {
                chunk_count += 1;
                if let Some(text) = &chunk.text {
                    combined_text.push_str(text);
                }
                if chunk.done {
                    break;
                }
            }
            Ok(None) => continue,
            Err(e) => panic!("Stream error: {:?}", e),
        }
    }
    registry.close(stream_id);

    println!("Received {} chunks", chunk_count);
    println!("Combined response: {}", combined_text);

    assert!(!combined_text.is_empty(), "Response should not be empty");
    let content_lower = combined_text.to_lowercase();
    // Accept English or Chinese for "red"
    assert!(
        content_lower.contains("red")
            || content_lower.contains("pink")
            || content_lower.contains("maroon")
            || combined_text.contains("红"), // Chinese for red
        "Response should describe the red image, got: {}",
        combined_text
    );

    println!("✅ OpenAI streaming image attachment test passed!");
}

/// A larger 200x200 red PNG image for testing size limits.
const TEST_LARGE_RED_PNG_BASE64: &str = "iVBORw0KGgoAAAANSUhEUgAAAMgAAADICAIAAAAiOjnJAAACcklEQVR4nO3OAQkAMBDEsPNvejPxUCiFCMjelpzjB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1HiB1H60Yes1qIoPaoAAAAASUVORK5CYII=";

#[tokio::test]
#[ignore]
async fn test_openai_image_attachment_with_agent_prompt() {
    // This test mirrors the exact sidebar agent flow:
    // - Full agent system prompt
    // - Image attachment
    // - Tools available
    let api_key = match get_env_key("OPENAI_API_KEY") {
        Some(key) => key,
        None => {
            eprintln!("Skipping: OPENAI_API_KEY not set");
            return;
        }
    };

    let registry = Arc::new(LlmStreamRegistry::new());

    // Use the exact agent system prompt
    let agent_system_prompt = r#"You are a powerful AI agent with full system access.

You can:
- Everything from browser mode
- Read, write, and edit local files
- Execute bash commands
- Use computer control (mouse, keyboard)
- Call MCP servers
- Spawn sub-agents for parallel work

Think step by step. Use tools to gather information before making changes.
Always verify your work after making modifications.
Ask for permission before destructive operations."#;

    let request = LlmChatRequest {
        messages: vec![
            LlmMessage {
                role: "system".into(),
                content: agent_system_prompt.into(),
                tool_calls: None,
                tool_call_id: None,
                attachments: vec![],
                reasoning: None,
            },
            LlmMessage::user_with_attachments(
                "描述一下这个图片",
                vec![LlmAttachment {
                    name: "screenshot.png".into(),
                    mime_type: "image/png".into(),
                    data: TEST_LARGE_RED_PNG_BASE64.into(),
                }],
            ),
        ],
        system: None,
        temperature: Some(0.0),
        max_tokens: Some(200),
        // Add tools like the agent mode does
        tools: Some(vec![LlmToolDefinition {
            name: "read_file".into(),
            description: "Read a file from the filesystem".into(),
            parameters: json!({
                "type": "object",
                "properties": {
                    "path": {"type": "string", "description": "File path to read"}
                },
                "required": ["path"]
            }),
        }]),
    };

    println!("Request messages count: {}", request.messages.len());
    println!("System prompt length: {}", agent_system_prompt.len());
    println!("Image data length: {}", TEST_LARGE_RED_PNG_BASE64.len());

    let stream_id = start_llm_stream(
        ProviderType::OpenAi,
        &api_key,
        "gpt-4o-mini",
        request,
        registry.clone(),
        None,
        None,
    )
    .await
    .expect("Failed to start stream");

    let mut combined_text = String::new();
    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        match registry.next_chunk(stream_id) {
            Ok(Some(chunk)) => {
                if let Some(text) = &chunk.text {
                    combined_text.push_str(text);
                }
                if chunk.done {
                    break;
                }
            }
            Ok(None) => continue,
            Err(e) => panic!("Stream error: {:?}", e),
        }
    }
    registry.close(stream_id);

    println!("Response: {}", combined_text);
    assert!(!combined_text.is_empty(), "Response should not be empty");

    // Should describe the image, not say "I cannot view"
    assert!(
        !combined_text.contains("无法查看")
            && !combined_text.contains("cannot view")
            && !combined_text.contains("can't see"),
        "Model should be able to see the image! Got: {}",
        combined_text
    );

    // Should mention red color
    assert!(
        combined_text.contains("红") || combined_text.to_lowercase().contains("red"),
        "Response should describe the red image, got: {}",
        combined_text
    );

    println!("✅ OpenAI image with agent prompt test passed!");
}

#[tokio::test]
#[ignore]
async fn test_anthropic_image_attachment() {
    // Test image attachments with Anthropic Claude
    let api_key = match get_env_key("ANTHROPIC_API_KEY") {
        Some(key) => key,
        None => {
            eprintln!("Skipping: ANTHROPIC_API_KEY not set");
            return;
        }
    };

    let request = LlmChatRequest {
        messages: vec![LlmMessage::user_with_attachments(
            "What color is this image? Reply with just the color name.",
            vec![LlmAttachment {
                name: "red_square.png".into(),
                mime_type: "image/png".into(),
                data: TEST_RED_PNG_BASE64.into(),
            }],
        )],
        system: Some(
            "You are a helpful assistant. Be very concise, reply with just a single word.".into(),
        ),
        temperature: Some(0.0),
        max_tokens: Some(50),
        tools: None,
    };

    let response = execute_llm_chat(
        ProviderType::Anthropic,
        &api_key,
        "claude-3-haiku-20240307",
        request,
        None,
    )
    .await;

    match response {
        Ok(resp) => {
            println!("Anthropic image response: {:?}", resp);
            assert!(!resp.content.is_empty(), "Response should not be empty");
            let content_lower = resp.content.to_lowercase();
            assert!(
                content_lower.contains("red")
                    || content_lower.contains("pink")
                    || content_lower.contains("maroon"),
                "Response should describe the red image, got: {}",
                resp.content
            );
            println!("✅ Anthropic image attachment test passed!");
        }
        Err(e) => panic!("Anthropic image chat failed: {:?}", e),
    }
}

/// Test with a large screenshot-like image (400x300, ~160KB base64)
/// This test aims to reproduce the sidebar issue where large screenshots
/// were not being described by gpt-4o-mini.
#[tokio::test]
#[ignore]
async fn test_openai_large_screenshot_image() {
    let api_key = match get_env_key("OPENAI_API_KEY") {
        Some(key) => key,
        None => {
            eprintln!("Skipping: OPENAI_API_KEY not set");
            return;
        }
    };

    // Try larger image first (800x600, ~937KB), fallback to 400x300
    let screenshot_base64 = match std::fs::read_to_string("/tmp/test_screenshot_800x600.txt") {
        Ok(data) => {
            println!("Using 800x600 screenshot (~937KB base64)");
            data
        }
        Err(_) => match std::fs::read_to_string("/tmp/test_screenshot_400x300.txt") {
            Ok(data) => {
                println!("Using 400x300 screenshot (~160KB base64)");
                data
            }
            Err(_) => {
                eprintln!("Skipping: No test screenshot files found");
                return;
            }
        },
    };

    println!(
        "Testing with large screenshot image: {} bytes base64",
        screenshot_base64.len()
    );

    let registry = Arc::new(LlmStreamRegistry::new());

    // Test with gpt-4o-mini (same as sidebar)
    let request = LlmChatRequest {
        messages: vec![
            LlmMessage {
                role: "system".into(),
                content: "You are a helpful assistant.".into(),
                tool_calls: None,
                tool_call_id: None,
                attachments: vec![],
                reasoning: None,
            },
            LlmMessage::user_with_attachments(
                "描述一下这个图片",
                vec![LlmAttachment {
                    name: "screenshot.png".into(),
                    mime_type: "image/png".into(),
                    data: screenshot_base64.clone(),
                }],
            ),
        ],
        system: None,
        temperature: Some(0.0),
        max_tokens: Some(200),
        tools: None,
    };

    let stream_id = start_llm_stream(
        ProviderType::OpenAi,
        &api_key,
        "gpt-4o-mini",
        request,
        registry.clone(),
        None,
        None,
    )
    .await
    .expect("Failed to start stream");

    let mut combined_text = String::new();
    loop {
        tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
        match registry.next_chunk(stream_id) {
            Ok(Some(chunk)) => {
                if let Some(text) = &chunk.text {
                    combined_text.push_str(text);
                }
                if chunk.done {
                    break;
                }
            }
            Ok(None) => continue,
            Err(e) => panic!("Stream error: {:?}", e),
        }
    }
    registry.close(stream_id);

    println!(
        "gpt-4o-mini response for large screenshot: {}",
        combined_text
    );

    // The response should NOT say "cannot view" or similar
    let has_error_response = combined_text.contains("无法查看")
        || combined_text.contains("cannot view")
        || combined_text.contains("can't see")
        || combined_text.contains("unable to")
        || combined_text.contains("请上传"); // "please upload"

    if has_error_response {
        println!("⚠️ gpt-4o-mini failed to process large screenshot!");
        println!("Response: {}", combined_text);

        // Try with gpt-4o
        println!("\nRetrying with gpt-4o...");
        let request_gpt4o = LlmChatRequest {
            messages: vec![
                LlmMessage {
                    role: "system".into(),
                    content: "You are a helpful assistant.".into(),
                    tool_calls: None,
                    tool_call_id: None,
                    attachments: vec![],
                    reasoning: None,
                },
                LlmMessage::user_with_attachments(
                    "描述一下这个图片",
                    vec![LlmAttachment {
                        name: "screenshot.png".into(),
                        mime_type: "image/png".into(),
                        data: screenshot_base64,
                    }],
                ),
            ],
            system: None,
            temperature: Some(0.0),
            max_tokens: Some(200),
            tools: None,
        };

        let stream_id2 = start_llm_stream(
            ProviderType::OpenAi,
            &api_key,
            "gpt-4o",
            request_gpt4o,
            registry.clone(),
            None,
            None,
        )
        .await
        .expect("Failed to start stream with gpt-4o");

        let mut gpt4o_response = String::new();
        loop {
            tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            match registry.next_chunk(stream_id2) {
                Ok(Some(chunk)) => {
                    if let Some(text) = &chunk.text {
                        gpt4o_response.push_str(text);
                    }
                    if chunk.done {
                        break;
                    }
                }
                Ok(None) => continue,
                Err(e) => panic!("Stream error: {:?}", e),
            }
        }
        registry.close(stream_id2);

        println!("gpt-4o response: {}", gpt4o_response);

        // gpt-4o should work
        assert!(
            !gpt4o_response.contains("无法查看") && !gpt4o_response.contains("cannot view"),
            "Even gpt-4o failed to view the image!"
        );
    }

    assert!(!combined_text.is_empty(), "Response should not be empty");

    println!("✅ Large screenshot image test completed");
}

#[tokio::test]
#[ignore]
async fn test_multiple_image_attachments() {
    // Test sending multiple images in a single message
    let api_key = match get_env_key("OPENAI_API_KEY") {
        Some(key) => key,
        None => {
            eprintln!("Skipping: OPENAI_API_KEY not set");
            return;
        }
    };

    let request = LlmChatRequest {
        messages: vec![LlmMessage::user_with_attachments(
            "I'm showing you two images. What colors do you see? List both colors.",
            vec![
                LlmAttachment {
                    name: "red_square.png".into(),
                    mime_type: "image/png".into(),
                    data: TEST_RED_PNG_BASE64.into(),
                },
                LlmAttachment {
                    name: "blue_square.png".into(),
                    mime_type: "image/png".into(),
                    data: TEST_BLUE_PNG_BASE64.into(),
                },
            ],
        )],
        system: Some("You are a helpful assistant. Be concise.".into()),
        temperature: Some(0.0),
        max_tokens: Some(100),
        tools: None,
    };

    let response =
        execute_llm_chat(ProviderType::OpenAi, &api_key, "gpt-4o-mini", request, None).await;

    match response {
        Ok(resp) => {
            println!("Multiple images response: {:?}", resp);
            assert!(!resp.content.is_empty(), "Response should not be empty");
            let content_lower = resp.content.to_lowercase();
            // Should mention both colors
            assert!(
                content_lower.contains("red") || content_lower.contains("pink"),
                "Response should mention red, got: {}",
                resp.content
            );
            assert!(
                content_lower.contains("blue"),
                "Response should mention blue, got: {}",
                resp.content
            );
            println!("✅ Multiple image attachments test passed!");
        }
        Err(e) => panic!("Multiple images chat failed: {:?}", e),
    }
}

// ==================== Custom base_url (OpenAI-compatible) tests ====================

#[tokio::test]
#[ignore]
async fn test_openai_custom_base_url_chat() {
    let api_key = match get_env_key("XFYUN_API_KEY") {
        Some(key) => key,
        None => {
            eprintln!("Skipping: XFYUN_API_KEY not set");
            return;
        }
    };

    let request = LlmChatRequest {
        messages: vec![LlmMessage::user("Say 'hello' and nothing else.")],
        system: Some("You are a helpful assistant. Be very concise.".into()),
        temperature: Some(0.0),
        max_tokens: Some(50),
        tools: None,
    };

    let base_url = Some("https://maas-coding-api.cn-huabei-1.xf-yun.com/v2");

    let response = execute_llm_chat(
        ProviderType::OpenAi,
        &api_key,
        "astron-code-latest",
        request,
        base_url,
    )
    .await;

    match response {
        Ok(resp) => {
            println!("xfyun non-streaming response: {:?}", resp);
            assert!(!resp.content.is_empty(), "Response should not be empty");
            println!("✅ Custom base_url non-streaming test passed!");
        }
        Err(e) => panic!("Custom base_url chat failed: {:?}", e),
    }
}

#[tokio::test]
#[ignore]
async fn test_openai_custom_base_url_streaming() {
    let api_key = match get_env_key("XFYUN_API_KEY") {
        Some(key) => key,
        None => {
            eprintln!("Skipping: XFYUN_API_KEY not set");
            return;
        }
    };

    let request = LlmChatRequest {
        messages: vec![LlmMessage::user("Say 'hello' and nothing else.")],
        system: Some("You are a helpful assistant. Be very concise.".into()),
        temperature: Some(0.0),
        max_tokens: Some(50),
        tools: None,
    };

    let registry = Arc::new(LlmStreamRegistry::new());
    let base_url = Some("https://maas-coding-api.cn-huabei-1.xf-yun.com/v2");

    let stream_id = start_llm_stream(
        ProviderType::OpenAi,
        &api_key,
        "astron-code-latest",
        request,
        registry.clone(),
        base_url,
        None,
    )
    .await;

    match stream_id {
        Ok(id) => {
            println!("xfyun streaming started with id: {}", id);
            let mut full_text = String::new();
            let mut done = false;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
            while std::time::Instant::now() < deadline {
                match registry.next_chunk(id) {
                    Ok(Some(chunk)) => {
                        if let Some(text) = &chunk.text {
                            full_text.push_str(text);
                        }
                        if chunk.done {
                            done = true;
                            break;
                        }
                    }
                    Ok(None) => {
                        // No chunk yet, wait a bit
                        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    }
                    Err(e) => panic!("next_chunk error: {:?}", e),
                }
            }
            assert!(done, "Stream should complete");
            assert!(!full_text.is_empty(), "Should receive text content");
            println!("xfyun streaming response: {}", full_text);
            println!("✅ Custom base_url streaming test passed!");
        }
        Err(e) => panic!("Custom base_url streaming failed: {:?}", e),
    }
}

// ==================== Image generation tests ====================

#[tokio::test]
#[ignore]
async fn test_openrouter_image_generation() {
    let api_key = match get_env_key("OPENROUTER_API_KEY") {
        Some(key) => key,
        None => {
            eprintln!("Skipping: OPENROUTER_API_KEY not set");
            return;
        }
    };

    let request = LlmChatRequest {
        messages: vec![LlmMessage::user(
            "Generate a small red circle on a white background",
        )],
        system: None,
        temperature: None,
        max_tokens: Some(2048),
        tools: None,
    };

    let response = execute_llm_chat(
        ProviderType::OpenRouter,
        &api_key,
        "google/gemini-3.1-flash-image-preview",
        request,
        None,
    )
    .await;

    match response {
        Ok(resp) => {
            println!(
                "Image generation response: content_len={}",
                resp.content.len()
            );
            println!("Image count: {}", resp.images.len());
            assert!(!resp.images.is_empty(), "Should contain at least one image");
            let img = &resp.images[0];
            assert_eq!(img.media_type, "image/png");
            assert!(!img.data.is_empty(), "Image data should not be empty");
            println!(
                "Image: media_type={}, data_len={}",
                img.media_type,
                img.data.len()
            );
            println!("✅ Image generation test passed!");
        }
        Err(e) => panic!("Image generation failed: {:?}", e),
    }
}

#[tokio::test]
#[ignore]
async fn test_openrouter_image_generation_streaming() {
    let api_key = match get_env_key("OPENROUTER_API_KEY") {
        Some(key) => key,
        None => {
            eprintln!("Skipping: OPENROUTER_API_KEY not set");
            return;
        }
    };

    let request = LlmChatRequest {
        messages: vec![LlmMessage::user(
            "Generate a small blue square on a white background",
        )],
        system: None,
        temperature: None,
        max_tokens: Some(2048),
        tools: None,
    };

    let registry = Arc::new(LlmStreamRegistry::new());

    let stream_id = start_llm_stream(
        ProviderType::OpenRouter,
        &api_key,
        "google/gemini-3.1-flash-image-preview",
        request,
        registry.clone(),
        None,
        None,
    )
    .await;

    match stream_id {
        Ok(id) => {
            println!("Image stream started with id: {}", id);
            let mut full_text = String::new();
            let mut image_count = 0;
            let mut done = false;
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
            while std::time::Instant::now() < deadline {
                match registry.next_chunk(id) {
                    Ok(Some(chunk)) => {
                        if let Some(text) = &chunk.text {
                            full_text.push_str(text);
                        }
                        image_count += chunk.images.len();
                        if chunk.done {
                            done = true;
                            break;
                        }
                    }
                    Ok(None) => {
                        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                    }
                    Err(e) => panic!("next_chunk error: {:?}", e),
                }
            }
            assert!(done, "Stream should complete");
            assert!(image_count > 0, "Should receive at least one image");
            println!(
                "Image stream: text_len={}, images={}",
                full_text.len(),
                image_count
            );
            println!("✅ Image generation streaming test passed!");
        }
        Err(e) => panic!("Image generation streaming failed: {:?}", e),
    }
}
