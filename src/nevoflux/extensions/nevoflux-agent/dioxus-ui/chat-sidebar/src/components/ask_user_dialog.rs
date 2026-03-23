/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! AskUser dialog component
//!
//! Displays a dialog when the agent asks the user a question.

use dioxus::prelude::*;
use crate::context::use_app_context;
use crate::state::Message;

/// AskUser dialog component
///
/// Displays the question, options, and optional custom input field.
/// Sends response back to background.js when user answers.
#[component]
pub fn AskUserDialog() -> Element {
    let mut ctx = use_app_context();
    let ask_user = ctx.ask_user.read().clone();

    // If no pending request, don't render
    let request = match ask_user {
        Some(r) => r,
        None => return rsx! {},
    };

    let mut custom_input = use_signal(String::new);
    let mut selected_index = use_signal(|| None::<usize>);

    let request_id = request.request_id.clone();
    let options = request.options.clone();
    let allow_custom = request.allow_custom;

    // Handle option selection
    let mut handle_select_option = move |index: usize| {
        selected_index.set(Some(index));
        custom_input.set(String::new());
    };

    // Handle submit
    let handle_submit = {
        let request_id = request_id.clone();
        let options = options.clone();
        let question = request.question.clone();
        move |_| {
            let answer = if let Some(idx) = selected_index() {
                let answer = options.get(idx).cloned().unwrap_or_default();
                send_ask_user_response(&request_id, &answer, false, Some(idx as i32));
                answer
            } else if allow_custom && !custom_input.read().trim().is_empty() {
                let answer = custom_input.read().trim().to_string();
                send_ask_user_response(&request_id, &answer, true, None);
                answer
            } else {
                return;
            };

            // Add user reply to chat as a Q&A message
            ctx.messages.write().push(Message::qa(&question, &answer));

            // Clear the request
            ctx.ask_user.set(None);
        }
    };

    // Handle cancel
    let handle_cancel = {
        let request_id = request.request_id.clone();
        let question = request.question.clone();
        move |_| {
            send_ask_user_cancel(&request_id);

            // Add cancellation notice to chat
            ctx.messages.write().push(Message::qa(&question, "Cancelled"));

            ctx.ask_user.set(None);
        }
    };

    // Handle custom input change
    let handle_custom_input = move |evt: Event<FormData>| {
        custom_input.set(evt.value().clone());
        selected_index.set(None); // Deselect options when typing custom
    };

    // Handle keydown for Enter
    let handle_keydown = {
        let request_id = request.request_id.clone();
        let options = options.clone();
        let question = request.question.clone();
        move |evt: KeyboardEvent| {
            if evt.key() == Key::Enter && !evt.modifiers().shift() {
                evt.prevent_default();

                // Submit if there's a selection or custom input
                let answer = if let Some(idx) = selected_index() {
                    let answer = options.get(idx).cloned().unwrap_or_default();
                    send_ask_user_response(&request_id, &answer, false, Some(idx as i32));
                    Some(answer)
                } else if allow_custom && !custom_input.read().trim().is_empty() {
                    let answer = custom_input.read().trim().to_string();
                    send_ask_user_response(&request_id, &answer, true, None);
                    Some(answer)
                } else {
                    None
                };

                if let Some(answer) = answer {
                    ctx.messages.write().push(Message::qa(&question, &answer));
                    ctx.ask_user.set(None);
                }
            } else if evt.key() == Key::Escape {
                send_ask_user_cancel(&request_id);
                ctx.messages.write().push(Message::qa(&question, "Cancelled"));
                ctx.ask_user.set(None);
            }
        }
    };

    let can_submit = selected_index().is_some() || (allow_custom && !custom_input.read().trim().is_empty());

    rsx! {
        div { class: "ask-user-overlay",
            div { class: "ask-user-dialog",
                onkeydown: handle_keydown,

                // Question
                div { class: "ask-user-question",
                    "{request.question}"
                }

                // Options
                if !request.options.is_empty() {
                    div { class: "ask-user-options",
                        for (index , option) in request.options.iter().enumerate() {
                            {
                                let option_text = option.clone();
                                let is_selected = selected_index() == Some(index);
                                rsx! {
                                    button {
                                        class: "ask-user-option",
                                        class: if is_selected { "selected" },
                                        onclick: move |_| handle_select_option(index),
                                        "{option_text}"
                                    }
                                }
                            }
                        }
                    }
                }

                // Custom input (if allowed)
                if allow_custom {
                    div { class: "ask-user-custom",
                        input {
                            r#type: "text",
                            class: "ask-user-input",
                            placeholder: "Or type your own answer...",
                            value: "{custom_input}",
                            oninput: handle_custom_input,
                        }
                    }
                }

                // Actions
                div { class: "ask-user-actions",
                    button {
                        class: "ask-user-cancel",
                        onclick: handle_cancel,
                        "Cancel"
                    }
                    button {
                        class: "ask-user-submit",
                        disabled: !can_submit,
                        onclick: handle_submit,
                        "Submit"
                    }
                }
            }
        }
    }
}

/// Send ask user response to background.js
fn send_ask_user_response(request_id: &str, answer: &str, is_custom: bool, selected_index: Option<i32>) {
    let request_id = request_id.to_string();
    let answer = answer.to_string();

    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = crate::messaging::send_ask_user_response(&request_id, &answer, is_custom, selected_index).await {
            tracing::error!("Failed to send ask_user response: {}", e);
        }
    });
}

/// Send ask user cancel to background.js
fn send_ask_user_cancel(request_id: &str) {
    let request_id = request_id.to_string();

    wasm_bindgen_futures::spawn_local(async move {
        if let Err(e) = crate::messaging::send_ask_user_cancel(&request_id).await {
            tracing::error!("Failed to send ask_user cancel: {}", e);
        }
    });
}
