/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Render-job progress card shown inline with the `canvas_render_video`
//! tool_use that kicked off the job. Reads from
//! `ctx.render_jobs.get(job_id)` and re-renders on signal changes.

use dioxus::prelude::*;
use shared_protocol::RenderJobState;

use crate::context::use_app_context;
use crate::messaging::{send_canvas_video_cancel, send_canvas_video_reveal_path};

/// If the last delivery for a running job was more than this many
/// milliseconds ago we surface a soft "stalled" indicator in the UI.
const STALLED_THRESHOLD_MS: u64 = 180_000;

#[component]
pub fn RenderProgressCard(job_id: String) -> Element {
    let ctx = use_app_context();

    // Clone the entry out of the signal borrow immediately so we never
    // hold a read guard across rsx! or any potential .await. The signal
    // subscription is established by the `.read()` call itself.
    let entry_opt = {
        let jobs = ctx.render_jobs.read();
        jobs.get(&job_id).cloned()
    };

    let Some(entry) = entry_opt else {
        // Not yet in state — tool_use just landed but no events have
        // arrived. Show a minimal "starting" placeholder.
        let short = job_id_short(&job_id);
        return rsx! {
            div { class: "render-progress-card state-running",
                div { class: "card-title", "Render job {short} — starting…" }
            }
        };
    };

    let percent = entry.percent();
    let now_ms = js_sys::Date::now() as u64;
    let stalled = matches!(entry.state, RenderJobState::Running)
        && now_ms.saturating_sub(entry.last_update_ms) > STALLED_THRESHOLD_MS;

    let state_class = match entry.state {
        RenderJobState::Running => {
            if stalled {
                "state-running state-stalled"
            } else {
                "state-running"
            }
        }
        RenderJobState::Succeeded => "state-succeeded",
        RenderJobState::Failed => "state-failed",
        RenderJobState::Cancelled => "state-cancelled",
    };

    let title_text = match entry.state {
        RenderJobState::Running => {
            if stalled {
                format!(
                    "Rendering video {} — progress stalled…",
                    job_id_short(&entry.job_id)
                )
            } else {
                format!("Rendering video {}", job_id_short(&entry.job_id))
            }
        }
        RenderJobState::Succeeded => "\u{2705} Render complete".to_string(),
        RenderJobState::Failed => "\u{274C} Render failed".to_string(),
        RenderJobState::Cancelled => "\u{26D4} Render cancelled".to_string(),
    };

    let show_cancel = matches!(entry.state, RenderJobState::Running);
    let cancel_job_id = entry.job_id.clone();

    let body = match entry.state {
        RenderJobState::Running => {
            let current = entry.current;
            let total = entry.total;
            rsx! {
                div { class: "progress-track",
                    div { class: "progress-fill",
                        style: "width: {percent}%",
                    }
                }
                div { class: "counter",
                    "{current} / {total} frames · {percent}%"
                }
            }
        }
        RenderJobState::Succeeded => {
            let path = entry.output_path.clone().unwrap_or_default();
            let path_play = path.clone();
            let path_reveal = path.clone();
            let disabled = path.is_empty();
            rsx! {
                div { class: "terminal-line succeeded",
                    code { class: "path-display", "{path}" }
                }
                div { class: "card-actions",
                    button {
                        class: "reveal-btn play-btn",
                        disabled: disabled,
                        title: "Open in default video player",
                        onclick: move |_| {
                            let p = path_play.clone();
                            spawn(async move {
                                if let Err(e) = send_canvas_video_reveal_path(&p, "play").await {
                                    web_sys::console::warn_1(&format!("play failed: {}", e).into());
                                }
                            });
                        },
                        "\u{25B6} Play"
                    }
                    button {
                        class: "reveal-btn folder-btn",
                        disabled: disabled,
                        title: "Open containing folder",
                        onclick: move |_| {
                            let p = path_reveal.clone();
                            spawn(async move {
                                if let Err(e) = send_canvas_video_reveal_path(&p, "reveal").await {
                                    web_sys::console::warn_1(&format!("reveal failed: {}", e).into());
                                }
                            });
                        },
                        "\u{1F4C2} Open folder"
                    }
                }
            }
        }
        RenderJobState::Failed => {
            let err = entry
                .error
                .clone()
                .unwrap_or_else(|| "(no message)".to_string());
            let truncated = if err.chars().count() > 160 {
                let mut t: String = err.chars().take(157).collect();
                t.push_str("…");
                t
            } else {
                err
            };
            rsx! {
                div { class: "terminal-line failed",
                    "{truncated}"
                }
            }
        }
        RenderJobState::Cancelled => {
            let current = entry.current;
            let total = entry.total;
            rsx! {
                div { class: "terminal-line cancelled",
                    "Cancelled after {current} / {total} frames"
                }
            }
        }
    };

    rsx! {
        div { class: "render-progress-card {state_class}",
            div { class: "card-title", "{title_text}" }
            {body}
            if show_cancel {
                div { class: "card-actions",
                    button {
                        class: "cancel-btn",
                        onclick: move |_| {
                            let j = cancel_job_id.clone();
                            spawn(async move {
                                if let Err(e) = send_canvas_video_cancel(&j).await {
                                    web_sys::console::warn_1(
                                        &format!("cancel failed: {}", e).into(),
                                    );
                                }
                            });
                        },
                        "Cancel"
                    }
                }
            }
        }
    }
}

/// Compress `job-XXXXXXXXXXXX…` into a stable short form
/// `job-XXXXXXXX…` that fits in the card title without wrapping.
fn job_id_short(job_id: &str) -> String {
    let tail_start = if job_id.starts_with("job-") { 4 } else { 0 };
    let head = &job_id[..tail_start];
    let tail: String = job_id[tail_start..].chars().take(8).collect();
    format!("{}{}…", head, tail)
}
