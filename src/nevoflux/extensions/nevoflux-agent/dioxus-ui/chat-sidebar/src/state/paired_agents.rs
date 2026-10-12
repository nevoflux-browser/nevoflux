/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Paired AI agents as the desktop sees them right now (M2b §3).
//!
//! The daemon publishes `system:agent:status`: a sticky snapshot that is
//! always complete, one entry per agent pairing. Everything the sidebar shows
//! about it is derived here, with no signals and no browser, so it runs under
//! `cargo test --target x86_64-pc-windows-msvc`.

use serde::Deserialize;

/// The sticky EventBus topic the daemon publishes the snapshot on.
pub const AGENT_STATUS_TOPIC: &str = "system:agent:status";

/// Shown when a pairing has no label (the contract allows `label: null`).
pub const DEFAULT_LABEL: &str = "Muse";

/// How long the daemon waits for an approval (M2a `APPROVAL_TIMEOUT`).
pub const APPROVAL_TIMEOUT_SECS: i64 = 60;

/// How long "Unpair" waits for its confirming second click.
pub const UNPAIR_CONFIRM_MS: i64 = 3000;

/// How long a failed command's error line stays under its row.
pub const ERROR_LINE_MS: u32 = 5000;

/// One `system:agent:status` payload.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PairedAgentsSnapshot {
    pub agents: Vec<PairedAgentEntry>,
    /// When the daemon published it (the EventBus event's `timestamp_ms`).
    /// Not part of the payload: the sidebar sets it on receipt so a replayed
    /// older snapshot can never replace a newer one. 0 from the parser.
    pub published_ms: u64,
}

/// One agent pairing in the snapshot (contract §1).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PairedAgentEntry {
    /// The full control channel id: what the commands are sent with.
    pub channel_id: String,
    #[serde(default)]
    pub label: Option<String>,
    /// `connected` | `paused` | `offline`. Kept as text: a value this build
    /// does not know is treated as offline, never as connected.
    #[serde(default)]
    pub state: String,
    /// Unix seconds the pairing entered `state`.
    #[serde(default)]
    pub since: i64,
    /// The whitelisted tool running now, by its name.
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub approval: Option<PendingApproval>,
}

/// An approval the agent is waiting on.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct PendingApproval {
    /// What the call would do and where; never the values typed into a page.
    #[serde(default)]
    pub summary: String,
    /// Unix seconds the approval was asked.
    pub started_at: i64,
}

/// Read one `system:agent:status` payload. `None` when there is no `agents`
/// array at all; a single malformed entry is dropped, not the whole snapshot.
pub fn parse_paired_agents(payload: &serde_json::Value) -> Option<PairedAgentsSnapshot> {
    let agents = payload.get("agents")?.as_array()?;
    Some(PairedAgentsSnapshot {
        agents: agents
            .iter()
            .filter_map(|a| serde_json::from_value(a.clone()).ok())
            .collect(),
        published_ms: 0,
    })
}

/// The 16 tools a paired agent may call (M2a §2.5), each as the few words the
/// bar shows while it runs. The daemon sends only the tool name.
const TOOL_WORDS: [(&str, &str); 16] = [
    ("browser_snapshot", "reading the page"),
    ("browser_get_markdown", "reading the page text"),
    ("browser_screenshot", "screenshot"),
    ("browser_get_tabs", "listing tabs"),
    ("browser_navigate", "opening a page"),
    ("browser_activate_tab", "switching tabs"),
    ("browser_click_by_id", "clicking"),
    ("browser_fill_by_id", "filling a field"),
    ("browser_type_by_id", "typing"),
    ("browser_click", "clicking"),
    ("browser_fill", "filling a field"),
    ("browser_type", "typing"),
    ("browser_key_press", "pressing a key"),
    ("browser_scroll", "scrolling"),
    ("browser_wait_for", "waiting for the page"),
    ("browser_upload_file", "uploading a file"),
];

/// The snapshot to hold after one more `system:agent:status` delivery, or
/// `None` to keep the current one.
///
/// The bus keeps up to 100 sticky events per topic and replays them
/// oldest-first, so a new subscriber gets several full snapshots in order.
/// Each replaces the last entirely (never merged). One published before the
/// snapshot already held is ignored, so an out-of-order replay cannot roll
/// the bar back; a payload without `agents` is ignored too.
pub fn next_paired_agents(
    current: Option<&PairedAgentsSnapshot>,
    payload: &serde_json::Value,
    published_ms: u64,
) -> Option<PairedAgentsSnapshot> {
    if current.is_some_and(|c| published_ms < c.published_ms) {
        return None;
    }
    let mut next = parse_paired_agents(payload)?;
    next.published_ms = published_ms;
    Some(next)
}

/// The short words for a tool, if it is one of the 16.
pub fn tool_word(tool: &str) -> Option<&'static str> {
    TOOL_WORDS
        .iter()
        .find(|(name, _)| *name == tool)
        .map(|(_, word)| *word)
}

/// One line of the bar.
#[derive(Debug, Clone, PartialEq)]
pub struct AgentRow {
    /// The full control channel id: what Pause / Resume / Unpair send.
    pub channel_id: String,
    pub label: String,
    /// `●` connected, `⏳` waiting for an approval, `⏸` paused.
    pub icon: &'static str,
    /// Everything after the bold label.
    pub detail: String,
    pub paused: bool,
}

/// How long it has been connected, in the words the bar uses.
pub fn connected_for(since: i64, now: i64) -> String {
    let minutes = (now - since).max(0) / 60;
    match minutes {
        0 => "under a minute".to_string(),
        1..=59 => format!("{minutes} min"),
        _ => format!("{} h {} min", minutes / 60, minutes % 60),
    }
}

/// Seconds an approval has left, from the sidebar's own clock (§3.2).
pub fn seconds_left(started_at: i64, now: i64) -> i64 {
    (APPROVAL_TIMEOUT_SECS - (now - started_at)).clamp(0, APPROVAL_TIMEOUT_SECS)
}

/// The bar's rows: connected and paused agents, in the daemon's order.
/// Offline agents, and states this build does not know, are not shown.
pub fn rows_from_status(snapshot: &PairedAgentsSnapshot, now: i64) -> Vec<AgentRow> {
    snapshot
        .agents
        .iter()
        .filter_map(|a| {
            let (icon, detail, paused) = match a.state.as_str() {
                "paused" => ("⏸", "paused".to_string(), true),
                "connected" => match (&a.approval, &a.tool) {
                    (Some(ap), _) => (
                        "⏳",
                        format!(
                            "waiting for approval: {} · {} s left",
                            ap.summary,
                            seconds_left(ap.started_at, now)
                        ),
                        false,
                    ),
                    (None, Some(tool)) => (
                        "●",
                        format!("working: {}", tool_word(tool).unwrap_or(tool.as_str())),
                        false,
                    ),
                    (None, None) => (
                        "●",
                        format!("connected · {}", connected_for(a.since, now)),
                        false,
                    ),
                },
                _ => return None,
            };
            let label = a
                .label
                .as_deref()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .unwrap_or(DEFAULT_LABEL)
                .to_string();
            Some(AgentRow {
                channel_id: a.channel_id.clone(),
                label,
                icon,
                detail,
                paused,
            })
        })
        .collect()
}

/// What the bar shows: nothing before the first snapshot, and nothing while
/// the sidebar is not connected to the daemon, because the last snapshot
/// then describes a daemon that may be gone.
pub fn bar_rows(
    snapshot: Option<&PairedAgentsSnapshot>,
    daemon_connected: bool,
    now: i64,
) -> Vec<AgentRow> {
    match snapshot {
        Some(s) if daemon_connected => rows_from_status(s, now),
        _ => Vec::new(),
    }
}

/// An agent's live state for `/devices` (§3.4), or `None` when the snapshot
/// does not mention it.
pub fn presence_word(snapshot: &PairedAgentsSnapshot, channel_id: &str) -> Option<&'static str> {
    let entry = snapshot.agents.iter().find(|a| a.channel_id == channel_id)?;
    Some(match entry.state.as_str() {
        "connected" => "connected",
        "paused" => "paused",
        _ => "offline",
    })
}

fn unpair_armed(armed_at: Option<i64>, now_ms: i64) -> bool {
    matches!(armed_at, Some(at) if now_ms >= at && now_ms - at < UNPAIR_CONFIRM_MS)
}

/// A click on Unpair: returns the new armed time and whether to send now.
/// The first click only arms; a second within 3 s sends; a later one arms
/// again (§3.3). Unpairing cannot be undone, so nothing else sends.
pub fn unpair_click(armed_at: Option<i64>, now_ms: i64) -> (Option<i64>, bool) {
    if unpair_armed(armed_at, now_ms) {
        (None, true)
    } else {
        (Some(now_ms), false)
    }
}

/// The Unpair button's text.
pub fn unpair_label(armed_at: Option<i64>, now_ms: i64) -> &'static str {
    if unpair_armed(armed_at, now_ms) {
        "Confirm unpair?"
    } else {
        "Unpair"
    }
}

/// Whether a `remote.agent_pause` / `remote.agent_resume` answer confirms the
/// state that was asked for.
pub fn confirms_paused(data: &serde_json::Value, want: bool) -> Result<(), String> {
    match data.get("paused").and_then(|v| v.as_bool()) {
        Some(p) if p == want => Ok(()),
        _ => Err(format!(
            "the daemon did not confirm the agent is {}",
            if want { "paused" } else { "resumed" }
        )),
    }
}

/// The error line under a row when a command failed.
pub fn command_error(action: &str, reason: &str) -> String {
    format!("Could not {action}: {reason}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn snapshot(agents: serde_json::Value) -> PairedAgentsSnapshot {
        parse_paired_agents(&json!({ "agents": agents })).expect("agents array")
    }

    #[test]
    fn the_status_topic_is_the_contract_topic() {
        // Pinned to the contract's literal: the subscription and the handler
        // branch both use the const, so a typo would silently show nothing.
        assert_eq!(AGENT_STATUS_TOPIC, "system:agent:status");
    }

    #[test]
    fn a_later_snapshot_replaces_the_earlier_one_entirely() {
        // Sticky replay delivers several full snapshots oldest-first (the bus
        // keeps up to 100 per topic). The last one is the truth; nothing from
        // an earlier one survives, so c2 must be gone.
        let first = json!({"agents": [
            {"channel_id": "c1", "state": "connected", "since": 0},
            {"channel_id": "c2", "state": "paused", "since": 0}
        ]});
        let second = json!({"agents": [
            {"channel_id": "c1", "state": "paused", "since": 50}
        ]});
        let mut held: Option<PairedAgentsSnapshot> = None;
        for (payload, ts) in [(&first, 1_000u64), (&second, 2_000u64)] {
            if let Some(next) = next_paired_agents(held.as_ref(), payload, ts) {
                held = Some(next);
            }
        }
        let held = held.expect("a snapshot");
        assert_eq!(held.agents.len(), 1, "an entry from the first snapshot survived");
        assert_eq!(held.agents[0].channel_id, "c1");
        assert_eq!(held.agents[0].state, "paused");
        assert_eq!(held.published_ms, 2_000);
    }

    #[test]
    fn an_older_or_malformed_delivery_keeps_the_current_snapshot() {
        let current = next_paired_agents(
            None,
            &json!({"agents": [{"channel_id": "c1", "state": "paused", "since": 0}]}),
            2_000,
        )
        .expect("first snapshot");
        // A replayed snapshot published earlier than the one held.
        assert!(next_paired_agents(Some(&current), &json!({"agents": []}), 1_000).is_none());
        // No agents array at all.
        assert!(next_paired_agents(Some(&current), &json!({"oops": true}), 3_000).is_none());
        // Same timestamp: delivered later, so it wins.
        assert!(next_paired_agents(Some(&current), &json!({"agents": []}), 2_000).is_some());
    }

    #[test]
    fn parse_reads_the_contract_payload() {
        let s = snapshot(json!([
            {"channel_id": "c1", "label": null, "state": "connected", "since": 100,
             "tool": null, "approval": null},
            {"channel_id": "c2", "label": "Muse at home", "state": "connected", "since": 100,
             "tool": "browser_click",
             "approval": {"summary": "Click \"Submit\" on example.com", "started_at": 150}}
        ]));
        assert_eq!(s.agents.len(), 2);
        assert_eq!(s.agents[0].label, None);
        assert_eq!(s.agents[0].tool, None);
        assert_eq!(s.agents[1].label.as_deref(), Some("Muse at home"));
        let ap = s.agents[1].approval.as_ref().expect("approval");
        assert_eq!(ap.started_at, 150);
        assert!(ap.summary.contains("example.com"));
    }

    #[test]
    fn parse_drops_a_malformed_entry_but_keeps_the_rest() {
        let s = snapshot(json!([
            {"label": "no channel id"},
            {"channel_id": "c1", "state": "paused", "since": 1}
        ]));
        assert_eq!(s.agents.len(), 1);
        assert_eq!(s.agents[0].channel_id, "c1");
    }

    #[test]
    fn parse_refuses_a_payload_without_agents() {
        assert!(parse_paired_agents(&json!({})).is_none());
        assert!(parse_paired_agents(&json!({"agents": "none"})).is_none());
        assert_eq!(parse_paired_agents(&json!({"agents": []})).unwrap().agents.len(), 0);
    }

    #[test]
    fn an_idle_connected_agent_shows_how_long_it_has_been_connected() {
        let s = snapshot(json!([
            {"channel_id": "c1", "label": null, "state": "connected", "since": 1_000}
        ]));
        let rows = rows_from_status(&s, 1_000 + 12 * 60 + 30);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].label, "Muse");
        assert_eq!(rows[0].icon, "●");
        assert_eq!(rows[0].detail, "connected · 12 min");
        assert!(!rows[0].paused);
        assert_eq!(rows[0].channel_id, "c1");
    }

    #[test]
    fn a_working_agent_names_its_tool_in_short_words() {
        let s = snapshot(json!([
            {"channel_id": "c1", "label": "Muse", "state": "connected", "since": 0,
             "tool": "browser_screenshot"}
        ]));
        let rows = rows_from_status(&s, 10);
        assert_eq!(rows[0].detail, "working: screenshot");
        assert_eq!(rows[0].icon, "●");
    }

    #[test]
    fn an_unknown_tool_is_shown_by_its_name() {
        let s = snapshot(json!([
            {"channel_id": "c1", "state": "connected", "since": 0, "tool": "browser_new_thing"}
        ]));
        assert_eq!(rows_from_status(&s, 10)[0].detail, "working: browser_new_thing");
    }

    #[test]
    fn a_pending_approval_counts_down_from_sixty_and_stops_at_zero() {
        // `tool` is set too while an approval waits (contract §1); the
        // approval wins, because it is what the person must act on.
        let s = snapshot(json!([
            {"channel_id": "c1", "state": "connected", "since": 0, "tool": "browser_click",
             "approval": {"summary": "Click \"Submit\" on example.com", "started_at": 1_000}}
        ]));
        let at = |now: i64| rows_from_status(&s, now)[0].detail.clone();
        assert_eq!(
            at(1_000),
            "waiting for approval: Click \"Submit\" on example.com · 60 s left"
        );
        assert_eq!(
            at(1_018),
            "waiting for approval: Click \"Submit\" on example.com · 42 s left"
        );
        assert!(at(1_090).ends_with("· 0 s left"), "{}", at(1_090));
        // A sidebar clock behind the daemon's never shows more than the timeout.
        assert!(at(900).ends_with("· 60 s left"), "{}", at(900));
        assert_eq!(rows_from_status(&s, 1_018)[0].icon, "⏳");
    }

    #[test]
    fn a_paused_agent_offers_resume() {
        let s = snapshot(json!([
            {"channel_id": "c1", "label": "Muse", "state": "paused", "since": 0,
             "tool": "browser_click"}
        ]));
        let rows = rows_from_status(&s, 10);
        assert!(rows[0].paused);
        assert_eq!(rows[0].icon, "⏸");
        assert_eq!(rows[0].detail, "paused");
    }

    #[test]
    fn offline_and_unknown_states_are_not_shown() {
        let s = snapshot(json!([
            {"channel_id": "c1", "state": "offline", "since": 0},
            {"channel_id": "c2", "state": "reconnecting", "since": 0},
            {"channel_id": "c3", "since": 0}
        ]));
        assert!(rows_from_status(&s, 10).is_empty());
    }

    #[test]
    fn several_pairings_keep_the_daemon_order() {
        let s = snapshot(json!([
            {"channel_id": "c1", "label": "First", "state": "paused", "since": 0},
            {"channel_id": "c2", "label": "Gone", "state": "offline", "since": 0},
            {"channel_id": "c3", "label": "  ", "state": "connected", "since": 0}
        ]));
        let rows = rows_from_status(&s, 10);
        let labels: Vec<&str> = rows.iter().map(|r| r.label.as_str()).collect();
        assert_eq!(labels, ["First", "Muse"]);
    }

    #[test]
    fn a_disconnected_daemon_shows_no_rows() {
        let s = snapshot(json!([{"channel_id": "c1", "state": "connected", "since": 0}]));
        assert_eq!(bar_rows(Some(&s), true, 10).len(), 1);
        assert!(
            bar_rows(Some(&s), false, 10).is_empty(),
            "a snapshot from before the daemon went away is stale"
        );
        assert!(bar_rows(None, true, 10).is_empty());
    }

    #[test]
    fn connected_time_reads_in_minutes_and_hours() {
        assert_eq!(connected_for(0, 59), "under a minute");
        assert_eq!(connected_for(0, 60), "1 min");
        assert_eq!(connected_for(0, 59 * 60), "59 min");
        assert_eq!(connected_for(0, 125 * 60), "2 h 5 min");
        // A clock that went backwards is not a negative duration.
        assert_eq!(connected_for(100, 0), "under a minute");
    }

    #[test]
    fn every_whitelisted_tool_has_a_short_word() {
        // Written out by hand from the agent channel's whitelist (M2a §2.5),
        // not derived from TOOL_WORDS, so a tool added on either side without
        // the other fails here.
        let whitelist = [
            "browser_snapshot",
            "browser_get_markdown",
            "browser_screenshot",
            "browser_get_tabs",
            "browser_navigate",
            "browser_activate_tab",
            "browser_click_by_id",
            "browser_fill_by_id",
            "browser_type_by_id",
            "browser_click",
            "browser_fill",
            "browser_type",
            "browser_key_press",
            "browser_scroll",
            "browser_wait_for",
            "browser_upload_file",
        ];
        for tool in whitelist {
            let word = tool_word(tool).unwrap_or_else(|| panic!("no short word for {tool}"));
            assert!(!word.is_empty() && !word.starts_with("browser_"), "{tool} -> {word}");
        }
        assert_eq!(TOOL_WORDS.len(), whitelist.len());
        for (name, _) in TOOL_WORDS {
            assert!(whitelist.contains(&name), "{name} has a word but is not whitelisted");
        }
    }

    #[test]
    fn unpair_needs_a_second_click_within_three_seconds() {
        let (armed, send) = unpair_click(None, 10_000);
        assert_eq!((armed, send), (Some(10_000), false), "a single click never unpairs");
        assert_eq!(unpair_label(armed, 10_000), "Confirm unpair?");
        let (armed, send) = unpair_click(armed, 12_999);
        assert!(send);
        assert_eq!(armed, None);
        assert_eq!(unpair_label(armed, 13_000), "Unpair");
    }

    #[test]
    fn a_second_click_after_three_seconds_arms_again_instead_of_unpairing() {
        let (armed, _) = unpair_click(None, 10_000);
        assert_eq!(unpair_label(armed, 13_000), "Unpair", "the label reverts at 3 s");
        let (armed, send) = unpair_click(armed, 13_000);
        assert!(!send);
        assert_eq!(armed, Some(13_000));
        // A clock that went backwards does not count as "within 3 s".
        let (_, send) = unpair_click(Some(20_000), 19_000);
        assert!(!send);
    }

    #[test]
    fn the_daemon_must_confirm_pause_and_resume() {
        assert!(confirms_paused(&json!({"paused": true}), true).is_ok());
        assert!(confirms_paused(&json!({"paused": false, "opened": true}), false).is_ok());
        assert!(confirms_paused(&json!({"paused": false}), true).is_err());
        assert!(confirms_paused(&json!({}), true).is_err());
        assert_eq!(
            command_error("pause", "Unknown command: remote.agent_pause"),
            "Could not pause: Unknown command: remote.agent_pause"
        );
    }

    #[test]
    fn presence_words_follow_the_snapshot() {
        let s = snapshot(json!([
            {"channel_id": "c1", "state": "connected", "since": 0},
            {"channel_id": "c2", "state": "paused", "since": 0},
            {"channel_id": "c3", "state": "offline", "since": 0},
            {"channel_id": "c5", "state": "reconnecting", "since": 0}
        ]));
        assert_eq!(presence_word(&s, "c1"), Some("connected"));
        assert_eq!(presence_word(&s, "c2"), Some("paused"));
        assert_eq!(presence_word(&s, "c3"), Some("offline"));
        assert_eq!(presence_word(&s, "c5"), Some("offline"));
        assert_eq!(presence_word(&s, "c4"), None);
    }
}
