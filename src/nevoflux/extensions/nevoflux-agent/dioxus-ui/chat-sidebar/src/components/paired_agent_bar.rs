/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Paired AI agents at the top of the sidebar (M2b §3).
//!
//! One row per agent that is connected or paused: what it is doing, an
//! approval it waits on with the time left, and Pause / Resume / Unpair.
//! What a row says comes from `crate::state::paired_agents`; this file only
//! renders it and sends the commands. The buttons never change a row
//! themselves: the daemon's next status snapshot does (§3.3).

use dioxus::prelude::*;

use crate::context::use_app_context;
use crate::state::{
    bar_rows, command_error, unpair_click, unpair_label, AgentRow, ConnectionState, ERROR_LINE_MS,
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

    rsx! {
        div { class: "{row_class}",
            span { class: "paired-agent-icon", aria_hidden: "true", "{icon}" }
            span { class: "paired-agent-text", title: "{detail}",
                strong { "{label}" }
                " {detail}"
            }
            button {
                class: "paired-agent-btn",
                disabled: is_busy,
                aria_label: "{toggle_aria}",
                onclick: toggle,
                "{toggle_text}"
            }
            button {
                class: "{unpair_class}",
                disabled: is_busy,
                aria_label: "{unpair_aria}",
                onclick: unpair,
                "{unpair_text}"
            }
        }
        {error_line.map(|msg| rsx! {
            div { class: "paired-agent-error", role: "alert", "{msg}" }
        })}
    }
}
