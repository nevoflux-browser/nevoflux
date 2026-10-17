/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Plan card component for displaying execution plans

use dioxus::prelude::*;
use crate::context::{use_app_context, AppContext};
use crate::state::{Message, MessageContent};

/// The session a plan answer goes to: the one that proposed it (M3 §6.1),
/// or, from an older daemon that did not say, the one on screen.
fn answer_session(ctx: &AppContext, plan_session: &str) -> String {
    if plan_session.is_empty() {
        ctx.session.read().id.clone()
    } else {
        plan_session.to_string()
    }
}

/// Plan card component displaying a proposed execution plan
#[component]
pub fn PlanCard(message: Message) -> Element {
    let mut ctx = use_app_context();

    let plan = match &message.content {
        MessageContent::Plan(plan) => plan.clone(),
        _ => return rsx! {},
    };

    let is_active = plan.is_active;
    let msg_id = message.id.clone();

    let handle_confirm = {
        let msg_id = msg_id.clone();
        let plan_session = plan.session_id.clone();
        move |_| {
            let msg_id = msg_id.clone();
            let session_id = answer_session(&ctx, &plan_session);

            // Deactivate this plan
            ctx.messages.with_mut(|messages| {
                for msg in messages.iter_mut() {
                    if msg.id == msg_id {
                        if let MessageContent::Plan(ref mut plan) = msg.content {
                            plan.is_active = false;
                        }
                    }
                }
            });

            ctx.pending_plan.set(false);
            ctx.routing.write().drop_plan(&session_id);
            ctx.agent_status.write().set_executing();

            spawn(async move {
                if let Err(e) = crate::messaging::send_plan_confirmed(&session_id).await {
                    tracing::error!("Failed to send plan confirmed: {}", e);
                }
            });
        }
    };

    let handle_cancel = {
        let msg_id = msg_id.clone();
        let plan_session = plan.session_id.clone();
        move |_| {
            let msg_id = msg_id.clone();
            let session_id = answer_session(&ctx, &plan_session);

            // Deactivate this plan
            ctx.messages.with_mut(|messages| {
                for msg in messages.iter_mut() {
                    if msg.id == msg_id {
                        if let MessageContent::Plan(ref mut plan) = msg.content {
                            plan.is_active = false;
                        }
                    }
                }
            });

            ctx.pending_plan.set(false);
            ctx.routing.write().drop_plan(&session_id);
            ctx.agent_status.write().hide();

            spawn(async move {
                if let Err(e) = crate::messaging::send_plan_cancelled(&session_id).await {
                    tracing::error!("Failed to send plan cancelled: {}", e);
                }
            });
        }
    };

    let card_class = if is_active { "plan-card" } else { "plan-card inactive" };

    rsx! {
        div { class: "{card_class}",
            // Header
            div { class: "plan-header",
                span { class: "plan-icon", "\u{1F4CB}" }
                span { class: "plan-title", "Execution Plan" }
            }

            // Summary
            p { class: "plan-summary", "{plan.summary}" }

            // Steps list
            ol { class: "plan-steps",
                for (i, step) in plan.steps.iter().enumerate() {
                    li {
                        key: "{i}",
                        class: "plan-step",
                        span { class: "plan-step-text", "{step.description}" }
                        if let Some(ref model) = step.model {
                            span { class: "step-model-badge", "{model}" }
                        }
                    }
                }
            }

            // Action buttons (only when active)
            if is_active {
                div { class: "plan-actions",
                    button {
                        class: "plan-btn plan-btn-cancel",
                        onclick: handle_cancel,
                        "Cancel"
                    }
                    button {
                        class: "plan-btn plan-btn-confirm",
                        onclick: handle_confirm,
                        "Confirm"
                    }
                }
            }
        }
    }
}
