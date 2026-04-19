/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Handles incoming messages from background script
//!
//! Receives ChatMessage from Agent (via background.js) and InternalMessage
//! from background.js itself.

use dioxus::prelude::*;
use crate::context::AppContext;
use crate::messaging::bridge::*;
use crate::messaging::sender::*;
use crate::state::*;
use shared_protocol::*;
use wasm_bindgen::prelude::*;
use wasm_bindgen::closure::Closure;
use wasm_bindgen_futures::spawn_local;

/// Combined incoming message type
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(untagged)]
pub enum IncomingMessage {
    /// Agent protocol message (Chat channel - bidirectional)
    AgentMessage(ChatMessage),
    /// Extension internal message
    Internal(InternalMessage),
}

/// Initialize message listener
pub fn init_message_listener(ctx: AppContext) {
    let closure = Closure::<dyn Fn(JsValue, JsValue, JsValue) -> JsValue>::new(
        move |msg: JsValue, _sender: JsValue, _send_response: JsValue| {
            // Parse incoming message
            match from_js_value::<IncomingMessage>(msg.clone()) {
                Ok(incoming) => {
                    handle_incoming(ctx, incoming);
                }
                Err(e) => {
                    // Log raw message for debugging
                    let raw = js_sys::JSON::stringify(&msg)
                        .map(|s| s.as_string().unwrap_or_default())
                        .unwrap_or_else(|_| "???".to_string());
                    let truncated = if raw.len() > 300 { format!("{}...", &raw[..300]) } else { raw };
                    web_sys::console::warn_1(
                        &format!("[WASM] Failed to parse message: {} | raw: {}", e, truncated).into()
                    );
                }
            }

            // Return undefined (no async response)
            JsValue::UNDEFINED
        },
    );

    runtime_add_listener(&closure);

    // Keep closure alive for lifetime of app
    closure.forget();
}

/// Route incoming message to appropriate handler
fn handle_incoming(ctx: AppContext, message: IncomingMessage) {
    match message {
        IncomingMessage::AgentMessage(chat_msg) => {
            handle_chat_message(ctx, chat_msg);
        }
        IncomingMessage::Internal(internal) => {
            handle_internal_message(ctx, internal);
        }
    }
}

/// Handle Chat channel messages (bidirectional Agent <-> Sidebar)
///
/// Most messages from Agent are ToSidebar direction, but we also handle
/// BrowserToolRequest specially by executing via bg:exec_tool and sending response.
fn handle_chat_message(ctx: AppContext, message: ChatMessage) {
    match message {
        // ========== Agent -> Sidebar messages ==========
        ChatMessage::StreamChunk(payload) => {
            handle_stream_chunk(ctx, payload);
        }
        ChatMessage::StreamEnd(payload) => {
            handle_stream_end(ctx, payload);
        }
        ChatMessage::AgentState(payload) => {
            handle_agent_state(ctx, payload);
        }
        ChatMessage::PermissionRequest(payload) => {
            handle_permission_request(ctx, payload);
        }
        ChatMessage::Error(payload) => {
            handle_error(ctx, payload);
        }
        ChatMessage::ContentBlock(payload) => {
            handle_content_block(ctx, payload);
        }
        ChatMessage::AccountStatus(_payload) => {
            // P2: Handle account status
            tracing::debug!("Received account status - not yet implemented");
        }
        ChatMessage::SystemResponse(payload) => {
            handle_system_response(ctx, payload);
        }
        ChatMessage::BrowserToolRequest(payload) => {
            // Execute browser tool via bg:exec_tool and send response back to agent
            handle_browser_tool_request(payload);
        }
        ChatMessage::PickFilesResponse(_payload) => {
            // Not used - file.pick responses come via SystemResponse
            tracing::debug!("Received PickFilesResponse - using SystemResponse instead");
        }
        ChatMessage::PlanProposal(payload) => {
            handle_plan_proposal(ctx, payload);
        }

        // ========== EventBus messages ==========
        ChatMessage::EventsResponse(response) => {
            tracing::info!("[Sidebar] EventBus response: {:?}", response);
            match response {
                shared_protocol::EventBusResponse::Error { code, message } => {
                    tracing::warn!("[Sidebar] EventBus error: {} - {}", code, message);
                }
                _ => {}
            }
        }

        ChatMessage::EventsDelivery(delivery) => {
            tracing::info!(
                "[Sidebar] EventBus delivery: sub={}, topic={}",
                delivery.subscription_id, delivery.event.topic
            );
            handle_event_delivery(ctx, delivery);
        }

        ChatMessage::EventsRequest(_) => {
            tracing::warn!("[Sidebar] Received EventsRequest (unexpected direction)");
        }

        // ========== Sidebar -> Agent messages (should not be received) ==========
        ChatMessage::ChatMessage(_) |
        ChatMessage::SkillCommand(_) |
        ChatMessage::StopGeneration(_) |
        ChatMessage::Cancel(_) |
        ChatMessage::PermissionResponse(_) |
        ChatMessage::PluginCommand(_) |
        ChatMessage::SystemCommand(_) |
        ChatMessage::BrowserToolResponse(_) |
        ChatMessage::PickFilesRequest(_) |
        ChatMessage::PlanResponse(_) |
        ChatMessage::ToolAuthResponse(_) => {
            tracing::warn!("Received unexpected ToAgent message in sidebar");
        }
    }
}

/// Handle BrowserToolRequest from Agent
///
/// 1. Call bg:exec_tool to execute the browser tool
/// 2. Build response payload from result
/// 3. Send response via bg:send_to_agent
fn handle_browser_tool_request(payload: BrowserToolRequestPayload) {
    tracing::info!(
        "Executing browser tool: {} (action={:?}, tab_id={:?})",
        payload.request_id,
        payload.action,
        payload.tab_id
    );

    // Use spawn_local for better error handling in WASM context
    // Clone all data needed for the async block upfront
    let request_id = payload.request_id.clone();
    let session_id = payload.session_id.clone();
    let action = payload.action.clone();
    let params = payload.params.clone();
    let tab_id = payload.tab_id;
    let timeout_ms = payload.timeout_ms;

    tracing::debug!("Spawning async task for browser tool...");

    spawn_local(async move {
        tracing::debug!("Async task started for browser tool: {}", request_id);

        // Reconstruct payload inside async block to avoid closure capture issues
        let payload = BrowserToolRequestPayload {
            request_id: request_id.clone(),
            session_id: session_id.clone(),
            action,
            params,
            tab_id,
            timeout_ms,
        };

        execute_browser_tool_and_respond(payload, request_id, session_id).await;
    });

    tracing::debug!("Browser tool task spawned");
}

/// Execute browser tool and send response - isolated to prevent panic propagation
async fn execute_browser_tool_and_respond(
    payload: BrowserToolRequestPayload,
    request_id: String,
    session_id: String,
) {
    tracing::debug!("execute_browser_tool_and_respond starting...");

    // Execute browser tool via bg:exec_tool
    let result = crate::messaging::exec_browser_tool(payload).await;

    tracing::debug!("exec_browser_tool returned");

    match result {
        Ok(response) => {
            tracing::info!(
                "Browser tool {} completed (success={})",
                response.request_id,
                response.success
            );

            // Send response back to agent via bg:send_to_agent
            if let Err(e) = crate::messaging::send_browser_tool_response(response).await {
                tracing::error!("Failed to send browser tool response to agent: {}", e);
            }
        }
        Err(e) => {
            tracing::error!("Browser tool execution failed: {}", e);

            // Send error response back to agent
            let error_response = BrowserToolResponsePayload {
                request_id,
                session_id,
                success: false,
                result: None,
                error: Some(BrowserToolError {
                    code: -1,
                    message: e,
                    recoverable: true,
                }),
            };

            if let Err(e) = crate::messaging::send_browser_tool_response(error_response).await {
                tracing::error!("Failed to send browser tool error response: {}", e);
            }
        }
    }

    tracing::debug!("execute_browser_tool_and_respond completed");
}

// ============================================
// EventBus Handlers
// ============================================

fn handle_event_delivery(mut ctx: AppContext, delivery: shared_protocol::EventBusDelivery) {
    let topic = &delivery.event.topic;

    if topic.contains(":notification") {
        let title = delivery.event.payload
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("Notification")
            .to_string();
        let body = delivery.event.payload
            .get("body")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();

        let mut notifications = ctx.event_notifications.write();
        // Dedupe by event_id — the same event may be delivered via multiple
        // subscriptions (e.g. stale subscriptions left on daemon side after
        // proxy reconnects), which would yield duplicate Dioxus keys and
        // panic the diff engine.
        if notifications.iter().any(|n| n.id == delivery.event.event_id) {
            return;
        }
        notifications.push(crate::context::EventNotification {
            id: delivery.event.event_id.clone(),
            title,
            body,
            topic: topic.clone(),
            timestamp_ms: delivery.event.timestamp_ms,
        });
        let len = notifications.len();
        if len > 20 {
            notifications.drain(0..len - 20);
        }
    }
}

// ============================================
// Stream Handlers
// ============================================

fn handle_stream_chunk(mut ctx: AppContext, payload: StreamChunkPayload) {
    // Capture session title if provided (generated from first message)
    if let Some(ref title) = payload.session_title {
        tracing::info!("Received session title: {}", title);
        ctx.session.write().set_title(title.clone());

        // Refresh history to show the new title
        ctx.history.write().set_loading();
        spawn_local(async move {
            let _ = crate::messaging::send_session_list(50, 0).await;
        });
    }

    // Process tool event if present
    if let Some(event) = payload.event {
        handle_tool_event(ctx, event);
    }

    // Process thinking event if present
    if let Some(thinking_event) = payload.thinking_event {
        handle_thinking_event(ctx, thinking_event);
    }

    // New protocol: content + done flag (no stream_id needed)
    if payload.done {
        // Stream complete - finalize accumulated content and tool_calls into a message
        let (final_content, accumulated_tool_calls) = {
            let mut streaming = ctx.streaming.write();
            if let Some(mut stream) = streaming.take() {
                // Append (or replace) final content from the done payload
                if !payload.content.is_empty() {
                    if payload.replace_content {
                        stream.content = payload.content.clone();
                    } else {
                        stream.content.push_str(&payload.content);
                    }
                }
                // Also accumulate any tool_calls from the done payload
                if !payload.tool_calls.is_empty() {
                    stream.accumulate_tool_calls(&payload.tool_calls);
                }
                (stream.content, stream.tool_calls)
            } else {
                // No streaming state - use payload content directly
                (payload.content.clone(), payload.tool_calls.clone())
            }
        };

        // Drain live_tools into final ToolCallData, merging with accumulated tool_calls
        let live_tool_data: Vec<ToolCallData> = ctx.live_tools.write().drain(..).map(|entry| {
            let status = match entry.status {
                LiveToolStatus::Success => Some(ToolCallStatus::Success),
                LiveToolStatus::Failed => Some(ToolCallStatus::Failed),
                _ => None,
            };
            ToolCallData {
                id: entry.id,
                name: entry.name,
                icon: entry.icon,
                display_target: Some(entry.summary),
                arguments: String::new(),
                duration_ms: entry.duration_ms,
                status,
                kind: entry.kind,
            }
        }).collect();

        // Convert accumulated tool_calls to UI data (fallback for tools without events)
        let payload_tool_calls: Vec<ToolCallData> = accumulated_tool_calls.iter()
            .filter(|tc| !live_tool_data.iter().any(|lt| lt.id == tc.id))
            .map(|tc| {
                ToolCallData {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    icon: get_tool_icon(&tc.name).to_string(),
                    display_target: extract_tool_target(&tc.name, &tc.arguments),
                    arguments: tc.arguments.clone(),
                    duration_ms: None,
                    status: None,
                    kind: ActivityKind::Tool,
                }
            }).collect();

        // Merge: live_tools first (they have duration/status), then any remaining payload tools
        let mut tool_calls = live_tool_data;
        tool_calls.extend(payload_tool_calls);

        // Clear pending tool auth
        ctx.pending_tool_auth.set(None);

        // Strip tool XML artifacts and apply narration filter
        let stripped = strip_tool_xml(&final_content);
        let display_content = if tool_calls.is_empty() {
            stripped
        } else {
            let tool_names: Vec<String> = tool_calls.iter()
                .map(|tc| tc.name.clone()).collect();
            filter_narration(&stripped, &tool_names)
        };

        // Add message if there's content or tool calls.
        // When we have tool_calls but no text, try to merge into the previous
        // assistant message so the ActivityFeed stays visually unified with
        // the text rather than appearing as a separate empty bubble below.
        web_sys::console::log_1(&format!(
            "[WASM] stream done: content_len={}, tool_calls={}",
            display_content.len(), tool_calls.len()
        ).into());

        if !display_content.is_empty() || !tool_calls.is_empty() {
            if display_content.is_empty() && !tool_calls.is_empty() {
                // Merge tool_calls into the last assistant message in-place.
                // Using write() ensures Dioxus detects the mutation when
                // the guard drops (more reliable than read/clone/set).
                web_sys::console::log_1(&format!(
                    "[WASM] MERGE branch: merging {} tool_calls into prev assistant",
                    tool_calls.len()
                ).into());
                {
                    let mut msgs = ctx.messages.write();
                    // Only merge into Markdown messages — skip Artifact/Plan
                    // cards which are rendered by ArtifactCard/PlanCard
                    // (they don't have ActivityFeed).
                    if let Some(last_assistant) = msgs.iter_mut().rev()
                        .find(|m| m.role == MessageRole::Assistant
                            && matches!(m.content, MessageContent::Markdown(_)))
                    {
                        last_assistant.tool_calls.extend(tool_calls);
                        last_assistant.is_live = true;
                        // Change ID so Dioxus sees a different key and
                        // creates a fresh MessageBubble (bypasses memoization).
                        last_assistant.id = uuid::Uuid::new_v4().to_string();
                        web_sys::console::log_1(&format!(
                            "[WASM] MERGE done: new id={}, total tool_calls={}",
                            last_assistant.id, last_assistant.tool_calls.len()
                        ).into());
                    } else {
                        web_sys::console::log_1(&"[WASM] MERGE: no prev assistant, creating new msg".into());
                        msgs.push(Message::assistant_with_activity(display_content, tool_calls).set_live());
                    }
                    // write guard drops here → Dioxus marks signal dirty
                }
            } else {
                let tc_count = tool_calls.len();
                let has_content = !display_content.is_empty();
                let message = if tool_calls.is_empty() {
                    Message::assistant_markdown(display_content).set_live()
                } else {
                    Message::assistant_with_activity(display_content, tool_calls).set_live()
                };
                web_sys::console::log_1(&format!(
                    "[WASM] NEW msg: has_content={}, tool_calls={}",
                    has_content, tc_count
                ).into());
                ctx.messages.write().push(message);
            }
        }

        // Clear agent status
        ctx.agent_status.write().hide();
    } else {
        // Streaming in progress - accumulate content and tool_calls
        let has_content = !payload.content.is_empty();
        let has_tool_calls = !payload.tool_calls.is_empty();
        if has_content || has_tool_calls {
            let mut streaming = ctx.streaming.write();
            match &mut *streaming {
                Some(ref mut stream) => {
                    if has_content {
                        if payload.replace_content {
                            stream.content = payload.content;
                        } else {
                            stream.content.push_str(&payload.content);
                        }
                    }
                    if has_tool_calls {
                        stream.accumulate_tool_calls(&payload.tool_calls);
                    }
                }
                None => {
                    // Start new stream
                    let mut state = StreamingState {
                        id: uuid::Uuid::new_v4().to_string(),
                        content: payload.content,
                        format: StreamFormat::Markdown,
                        tool_calls: Vec::new(),
                    };
                    if has_tool_calls {
                        state.accumulate_tool_calls(&payload.tool_calls);
                    }
                    *streaming = Some(state);
                }
            }
        }
    }
}

/// Handle real-time tool execution events
fn handle_tool_event(mut ctx: AppContext, event: shared_protocol::ToolEvent) {
    use shared_protocol::ToolEvent;

    match event {
        ToolEvent::Start { tool_id, tool_name, icon, summary } => {
            tracing::debug!("Tool started: {} ({})", tool_name, tool_id);
            ctx.live_tools.write().push(LiveToolEntry {
                id: tool_id,
                name: tool_name,
                icon,
                summary,
                status: LiveToolStatus::Running,
                duration_ms: None,
                kind: ActivityKind::Tool,
            });
        }
        ToolEvent::Auth { tool_id, request } => {
            tracing::info!("Tool auth requested: {} ({})", request.tool, tool_id);
            // Update matching live tool entry status
            ctx.live_tools.with_mut(|tools| {
                if let Some(entry) = tools.iter_mut().find(|t| t.id == tool_id) {
                    entry.status = LiveToolStatus::WaitingAuth(request.clone());
                }
            });
            // Set pending auth for dialog
            ctx.pending_tool_auth.set(Some(request));
        }
        ToolEvent::End { tool_id, status, duration_ms, summary } => {
            tracing::debug!("Tool ended: {} ({}, {}ms)", tool_id, summary, duration_ms);
            ctx.live_tools.with_mut(|tools| {
                if let Some(entry) = tools.iter_mut().find(|t| t.id == tool_id) {
                    entry.status = match status {
                        shared_protocol::ToolEventStatus::Success => LiveToolStatus::Success,
                        shared_protocol::ToolEventStatus::Failed => LiveToolStatus::Failed,
                    };
                    entry.duration_ms = Some(duration_ms);
                    entry.summary = summary;
                }
            });
            // Clear pending auth if this tool was waiting
            ctx.pending_tool_auth.with_mut(|auth| {
                if let Some(req) = auth.as_ref() {
                    if req.tool_id == tool_id {
                        *auth = None;
                    }
                }
            });
        }
    }
}

/// Handle thinking/reasoning events from the agent stream
fn handle_thinking_event(mut ctx: AppContext, event: shared_protocol::ThinkingEvent) {
    use shared_protocol::ThinkingEvent;

    match event {
        ThinkingEvent::Start { thinking_id } => {
            tracing::debug!("Thinking started: {}", thinking_id);
            ctx.live_tools.write().push(LiveToolEntry {
                id: thinking_id,
                name: "Thinking".into(),
                icon: "\u{1F4AD}".into(),
                summary: "Reasoning...".into(),
                status: LiveToolStatus::Running,
                duration_ms: None,
                kind: ActivityKind::Thinking { content: String::new() },
            });
        }
        ThinkingEvent::Delta { thinking_id, content } => {
            let mut tools = ctx.live_tools.write();
            if let Some(entry) = tools.iter_mut().find(|e| e.id == thinking_id) {
                if let ActivityKind::Thinking { content: ref mut c } = entry.kind {
                    c.push_str(&content);
                    // Update summary to first ~50 chars (single pass)
                    let mut chars = c.chars();
                    let summary: String = chars.by_ref().take(50).collect();
                    let has_more = chars.next().is_some();
                    entry.summary = if has_more {
                        format!("{}...", summary)
                    } else {
                        summary
                    };
                }
            }
        }
        ThinkingEvent::End { thinking_id, duration_ms } => {
            tracing::debug!("Thinking ended: {}", thinking_id);
            ctx.live_tools.with_mut(|tools| {
                if let Some(entry) = tools.iter_mut().find(|e| e.id == thinking_id) {
                    entry.status = LiveToolStatus::Success;
                    entry.duration_ms = duration_ms;
                }
            });
        }
    }
}

/// Filter narration paragraphs from accumulated text.
///
/// Heuristic: paragraphs that start with narration patterns AND mention
/// a tool name are considered narration and removed. If everything would
/// be filtered, the original text is returned as a safety net.
fn filter_narration(text: &str, tool_names: &[String]) -> String {
    let paragraphs: Vec<&str> = text.split("\n\n").collect();
    let mut answer_parts = Vec::new();

    let narration_starters = [
        "Let me ", "I'll ", "I need to ", "I'm going to ",
        "First, let me ", "Now let me ", "Now I'll ",
    ];

    for para in &paragraphs {
        let trimmed = para.trim();
        if trimmed.is_empty() {
            continue;
        }

        let starts_with_narration = narration_starters.iter()
            .any(|s| trimmed.starts_with(s));
        let mentions_tool = tool_names.iter()
            .any(|t| trimmed.contains(t.as_str()));

        // Only filter if BOTH conditions: narration pattern AND tool mention
        if starts_with_narration && mentions_tool {
            continue;
        }

        answer_parts.push(*para);
    }

    // If everything was filtered, return original (safety net)
    if answer_parts.is_empty() {
        return text.to_string();
    }

    answer_parts.join("\n\n")
}

/// Strip `<tool_result>…</tool_result>` and `<tool_call>…</tool_call>` XML blocks
/// from assistant text. These are agent protocol artifacts that should not be
/// displayed to users.
fn strip_tool_xml(text: &str) -> String {
    let mut result = text.to_string();
    for tag in &["tool_result", "tool_call"] {
        loop {
            let open = format!("<{}", tag);
            let close = format!("</{}>", tag);
            if let Some(start) = result.find(&open) {
                if let Some(end_pos) = result[start..].find(&close) {
                    let end = start + end_pos + close.len();
                    result.replace_range(start..end, "");
                    continue;
                }
            }
            break;
        }
    }
    result
}

fn handle_stream_end(mut ctx: AppContext, payload: StreamEndPayload) {
    // Finalize stream into message
    let final_content = {
        let mut streaming = ctx.streaming.write();
        if let Some(stream) = streaming.take() {
            if stream.id == payload.stream_id {
                Some((stream.content, stream.format))
            } else {
                None
            }
        } else {
            None
        }
    };

    if let Some((content, format)) = final_content {
        let message = match format {
            StreamFormat::Markdown => Message::assistant_markdown(content).set_live(),
            _ => Message::assistant(content).set_live(),
        };

        ctx.messages.write().push(message);
    }
}

// ============================================
// Agent State Handler
// ============================================

fn handle_agent_state(mut ctx: AppContext, payload: AgentStatePayload) {
    let mut status = ctx.agent_status.write();
    status.state = payload.state.clone();

    // Update tool info
    status.current_tool = payload.tool.map(|t| ToolDisplayInfo {
        name: t.name.clone(),
        icon: get_tool_icon(&t.name),
        description: t.target,
    });

    // Update step info
    status.step = payload.step.map(|s| StepDisplayInfo {
        current: s.current,
        total: s.total,
    });

    // Visibility
    status.visible = !matches!(payload.state, AgentState::Complete);
}

// ============================================
// Permission Request Handler
// ============================================

fn handle_permission_request(mut ctx: AppContext, payload: PermissionRequestPayload) {
    // Set agent to waiting state
    {
        let mut status = ctx.agent_status.write();
        status.state = AgentState::Waiting;
        status.visible = true;
    }

    // Show permission dialog
    ctx.permission_request.set(Some(PermissionRequestState {
        request_id: payload.request_id,
        resource_type: payload.resource_type,
        action: payload.action,
        resource: payload.resource,
        requester: payload.requester.name,
        reason: payload.reason,
        timeout_ms: payload.timeout_ms,
        created_at: js_sys::Date::now() as u64,
    }));
}

// ============================================
// Error Handler
// ============================================

fn handle_error(mut ctx: AppContext, payload: ErrorPayload) {
    // Update agent status
    {
        let mut status = ctx.agent_status.write();
        status.state = AgentState::Error;
        status.error_message = Some(payload.message.clone());
        status.visible = true;
    }

    // Clear any streaming
    ctx.streaming.set(None);

    // Add error message to chat
    ctx.messages.write().push(Message::error(
        payload.code,
        payload.message,
        payload.recoverable,
    ));
}

// ============================================
// Content Block Handler
// ============================================

fn handle_content_block(mut ctx: AppContext, payload: ContentBlockPayload) {
    let raw_text = payload.content.as_str().unwrap_or_default().to_string();

    match payload.content_type {
        ContentType::Text | ContentType::Markdown => {
            // Strip tool XML artifacts before display
            let clean = strip_tool_xml(&raw_text);
            if clean.trim().is_empty() {
                return; // Nothing to show after stripping
            }

            // Merge into the last assistant message if possible
            let merged = ctx.messages.with_mut(|messages| {
                if let Some(last) = messages.last_mut() {
                    if last.role == MessageRole::Assistant {
                        if let MessageContent::Markdown(ref mut md) = last.content {
                            md.push_str("\n\n");
                            md.push_str(&clean);
                            return true;
                        }
                    }
                }
                false
            });

            if !merged {
                ctx.messages.write().push(Message::assistant_markdown(clean));
            }
        }
        ContentType::Code => {
            let language = payload
                .metadata
                .as_ref()
                .and_then(|m| m.get("language"))
                .and_then(|v| v.as_str())
                .unwrap_or("text")
                .to_string();
            ctx.messages.write().push(Message::code(language, raw_text));
        }
        ContentType::A2ui | ContentType::Image => {
            // P2: placeholder for now
            ctx.messages.write().push(Message::assistant("[Content block not yet supported]"));
        }
    }
}

// ============================================
// Plan Proposal Handler
// ============================================

fn handle_plan_proposal(mut ctx: AppContext, payload: shared_protocol::PlanProposalPayload) {
    tracing::info!("Received plan proposal: {} steps", payload.steps.len());

    // Deactivate all existing plan messages
    ctx.messages.with_mut(|messages| {
        for msg in messages.iter_mut() {
            if let MessageContent::Plan(ref mut plan) = msg.content {
                plan.is_active = false;
            }
        }
    });

    // Convert protocol steps to state steps
    let steps: Vec<PlanStepData> = payload.steps.into_iter().map(|s| PlanStepData {
        description: s.description,
        model: s.model,
    }).collect();

    // Push new plan message as active
    ctx.messages.write().push(Message::plan(payload.summary, steps, true));

    // Set pending plan flag
    ctx.pending_plan.set(true);

    // Set agent to waiting state
    ctx.agent_status.write().set_waiting();
}

// ============================================
// System Response Handler
// ============================================

fn handle_system_response(mut ctx: AppContext, payload: SystemResponsePayload) {
    tracing::debug!("Received system response for command: {}", payload.command);

    match payload.command.as_str() {
        "session.resolve" => {
            if payload.success {
                if let Some(data) = payload.data {
                    handle_session_resolve_response(ctx, data);
                }
            } else {
                tracing::error!("session.resolve failed: {:?}", payload.error);
            }
        }
        "session.list" => {
            if payload.success {
                if let Some(data) = payload.data {
                    handle_session_list_response(ctx, data);
                }
            } else {
                let error_msg = payload.error
                    .map(|e| e.message)
                    .unwrap_or_else(|| "Unknown error".to_string());
                tracing::error!("session.list failed: {}", error_msg);
                ctx.history.write().set_error(error_msg);
            }
        }
        "session.clone" => {
            if payload.success {
                if let Some(data) = payload.data {
                    handle_session_clone_response(ctx, data);
                }
            } else {
                tracing::error!("session.clone failed: {:?}", payload.error);
            }
        }
        "session.delete" => {
            if payload.success {
                if let Some(data) = payload.data {
                    handle_session_delete_response(ctx, data);
                }
            } else {
                tracing::error!("session.delete failed: {:?}", payload.error);
            }
        }
        "session.rename" => {
            if payload.success {
                if let Some(data) = payload.data {
                    handle_session_rename_response(ctx, data);
                }
            } else {
                tracing::error!("session.rename failed: {:?}", payload.error);
            }
        }
        "session.pin" | "session.unpin" => {
            if payload.success {
                if let Some(data) = payload.data {
                    handle_session_pin_response(ctx, data);
                }
            } else {
                tracing::error!("{} failed: {:?}", payload.command, payload.error);
            }
        }
        // MCP configuration commands
        "mcp.list" => {
            if payload.success {
                if let Some(data) = payload.data {
                    handle_mcp_list_response(ctx, data);
                }
            } else {
                let error_msg = payload.error
                    .map(|e| e.message)
                    .unwrap_or_else(|| "Unknown error".to_string());
                tracing::error!("mcp.list failed: {}", error_msg);
                ctx.mcp_config.write().set_error(error_msg);
            }
        }
        "mcp.add" | "mcp.update" | "mcp.delete" => {
            handle_mcp_mutation_response(ctx, payload);
        }
        "mcp.test" => {
            if let Some(data) = payload.data {
                handle_mcp_test_response(ctx, data);
            } else if !payload.success {
                let error_msg = payload.error
                    .map(|e| e.message)
                    .unwrap_or_else(|| "Test failed".to_string());
                tracing::error!("mcp.test failed: {}", error_msg);
            }
        }
        "mcp.connect" | "mcp.disconnect" => {
            if let Some(data) = payload.data {
                handle_mcp_connection_response(ctx, data);
            } else if !payload.success {
                let error_msg = payload.error
                    .map(|e| e.message)
                    .unwrap_or_else(|| "Connection operation failed".to_string());
                tracing::error!("{} failed: {}", payload.command, error_msg);
            }
        }
        // File picker response
        "file.pick" => {
            if let Some(data) = payload.data {
                handle_file_pick_response(ctx, data);
            } else {
                // Cancelled or error
                ctx.pending_file_pick.write().take();
                if !payload.success {
                    let error_msg = payload.error
                        .map(|e| e.message)
                        .unwrap_or_else(|| "File picker failed".to_string());
                    tracing::error!("file.pick failed: {}", error_msg);
                }
            }
        }
        // Skill list response
        "skill.list" => {
            if payload.success {
                if let Some(data) = payload.data {
                    handle_skill_list_response(ctx, data);
                }
            } else {
                let error_msg = payload.error
                    .map(|e| e.message)
                    .unwrap_or_else(|| "Unknown error".to_string());
                tracing::error!("skill.list failed: {}", error_msg);
            }
        }
        "content_store.set" => {
            if payload.success {
                // Check if settings changed — re-fetch avatar
                if let Some(ref data) = payload.data {
                    if data.get("key").and_then(|k| k.as_str()) == Some("config:settings") {
                        tracing::info!("Settings updated via content_store.set, refreshing avatar");
                        refresh_avatar(ctx);
                    }
                }
            }
        }
        // ContentStore hydrated from persistence — re-fetch avatar
        "content_store.loaded" => {
            if payload.success {
                tracing::info!("ContentStore hydrated, refreshing avatar");
                refresh_avatar(ctx);
            }
        }
        _ => {
            tracing::debug!("Unhandled system response: {}", payload.command);
        }
    }
}

/// Re-fetch avatar from settings and update context
fn refresh_avatar(mut ctx: AppContext) {
    spawn_local(async move {
        match crate::messaging::fetch_avatar().await {
            Ok(Some(url)) => {
                tracing::info!("Avatar refreshed (len={})", url.len());
                ctx.avatar_url.set(Some(url));
            }
            Ok(None) => {
                ctx.avatar_url.set(None);
            }
            Err(e) => {
                tracing::warn!("Failed to refresh avatar: {}", e);
            }
        }
    });
}

/// Handle file.pick response - add picked files to context or show mode choice
fn handle_file_pick_response(mut ctx: AppContext, data: serde_json::Value) {
    tracing::info!("Received file.pick response: {:?}", data);

    // Check for choose_mode (Linux "both" mode workaround)
    if data.get("choose_mode").and_then(|v| v.as_bool()).unwrap_or(false) {
        let options: Vec<String> = data.get("options")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();

        if options.is_empty() {
            tracing::error!("choose_mode response with empty options");
            ctx.pending_file_pick.write().take();
            return;
        }

        // Recover original request params from pending state
        let (multiple, title) = ctx.pending_file_pick.read()
            .as_ref()
            .map(|p| (p.multiple, p.title.clone()))
            .unwrap_or((false, None));

        ctx.pending_file_pick.write().take();
        ctx.pending_mode_choice.set(Some(crate::state::PendingModeChoice {
            options,
            multiple,
            title,
        }));
        tracing::info!("Mode choice required — showing inline picker");
        return;
    }

    // Check if cancelled
    let cancelled = data.get("cancelled").and_then(|v| v.as_bool()).unwrap_or(false);
    if cancelled {
        tracing::debug!("File picker was cancelled by user");
        ctx.pending_file_pick.write().take();
        return;
    }

    // Parse files array
    let files = data.get("files").and_then(|v| v.as_array());
    if let Some(files_arr) = files {
        let picked_files: Vec<crate::state::PickedFile> = files_arr.iter().filter_map(|f| {
            let path = f.get("path").and_then(|v| v.as_str())?;
            Some(crate::state::PickedFile {
                path: path.to_string(),
                is_directory: f.get("is_directory").and_then(|v| v.as_bool()).unwrap_or(false),
                size: f.get("size").and_then(|v| v.as_u64()),
                modified: f.get("modified").and_then(|v| v.as_u64()),
                mime_type: f.get("mime_type").and_then(|v| v.as_str()).map(|s| s.to_string()),
            })
        }).collect();

        tracing::info!("Picked {} files", picked_files.len());
        ctx.picked_files.write().extend(picked_files);
    }

    ctx.pending_file_pick.write().take();
}

/// Handle session.resolve response - load messages into chat
fn handle_session_resolve_response(mut ctx: AppContext, data: serde_json::Value) {
    // Parse messages from response
    let messages_json = data.get("messages").and_then(|m| m.as_array());

    if let Some(messages_arr) = messages_json {
        // Clear current messages and load new ones
        let mut new_messages: Vec<Message> = Vec::new();

        for msg_json in messages_arr {
            let id = msg_json.get("id").and_then(|v| v.as_str()).unwrap_or_default();
            let role = msg_json.get("role").and_then(|v| v.as_str()).unwrap_or_default();
            let content = msg_json.get("content").and_then(|v| v.as_str()).unwrap_or_default();
            let content_type = msg_json.get("content_type").and_then(|v| v.as_str()).unwrap_or("text");
            let metadata = msg_json.get("metadata");

            if id.is_empty() || role.is_empty() {
                continue;
            }

            match content_type {
                "tool_use" => {
                    // Fold tool_use messages into the preceding assistant message
                    // as ActivityFeed entries instead of showing them as text.
                    let tool_name = metadata
                        .and_then(|m| m.get("tool_name"))
                        .and_then(|v| v.as_str())
                        .unwrap_or("unknown");
                    let arguments = metadata
                        .and_then(|m| m.get("arguments"))
                        .map(|v| {
                            if let Some(s) = v.as_str() { s.to_string() }
                            else { v.to_string() }
                        })
                        .unwrap_or_default();
                    let result_str = metadata
                        .and_then(|m| m.get("result"))
                        .map(|v| {
                            if let Some(s) = v.as_str() { s.to_string() }
                            else { v.to_string() }
                        });

                    let tool_data = ToolCallData {
                        id: id.to_string(),
                        name: tool_name.to_string(),
                        icon: get_tool_icon(tool_name).to_string(),
                        display_target: extract_tool_target(tool_name, &arguments)
                            .or(result_str.as_ref().map(|r| {
                                if r.len() > 60 { format!("{}...", &r[..60]) } else { r.clone() }
                            })),
                        arguments,
                        duration_ms: metadata
                            .and_then(|m| m.get("duration_ms"))
                            .and_then(|v| v.as_u64()),
                        status: Some(ToolCallStatus::Success),
                        kind: ActivityKind::Tool,
                    };

                    // Attach to preceding assistant Markdown message
                    if let Some(last_assistant) = new_messages.iter_mut().rev()
                        .find(|m| m.role == MessageRole::Assistant
                            && matches!(m.content, MessageContent::Markdown(_)))
                    {
                        last_assistant.tool_calls.push(tool_data);
                    }
                    // tool_use messages are folded — don't push as standalone
                }
                "tool_result" => {
                    // Tool results: try to update the matching tool_call status
                    // in the preceding assistant message. Don't show as standalone.
                    let tool_call_id = metadata
                        .and_then(|m| m.get("tool_call_id"))
                        .and_then(|v| v.as_str());

                    if let Some(tc_id) = tool_call_id {
                        // Find the tool_call and update its display_target with the result
                        for msg in new_messages.iter_mut().rev() {
                            if let Some(tc) = msg.tool_calls.iter_mut().find(|t| t.id == tc_id) {
                                if tc.display_target.is_none() && !content.is_empty() {
                                    let truncated = if content.len() > 80 {
                                        format!("{}...", &content[..80])
                                    } else {
                                        content.to_string()
                                    };
                                    tc.display_target = Some(truncated);
                                }
                                break;
                            }
                        }
                    }
                    // tool_result messages are folded — don't push as standalone
                }
                _ => {
                    // Regular text/thinking messages
                    let message = match role {
                        "user" => {
                            // Check for attachment metadata saved in history
                            let attachments: Vec<ImageAttachment> = metadata
                                .and_then(|m| m.get("attachments"))
                                .and_then(|a| a.as_array())
                                .map(|arr| {
                                    arr.iter()
                                        .filter_map(|v| {
                                            let name = v.get("name")?.as_str()?.to_string();
                                            let mime_type = v.get("mime_type")?.as_str()?.to_string();
                                            Some(ImageAttachment {
                                                id: format!("hist-{}", name),
                                                name,
                                                mime_type,
                                                data: String::new(),
                                            })
                                        })
                                        .collect()
                                })
                                .unwrap_or_default();

                            if attachments.is_empty() {
                                Message::user(content.to_string())
                            } else {
                                Message::user_with_images(content.to_string(), attachments)
                            }
                        }
                        "assistant" => Message::assistant_markdown(content.to_string()),
                        _ => continue,
                    };
                    let mut msg = message;
                    msg.id = id.to_string();
                    if let Some(ts) = msg_json.get("created_at").and_then(|v| v.as_u64()) {
                        msg.timestamp = ts * 1000; // Backend stores seconds, UI uses milliseconds
                    }
                    new_messages.push(msg);
                }
            }
        }

        // Process artifacts from session — interleave at correct chronological positions
        if let Some(artifacts_arr) = data.get("artifacts").and_then(|a| a.as_array()) {
            // Collect artifacts with their timestamps
            let mut artifacts_with_ts: Vec<(u64, Message)> = Vec::new();
            for art_json in artifacts_arr {
                if let (Some(id), Some(title)) = (
                    art_json.get("id").and_then(|v| v.as_str()),
                    art_json.get("title").and_then(|v| v.as_str()),
                ) {
                    let content_type = art_json.get("content_type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("text/html")
                        .to_string();
                    let ts = art_json.get("created_at")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) * 1000;

                    // Read is_persistent from the session response if provided.
                    let is_persistent = art_json.get("is_persistent")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false);

                    let mut art_msg = Message::artifact(
                        id.to_string(),
                        title.to_string(),
                        content_type,
                        ArtifactState::Complete,
                        is_persistent,
                    );
                    art_msg.timestamp = ts;
                    artifacts_with_ts.push((ts, art_msg));
                }
            }

            // Sort by timestamp (earliest first) so insertions proceed in order
            artifacts_with_ts.sort_by_key(|(ts, _)| *ts);

            // Insert each artifact after the last message with timestamp <= artifact's timestamp
            for (ts, art_msg) in artifacts_with_ts {
                let insert_pos = new_messages.iter()
                    .rposition(|m| m.timestamp <= ts)
                    .map(|pos| pos + 1)
                    .unwrap_or(0);
                new_messages.insert(insert_pos, art_msg);
            }

            tracing::info!("Interleaved {} artifacts into message history", artifacts_arr.len());
        }

        let tool_count: usize = new_messages.iter().map(|m| m.tool_calls.len()).sum();
        tracing::info!("Loaded {} messages ({} tool calls folded) from session",
            new_messages.len(), tool_count);
        ctx.messages.set(new_messages);
    } else {
        // No messages - clear and show welcome screen
        ctx.messages.set(Vec::new());
        tracing::info!("Session has no messages, showing welcome screen");
    }
}

/// Handle session.clone response - load cloned messages into chat
fn handle_session_clone_response(ctx: AppContext, data: serde_json::Value) {
    // Same logic as session.resolve
    handle_session_resolve_response(ctx, data);
}

/// Handle session.list response - populate history state
fn handle_session_list_response(mut ctx: AppContext, data: serde_json::Value) {
    use crate::state::SessionSummary;

    let sessions_json = data.get("sessions").and_then(|s| s.as_array());
    let total = data.get("total").and_then(|t| t.as_u64()).unwrap_or(0) as u32;
    let is_loading_more = ctx.history.read().loading_more;

    if let Some(sessions_arr) = sessions_json {
        let mut sessions = Vec::new();

        for session_json in sessions_arr {
            let id = session_json.get("id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let title = session_json.get("title").and_then(|v| v.as_str()).map(|s| s.to_string());
            let updated_at = session_json.get("updated_at").and_then(|v| v.as_u64()).unwrap_or(0);
            let message_count = session_json.get("message_count").and_then(|v| v.as_u64()).unwrap_or(0) as u32;

            // Skip sessions with no messages
            if message_count == 0 {
                continue;
            }

            sessions.push(SessionSummary {
                id,
                title,
                updated_at,
                message_count,
                pinned: session_json.get("pinned").and_then(|v| v.as_bool()).unwrap_or(false),
            });
        }

        if is_loading_more {
            ctx.history.write().append_sessions(sessions, total);
        } else {
            ctx.history.write().set_sessions(sessions, total);
        }
        tracing::info!("Loaded {} sessions into history (append={})", ctx.history.read().sessions.len(), is_loading_more);
    } else {
        if !is_loading_more {
            ctx.history.write().set_sessions(Vec::new(), 0);
        }
        tracing::info!("No sessions in history");
    }
}

/// Handle session.delete response
fn handle_session_delete_response(_ctx: AppContext, data: serde_json::Value) {
    let deleted = data.get("deleted").and_then(|v| v.as_bool()).unwrap_or(false);
    if deleted {
        tracing::info!("Session deleted successfully");
    }
}

/// Handle session.rename response - update title in history and active session
fn handle_session_rename_response(mut ctx: AppContext, data: serde_json::Value) {
    let id = data.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    let title = data.get("title").and_then(|v| v.as_str());

    if let Some(title) = title {
        ctx.history.write().update_title(id, title);

        let active_id = ctx.session.read().id.clone();
        if active_id == id {
            ctx.session.write().set_title(title.to_string());
        }

        tracing::info!("Session {} renamed to '{}'", id, title);
    }
}

/// Handle session.pin/unpin response - update pinned state in history
fn handle_session_pin_response(mut ctx: AppContext, data: serde_json::Value) {
    let id = data.get("id").and_then(|v| v.as_str()).unwrap_or_default();
    let pinned = data.get("pinned").and_then(|v| v.as_bool()).unwrap_or(false);

    ctx.history.write().update_pinned(id, pinned);
    tracing::info!("Session {} pinned={}", id, pinned);
}

// ============================================
// Internal Message Handler
// ============================================

fn handle_internal_message(mut ctx: AppContext, message: InternalMessage) {
    match message {
        InternalMessage::Pong { .. } => {
            ctx.connection.set(ConnectionState::Connected);
        }
        InternalMessage::ConnectionStatus { connected } => {
            if connected {
                ctx.connection.set(ConnectionState::Connected);
            } else {
                ctx.connection.set(ConnectionState::Disconnected);
            }
        }
        InternalMessage::TabContextUpdate(payload) => {
            // Session is window-scoped, not tab-scoped.
            // Tab switching only updates the current tab context (URL, title, etc.)
            // without changing the active session or conversation.
            tracing::info!("Tab context updated: tab_id={}, url={}", payload.tab_id, payload.url);

            // Defer signal write to next microtask to avoid AlreadyBorrowed panic.
            // The JS onMessage callback runs outside the Dioxus runtime context,
            // so we use spawn_local (not Dioxus's spawn which requires runtime).
            // This also avoids borrow conflicts when Dioxus effects/components
            // are holding read guards on tab_context during the notification phase.
            spawn_local(async move {
                ctx.tab_context.set(TabContext {
                    tab_id: payload.tab_id,
                    zen_sync_id: payload.zen_sync_id,
                    url: payload.url,
                    title: payload.title,
                    favicon_url: payload.favicon_url,
                });
                // Clear pending mode choice on tab switch
                ctx.pending_mode_choice.set(None);
            });
        }
        InternalMessage::AskUserRequest(payload) => {
            tracing::info!("Received AskUser request: {}", payload.request_id);
            handle_ask_user_request(ctx, payload);
        }
        InternalMessage::ArtifactStart(payload) => {
            handle_artifact_start(ctx, payload);
        }
        InternalMessage::ArtifactDelta(payload) => {
            handle_artifact_delta(ctx, payload);
        }
        InternalMessage::ArtifactComplete(payload) => {
            handle_artifact_complete(ctx, payload);
        }
        InternalMessage::CanvasChatInject(payload) => {
            handle_canvas_chat_inject(ctx, payload);
        }
        // Outgoing-only messages (shouldn't receive)
        InternalMessage::Ping { .. } => {
            tracing::warn!("Received unexpected outgoing Ping message");
        }
    }
}

// ============================================
// AskUser Request Handler
// ============================================

fn handle_ask_user_request(mut ctx: AppContext, payload: crate::messaging::sender::AskUserRequestPayload) {
    use crate::state::{AskUserState, Message};

    // Add assistant message showing the question in chat
    let mut question_md = format!("**Q: {}**", payload.question);
    if !payload.options.is_empty() {
        question_md.push('\n');
        for (i, opt) in payload.options.iter().enumerate() {
            question_md.push_str(&format!("\n{}. {}", i + 1, opt));
        }
    }
    ctx.messages.write().push(Message::assistant_markdown(&question_md));

    // Set the ask_user state to trigger the dialog
    ctx.ask_user.set(Some(AskUserState::new(
        payload.request_id,
        payload.question,
        payload.options,
        payload.allow_custom,
        payload.timeout_ms,
    )));

    tracing::debug!("AskUser dialog state set, waiting for user response");
}

// ============================================
// Artifact Message Handlers
// ============================================

fn handle_artifact_start(mut ctx: AppContext, payload: crate::messaging::sender::ArtifactStartPayload) {
    use crate::state::{Message, ArtifactState};

    tracing::info!("Artifact started: {}", payload.id);

    let title = payload.title.unwrap_or_else(|| "Untitled".to_string());
    let content_type = payload.content_type.unwrap_or_else(|| "text/html".to_string());

    ctx.messages.write().push(Message::artifact(
        payload.id,
        title,
        content_type,
        ArtifactState::Streaming,
        payload.is_persistent,
    ));
}

fn handle_artifact_delta(_ctx: AppContext, _payload: crate::messaging::sender::ArtifactDeltaPayload) {
    // Canvas page renders live via ContentStore. No sidebar update needed.
}

fn handle_artifact_complete(mut ctx: AppContext, payload: crate::messaging::sender::ArtifactCompletePayload) {
    use crate::state::{MessageContent, ArtifactState};

    tracing::info!("Artifact complete: {}", payload.id);

    ctx.messages.with_mut(|messages| {
        for msg in messages.iter_mut() {
            if let MessageContent::Artifact(ref mut data) = msg.content {
                if data.id == payload.id {
                    data.state = ArtifactState::Complete;
                    if let Some(ref title) = payload.title {
                        data.title = title.clone();
                    }
                    // ArtifactComplete may carry the authoritative is_persistent
                    // flag (e.g. for artifacts loaded from a resumed session that
                    // were already saved to My Canvas).
                    if payload.is_persistent {
                        data.is_persistent = true;
                    }
                    break;
                }
            }
        }
    });
}

/// Handle canvas chat inject — UI display only.
///
/// background.js already sent the chat_message to the native agent with the
/// canvas sessionId. We only need to show the user message in the sidebar UI.
/// Sending again here would use the sidebar's own session_id, causing a
/// session_id mismatch that prevents streaming responses from being routed
/// back to the canvas iframe.
fn handle_canvas_chat_inject(mut ctx: AppContext, payload: crate::messaging::sender::CanvasChatInjectPayload) {
    use crate::state::{Message, ImageAttachment};

    tracing::info!("Canvas chat inject (UI only): content='{}', attachments={}, local_files={}",
        payload.content, payload.attachments.len(), payload.local_files.len());

    // Build attachment list for display
    let mut display_attachments = Vec::new();
    for att in &payload.attachments {
        display_attachments.push(ImageAttachment {
            id: uuid::Uuid::new_v4().to_string(),
            name: att.name.clone(),
            mime_type: att.mime_type.clone(),
            data: att.data.clone().unwrap_or_default(),
        });
    }
    for f in &payload.local_files {
        let name = f.path.rsplit('/').next().unwrap_or(&f.path).to_string();
        let icon = if f.is_directory { "folder" } else { "file" };
        display_attachments.push(ImageAttachment {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            mime_type: f.mime_type.clone().unwrap_or_else(|| icon.to_string()),
            data: String::new(),
        });
    }

    // Add user message to sidebar message list for display
    if display_attachments.is_empty() {
        ctx.messages.write().push(Message::user(&payload.content));
    } else {
        ctx.messages.write().push(Message::user_with_images(&payload.content, display_attachments));
    }
    ctx.agent_status.write().set_thinking();
}

// ============================================
// MCP Response Handlers
// ============================================

/// Handle mcp.list response - populate MCP config state with servers
fn handle_mcp_list_response(mut ctx: AppContext, data: serde_json::Value) {
    use crate::state::{McpConnectionStatus, McpServer, McpServerConfig};

    let servers_json = data.get("servers").and_then(|s| s.as_array());
    let connected_json = data.get("connected").and_then(|c| c.as_array());

    // Build set of connected server names
    let connected_names: std::collections::HashSet<String> = connected_json
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    if let Some(servers_arr) = servers_json {
        let mut servers = Vec::new();

        for server_json in servers_arr {
            let name = server_json.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let command = server_json.get("command").and_then(|v| v.as_str()).unwrap_or_default().to_string();
            let args: Vec<String> = server_json.get("args")
                .and_then(|a| a.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            let enabled = server_json.get("enabled").and_then(|v| v.as_bool()).unwrap_or(true);
            let env: Vec<(String, String)> = server_json.get("env")
                .and_then(|e| e.as_object())
                .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or_default().to_string())).collect())
                .unwrap_or_default();

            let config = McpServerConfig {
                name: name.clone(),
                command,
                args,
                enabled,
                env,
            };

            let status = if connected_names.contains(&name) {
                McpConnectionStatus::Connected
            } else {
                McpConnectionStatus::Disconnected
            };

            servers.push(McpServer::with_status(config, status));
        }

        ctx.mcp_config.write().set_servers(servers);
        tracing::info!("Loaded {} MCP servers", ctx.mcp_config.read().servers.len());
    } else {
        ctx.mcp_config.write().set_servers(Vec::new());
        tracing::info!("No MCP servers configured");
    }
}

/// Handle mcp.add, mcp.update, mcp.delete responses
fn handle_mcp_mutation_response(mut ctx: AppContext, payload: SystemResponsePayload) {
    if payload.success {
        tracing::info!("{} succeeded", payload.command);
        // Refresh the server list
        ctx.mcp_config.write().set_loading();
        ctx.mcp_config.write().cancel_edit();
        spawn_local(async move {
            let _ = crate::messaging::send_mcp_list().await;
        });
    } else {
        let error_msg = payload.error
            .map(|e| e.message)
            .unwrap_or_else(|| "Operation failed".to_string());
        tracing::error!("{} failed: {}", payload.command, error_msg);
        ctx.mcp_config.write().set_error(error_msg);
    }
}

/// Handle mcp.test response
fn handle_mcp_test_response(mut ctx: AppContext, data: serde_json::Value) {
    let name = data.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let success = data.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
    let message = data.get("message").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let tools_count = data.get("tools_count").and_then(|v| v.as_u64()).unwrap_or(0);

    let result_message = if success {
        format!("Connection successful! Found {} tools.", tools_count)
    } else {
        message
    };

    ctx.mcp_config.write().set_test_result(name, success, result_message);
    tracing::info!("MCP test result: success={}, tools_count={}", success, tools_count);
}

/// Handle mcp.connect/mcp.disconnect response
fn handle_mcp_connection_response(mut ctx: AppContext, data: serde_json::Value) {
    use crate::state::McpConnectionStatus;

    let name = data.get("name").and_then(|v| v.as_str()).unwrap_or_default().to_string();
    let connected = data.get("connected").and_then(|v| v.as_bool()).unwrap_or(false);
    let error = data.get("error").and_then(|v| v.as_str()).map(|s| s.to_string());

    let status = if let Some(err) = error {
        McpConnectionStatus::Error(err)
    } else if connected {
        McpConnectionStatus::Connected
    } else {
        McpConnectionStatus::Disconnected
    };

    ctx.mcp_config.write().update_status(&name, status);
    tracing::info!("MCP server '{}' connection status updated: connected={}", name, connected);
}

// ============================================
// Skill List Response Handler
// ============================================

/// Handle skill.list response - populate available skills
fn handle_skill_list_response(mut ctx: AppContext, data: serde_json::Value) {
    use crate::state::SkillItem;

    let skills_json = data.get("skills").and_then(|s| s.as_array());

    if let Some(skills_arr) = skills_json {
        let skills: Vec<SkillItem> = skills_arr
            .iter()
            .filter_map(|s| {
                let name = s.get("name")?.as_str()?.to_string();
                let description = s.get("description")?.as_str().unwrap_or_default().to_string();
                let tags = s.get("tags")
                    .and_then(|t| t.as_array())
                    .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                    .unwrap_or_default();

                Some(SkillItem {
                    name,
                    description,
                    tags,
                })
            })
            .collect();

        tracing::info!("Loaded {} skills", skills.len());
        ctx.available_skills.set(skills);
    } else {
        tracing::info!("No skills available");
        ctx.available_skills.set(Vec::new());
    }
}
