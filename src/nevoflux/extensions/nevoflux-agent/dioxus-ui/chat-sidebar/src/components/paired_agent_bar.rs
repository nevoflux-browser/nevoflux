/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Paired AI agents at the top of the sidebar (M2b §3).
//!
//! One row per agent that is connected or paused: what it is doing, an
//! approval it waits on with the time left, and Pause / Resume / Unpair.
//! Under it: the "Let Muse approve sensitive actions" switch (M3 R39) and a
//! delegated task with Stop task, its approval and View session (M3 §6.3,
//! §6.4). An offline agent gets a row only for its running task, without
//! Pause (B-F1).
//! What a row says comes from `crate::state::paired_agents`; this file only
//! renders it and sends the commands. The buttons never change a row
//! themselves: the daemon's next status snapshot does (§3.3).

use dioxus::prelude::*;

use crate::context::{use_app_context, AppContext};
use crate::state::{
    approval_group_name, bar_rows, command_error, for_agent, task_banner, task_for_session, unpair_click, unpair_label, AgentRow,
    ConnectionState, TaskLine, ERROR_LINE_MS, GUARDED_TOGGLE_HELP, GUARDED_TOGGLE_LABEL,
    UNPAIR_CONFIRM_MS,
};

fn now_ms() -> i64 {
    js_sys::Date::now() as i64
}

/// Show `text` under a row for a few seconds. A newer error replaces it and is
/// not cleared early by the older one's timer.
fn flash_error(mut error: Signal<Option<String>>, text: String) {
    error.set(Some(text.clone()));
    spawn(async move {
        crate::messaging::sleep_ms(ERROR_LINE_MS).await;
        if error.peek().as_deref() == Some(text.as_str()) {
            error.set(None);
        }
    });
}

#[component]
pub fn PairedAgentBar() -> Element {
    let ctx = use_app_context();
    // One clock for the bar: minutes connected and the approval countdown.
    let now = use_signal(now_ms);
    use_future(move || async move {
        let mut clock = now;
        loop {
            crate::messaging::sleep_ms(1000).await;
            clock.set(now_ms());
        }
    });

    let daemon_connected = matches!(*ctx.connection.read(), ConnectionState::Connected);
    let snapshot = ctx.paired_agents.read().clone();
    let rows = bar_rows(snapshot.as_ref(), daemon_connected, *now.read() / 1000);
    if rows.is_empty() {
        return rsx! {};
    }

    rsx! {
        div {
            class: "paired-agent-bar",
            role: "region",
            aria_label: "Paired AI agents",
            for row in rows {
                PairedAgentRow { key: "{row.channel_id}", row: row.clone() }
            }
        }
    }
}

#[component]
fn PairedAgentRow(row: AgentRow) -> Element {
    let mut armed_at = use_signal(|| None::<i64>);
    let mut busy = use_signal(|| false);
    let error = use_signal(|| None::<String>);

    let toggle = {
        let channel = row.channel_id.clone();
        let paused = row.paused;
        move |_| {
            if *busy.peek() {
                return;
            }
            busy.set(true);
            let channel = channel.clone();
            spawn(async move {
                let result = if paused {
                    crate::messaging::remote_agent_resume(&channel)
                        .await
                        .map(|_opened| ())
                } else {
                    crate::messaging::remote_agent_pause(&channel).await
                };
                let mut busy = busy;
                busy.set(false);
                if let Err(e) = result {
                    let action = if paused { "resume" } else { "pause" };
                    flash_error(error, command_error(action, &e));
                }
            });
        }
    };

    let unpair = {
        let channel = row.channel_id.clone();
        move |_| {
            if *busy.peek() {
                return;
            }
            let (next, send) = unpair_click(*armed_at.peek(), now_ms());
            armed_at.set(next);
            if send {
                busy.set(true);
                let channel = channel.clone();
                spawn(async move {
                    let result = crate::messaging::remote_unpair(&channel).await;
                    let mut busy = busy;
                    busy.set(false);
                    // Ok(true) and Ok(false) both end with the pairing gone;
                    // the daemon's next snapshot drops the row.
                    if let Err(e) = result {
                        flash_error(error, command_error("unpair", &e));
                    }
                });
            } else if let Some(at) = next {
                // Put the label back when the confirm window closes.
                spawn(async move {
                    crate::messaging::sleep_ms(UNPAIR_CONFIRM_MS as u32).await;
                    let mut armed_at = armed_at;
                    if *armed_at.peek() == Some(at) {
                        armed_at.set(None);
                    }
                });
            }
        }
    };

    let row_class = if row.paused {
        "paired-agent-row paused"
    } else {
        "paired-agent-row"
    };
    let icon = row.icon;
    let label = row.label.clone();
    let detail = row.detail.clone();
    let toggle_text = if row.paused { "Resume" } else { "Pause" };
    let toggle_aria = format!("{toggle_text} {label}");
    let unpair_text = unpair_label(*armed_at.read(), now_ms());
    let unpair_class = if armed_at.read().is_some() {
        "paired-agent-btn armed"
    } else {
        "paired-agent-btn"
    };
    let unpair_aria = format!("Unpair {label}");
    let is_busy = *busy.read();
    let error_line = error.read().clone();
    // An offline row is there only for its task (B-F1): no Pause / Resume.
    let offers_pause = row.offers_pause();

    // Contract v1.2 R39: shows the snapshot only; a click sends the command
    // and the next snapshot moves the switch (no optimistic update).
    let guarded = row.muse_may_approve_guarded;
    let mut set_guarded = {
        let channel = row.channel_id.clone();
        move || {
            if *busy.peek() {
                return;
            }
            busy.set(true);
            let channel = channel.clone();
            spawn(async move {
                let result =
                    crate::messaging::remote_agent_set_guarded_approval(&channel, !guarded).await;
                let mut busy = busy;
                busy.set(false);
                if let Err(e) = result {
                    flash_error(error, command_error("change the setting", &e));
                }
            });
        }
    };
    let guarded_id = format!("guarded-help-{}", row.channel_id);
    let guarded_aria = for_agent(GUARDED_TOGGLE_LABEL, &label);

    rsx! {
        div { class: "{row_class}",
            span { class: "paired-agent-icon", aria_hidden: "true", "{icon}" }
            span { class: "paired-agent-text", title: "{detail}",
                strong { "{label}" }
                " {detail}"
            }
            if offers_pause {
                button {
                    class: "paired-agent-btn",
                    disabled: is_busy,
                    aria_label: "{toggle_aria}",
                    onclick: toggle,
                    "{toggle_text}"
                }
            }
            button {
                class: "{unpair_class}",
                disabled: is_busy,
                aria_label: "{unpair_aria}",
                onclick: unpair,
                "{unpair_text}"
            }
        }
        div { class: "paired-agent-setting",
            label { class: "paired-agent-toggle",
                input {
                    r#type: "checkbox",
                    role: "switch",
                    checked: guarded,
                    disabled: is_busy,
                    aria_label: "{guarded_aria}",
                    aria_describedby: "{guarded_id}",
                    // The snapshot decides; undo the browser's own flip.
                    onclick: move |evt: MouseEvent| {
                        evt.prevent_default();
                        set_guarded();
                    },
                }
                "{GUARDED_TOGGLE_LABEL}"
            }
            div { id: "{guarded_id}", class: "paired-agent-help", "{GUARDED_TOGGLE_HELP}" }
        }
        {error_line.map(|msg| rsx! {
            div { class: "paired-agent-error", role: "alert", "{msg}" }
        })}
        {row.task.clone().map(|task| rsx! {
            PairedAgentTask { task: task, label: label.clone() }
        })}
    }
}

/// Answer a delegated task's approval (contract R23). The line does not
/// change on success: the daemon's next status snapshot drops it.
fn answer_approval(
    busy: Signal<bool>,
    error: Signal<Option<String>>,
    task_id: String,
    approval_id: String,
    choice: &'static str,
) {
    if *busy.peek() {
        return;
    }
    let mut busy = busy;
    busy.set(true);
    spawn(async move {
        let result = crate::messaging::remote_task_answer(&task_id, &approval_id, choice).await;
        let mut busy = busy;
        busy.set(false);
        if let Err(e) = result {
            flash_error(error, command_error("answer", &e));
        }
    });
}

/// Show `session_id` in the sidebar, as picking it in the history list does.
fn open_session(mut ctx: AppContext, session_id: String) {
    spawn(async move {
        ctx.session.write().id = session_id.clone();
        ctx.session.write().title = None;
        ctx.messages.set(Vec::new());
        ctx.streaming.set(None);
        ctx.show_history_panel.set(false);
        let _ = crate::messaging::send_session_resolve(&session_id).await;
    });
}

/// A delegated task under its agent's row (M3 §6.3, §6.4).
#[component]
fn PairedAgentTask(task: TaskLine, label: String) -> Element {
    let ctx = use_app_context();
    let mut busy = use_signal(|| false);
    let error = use_signal(|| None::<String>);

    let stop = {
        let task_id = task.task_id.clone();
        move |_| {
            if *busy.peek() {
                return;
            }
            busy.set(true);
            let task_id = task_id.clone();
            spawn(async move {
                let result = crate::messaging::remote_task_cancel(&task_id).await;
                let mut busy = busy;
                busy.set(false);
                if let Err(e) = result {
                    flash_error(error, command_error("stop the task", &e));
                }
            });
        }
    };

    let is_busy = *busy.read();
    let error_line = error.read().clone();
    let text = task.text.clone();
    let task_id = task.task_id.clone();
    let session_id = task.session_id.clone();
    let approval = task.approval.clone();
    let approval_text = approval.as_ref().map(|a| a.text.clone()).unwrap_or_default();
    // Several agents can each show these lines: every accessible name says
    // whose task it is, while the visible copy stays exact.
    let stop_aria = for_agent("Stop task", &label);
    let view_aria = for_agent("View session", &label);
    let group_aria = approval_group_name(&label);

    rsx! {
        div { class: "paired-agent-task",
            span { class: "paired-agent-text", title: "{text}", "{text}" }
            button {
                class: "paired-agent-btn",
                disabled: is_busy,
                aria_label: "{stop_aria}",
                onclick: stop,
                "Stop task"
            }
            // With an approval pending, View session sits on its line instead.
            if approval.is_none() {
                button {
                    class: "paired-agent-btn",
                    aria_label: "{view_aria}",
                    onclick: {
                        let session_id = session_id.clone();
                        move |_| open_session(ctx, session_id.clone())
                    },
                    "View session"
                }
            }
        }
        if let Some(approval) = approval {
            div {
                class: "paired-agent-task approval",
                role: "group",
                aria_label: "{group_aria}",
                span { class: "paired-agent-text", title: "{approval_text}", "{approval_text}" }
                if let Some(hint) = approval.hint {
                    span { class: "paired-agent-hint", "{hint}" }
                }
                for (text, choice) in approval.buttons.clone() {
                    button {
                        key: "{choice}",
                        class: "paired-agent-btn",
                        disabled: is_busy,
                        aria_label: for_agent(text, &label),
                        onclick: {
                            let task_id = task_id.clone();
                            let approval_id = approval.approval_id.clone();
                            move |_| answer_approval(busy, error, task_id.clone(), approval_id.clone(), choice)
                        },
                        "{text}"
                    }
                }
                button {
                    class: "paired-agent-btn",
                    aria_label: "{view_aria}",
                    onclick: {
                        let session_id = session_id.clone();
                        move |_| open_session(ctx, session_id.clone())
                    },
                    "View session"
                }
            }
        }
        {error_line.map(|msg| rsx! {
            div { class: "paired-agent-error", role: "alert", "{msg}" }
        })}
    }
}

/// Over a conversation a paired agent is running right now (M3 §6.1).
#[component]
pub fn TaskSessionBanner() -> Element {
    let ctx = use_app_context();
    let connected = matches!(*ctx.connection.read(), ConnectionState::Connected);
    let snapshot = ctx.paired_agents.read().clone();
    let current = ctx.session.read().id.clone();
    let text = if connected {
        task_for_session(snapshot.as_ref(), &current).map(|(agent, task)| task_banner(agent, task))
    } else {
        None
    };
    match text {
        Some(text) => rsx! {
            div { class: "task-session-banner", role: "status", "{text}" }
        },
        None => rsx! {},
    }
}
