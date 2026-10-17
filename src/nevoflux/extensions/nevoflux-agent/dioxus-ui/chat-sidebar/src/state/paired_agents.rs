/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Paired AI agents as the desktop sees them right now (M2b §3).
//!
//! The daemon publishes `system:agent:status`: a sticky snapshot that is
//! always complete, one entry per agent pairing. Everything the sidebar shows
//! about it is derived here, with no signals and no browser, so it runs under
//! `cargo test --target x86_64-pc-windows-msvc`.

use serde::{Deserialize, Deserializer};

/// The sticky EventBus topic the daemon publishes the snapshot on.
pub const AGENT_STATUS_TOPIC: &str = "system:agent:status";

/// Subscribe buffer for `system:agent:status`. The daemon keeps up to
/// `STICKY_MAX_PER_TOPIC` = 100 sticky events per topic and replays them
/// oldest-first with `try_send`, dropping overflow; a smaller buffer could
/// leave a newly opened sidebar holding a stale snapshot.
pub const AGENT_STATUS_BUFFER: usize = 256;

/// Shown when a pairing has no label (the contract allows `label: null`).
pub const DEFAULT_LABEL: &str = "Muse";

/// How long the daemon waits for an approval (M2a `APPROVAL_TIMEOUT`).
pub const APPROVAL_TIMEOUT_SECS: i64 = 60;

/// How long "Unpair" waits for its confirming second click.
pub const UNPAIR_CONFIRM_MS: i64 = 3000;

/// A second click sooner than this after arming is treated as a double-click
/// and ignored: unpairing cannot be undone.
pub const UNPAIR_MIN_GAP_MS: i64 = 400;

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
    /// The task this pairing delegated and NevoFlux is running (M3 contract
    /// §4). A malformed one is dropped on its own: it must not hide the row.
    #[serde(default, deserialize_with = "lenient_task")]
    pub task: Option<AgentTask>,
    /// The person let this agent answer its tasks' must-approve approvals
    /// (M3 contract v1.2 R39). Absent (an older daemon) reads as off.
    #[serde(default)]
    pub muse_may_approve_guarded: bool,
}

fn lenient_task<'de, D: Deserializer<'de>>(d: D) -> Result<Option<AgentTask>, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(d)?;
    Ok(value.and_then(|v| serde_json::from_value(v).ok()))
}

/// A delegated task a pairing is running (M3 contract §4).
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct AgentTask {
    pub id: String,
    pub session_id: String,
    /// The prompt's first line, already cut to 80 characters by the daemon.
    #[serde(default)]
    pub summary: String,
    /// `running` | `awaiting_approval`; the daemon sends `task: null` once it ends.
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub started_at: i64,
    #[serde(default)]
    pub approval: Option<TaskApproval>,
}

/// The oldest approval a delegated task waits on.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct TaskApproval {
    pub approval_id: String,
    /// `tool` | `question` | `plan`.
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub summary: String,
    /// What the desktop may answer: `allow`, `allow_always`, `deny` (§4).
    #[serde(default)]
    pub choices: Vec<String>,
    #[serde(default)]
    pub started_at: i64,
    #[serde(default)]
    pub expires_at: i64,
    /// Raised by the must-approve list (contract v1.2 §4): `Allow` / `Deny`
    /// only, whatever the daemon's `choices` say.
    #[serde(default)]
    pub must_approve: bool,
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
    /// The delegated task under the row (M3 §6.4), if one is running.
    pub task: Option<TaskLine>,
    /// What the "Let Muse approve sensitive actions" toggle shows (R39).
    pub muse_may_approve_guarded: bool,
}

/// The status-bar lines for a delegated task (contract §8).
#[derive(Debug, Clone, PartialEq)]
pub struct TaskLine {
    pub task_id: String,
    pub session_id: String,
    /// `<label> task: <summary> · <tool>`.
    pub text: String,
    pub approval: Option<TaskApprovalLine>,
}

/// The approval line: what is asked and the buttons the desktop may press.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskApprovalLine {
    pub approval_id: String,
    /// `<label> task needs approval: <summary>`.
    pub text: String,
    /// (button text, choice sent), in the daemon's order.
    pub buttons: Vec<(&'static str, &'static str)>,
    /// `Muse may also answer` on a must-approve approval while the pairing
    /// lets the agent answer it (contract v1.2 R39).
    pub hint: Option<&'static str>,
}

/// The toggle in the paired-agent row (contract v1.2 R39, exact copy).
pub const GUARDED_TOGGLE_LABEL: &str = "Let Muse approve sensitive actions";
pub const GUARDED_TOGGLE_HELP: &str = "Muse can approve scheduling, packs, model switching, screen sharing and prompt changes for its tasks. Off: only you can, here.";
/// The hint on a must-approve approval line while the toggle is on.
pub const MUSE_MAY_ALSO_ANSWER: &str = "Muse may also answer";

/// Whether a `remote.agent_set_guarded_approval` answer confirms the state
/// that was asked for.
pub fn confirms_guarded(data: &serde_json::Value, want: bool) -> Result<(), String> {
    match data.get("enabled").and_then(|v| v.as_bool()) {
        Some(e) if e == want => Ok(()),
        _ => Err(format!(
            "the daemon did not confirm the setting is {}",
            if want { "on" } else { "off" }
        )),
    }
}

/// The name the bar shows for a pairing: its label, or "Muse".
pub fn agent_label(agent: &PairedAgentEntry) -> String {
    agent
        .label
        .as_deref()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .unwrap_or(DEFAULT_LABEL)
        .to_string()
}

fn choice_button(choice: &str) -> Option<(&'static str, &'static str)> {
    match choice {
        "allow" => Some(("Allow", "allow")),
        "allow_always" => Some(("Always allow", "allow_always")),
        "deny" => Some(("Deny", "deny")),
        _ => None,
    }
}

fn tool_text(tool: &str) -> &str {
    tool_word(tool).unwrap_or(tool)
}

/// The task lines under an agent's row, or `None` when it runs no task.
pub fn task_line(agent: &PairedAgentEntry) -> Option<TaskLine> {
    let task = agent.task.as_ref()?;
    if task.state != "running" && task.state != "awaiting_approval" {
        return None;
    }
    let label = agent_label(agent);
    let mut text = format!("{label} task: {}", task.summary);
    if let Some(tool) = task.tool.as_deref().filter(|t| !t.is_empty()) {
        text.push_str(" · ");
        text.push_str(tool_text(tool));
    }
    let approval = task.approval.as_ref().map(|ap| TaskApprovalLine {
        approval_id: ap.approval_id.clone(),
        text: format!("{label} task needs approval: {}", ap.summary),
        buttons: ap
            .choices
            .iter()
            // Contract v1.2 R39: never "Always allow" for a must-approve tool.
            .filter(|c| !(ap.must_approve && c.as_str() == "allow_always"))
            .filter_map(|c| choice_button(c))
            .collect(),
        hint: (ap.must_approve && agent.muse_may_approve_guarded).then_some(MUSE_MAY_ALSO_ANSWER),
    });
    Some(TaskLine {
        task_id: task.id.clone(),
        session_id: task.session_id.clone(),
        text,
        approval,
    })
}

/// The pairing and task running in `session_id`, if a delegated task is.
pub fn task_for_session<'a>(
    snapshot: Option<&'a PairedAgentsSnapshot>,
    session_id: &str,
) -> Option<(&'a PairedAgentEntry, &'a AgentTask)> {
    if session_id.is_empty() {
        return None;
    }
    snapshot?.agents.iter().find_map(|a| {
        a.task
            .as_ref()
            .filter(|t| t.session_id == session_id)
            .map(|t| (a, t))
    })
}

/// The line over a conversation a paired agent is running (M3 §6.1).
pub fn task_banner(agent: &PairedAgentEntry, task: &AgentTask) -> String {
    let label = agent_label(agent);
    if let Some(ap) = &task.approval {
        return format!("{label} is waiting for approval: {}", ap.summary);
    }
    match task.tool.as_deref().filter(|t| !t.is_empty()) {
        Some(tool) => format!("{label} is working on this task · {}", tool_text(tool)),
        None => format!("{label} is working on this task"),
    }
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
/// Offline agents, and states this build does not know, are shown only while
/// their entry carries a live delegated task.
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
                            "waiting for approval · {} s left: {}",
                            seconds_left(ap.started_at, now),
                            ap.summary
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
                // B-F1: the daemon keeps running a delegated task while the
                // agent is disconnected and publishes it on the offline entry,
                // so such an entry still gets a row (for its task line).
                _ if task_line(a).is_some() => ("○", "offline".to_string(), false),
                _ => return None,
            };
            Some(AgentRow {
                channel_id: a.channel_id.clone(),
                label: agent_label(a),
                icon,
                detail,
                paused,
                task: task_line(a),
                muse_may_approve_guarded: a.muse_may_approve_guarded,
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
/// The first click only arms; a second within 400 ms is ignored (double-click);
/// one from 400 ms to 3 s sends; a later one arms
/// again (§3.3). Unpairing cannot be undone, so nothing else sends.
pub fn unpair_click(armed_at: Option<i64>, now_ms: i64) -> (Option<i64>, bool) {
    if let Some(at) = armed_at {
        let gap = now_ms - at;
        if (0..UNPAIR_MIN_GAP_MS).contains(&gap) {
            // Fast double-click: stay armed, neither send nor re-arm.
            return (armed_at, false);
        }
    }
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
            "waiting for approval · 60 s left: Click \"Submit\" on example.com"
        );
        assert_eq!(
            at(1_018),
            "waiting for approval · 42 s left: Click \"Submit\" on example.com"
        );
        assert!(at(1_090).starts_with("waiting for approval · 0 s left:"), "{}", at(1_090));
        // A sidebar clock behind the daemon's never shows more than the timeout.
        assert!(at(900).starts_with("waiting for approval · 60 s left:"), "{}", at(900));
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
    fn a_fast_double_click_neither_sends_nor_rearms() {
        let (armed, _) = unpair_click(None, 10_000);
        let (a2, send) = unpair_click(armed, 10_150);
        assert_eq!((a2, send), (Some(10_000), false));
        let (a3, send) = unpair_click(armed, 10_399);
        assert_eq!((a3, send), (Some(10_000), false));
        let (a4, send) = unpair_click(armed, 10_400);
        assert_eq!((a4, send), (None, true));
    }

    #[test]
    fn agent_status_buffer_holds_the_daemons_whole_sticky_replay() {
        // Daemon STICKY_MAX_PER_TOPIC = 100.
        assert!(AGENT_STATUS_BUFFER >= 100);
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

    fn task_snapshot(task: serde_json::Value) -> PairedAgentsSnapshot {
        snapshot(json!([
            {"channel_id": "c1", "label": null, "state": "connected", "since": 0, "task": task}
        ]))
    }

    #[test]
    fn parse_reads_a_task_and_its_approval() {
        let s = task_snapshot(json!({
            "id": "t1", "session_id": "s1", "summary": "Generate a calculator Canvas",
            "state": "awaiting_approval", "tool": "canvas_render", "started_at": 1792000000,
            "approval": {"approval_id": "a1", "kind": "tool", "summary": "Submit a form on example.com",
                         "choices": ["allow", "allow_always", "deny"],
                         "started_at": 1792000123, "expires_at": 1792001923}
        }));
        let task = s.agents[0].task.as_ref().expect("task");
        assert_eq!(task.id, "t1");
        assert_eq!(task.session_id, "s1");
        assert_eq!(task.state, "awaiting_approval");
        let ap = task.approval.as_ref().expect("approval");
        assert_eq!(ap.choices, ["allow", "allow_always", "deny"]);
        assert_eq!(ap.expires_at, 1792001923);
    }

    #[test]
    fn a_malformed_task_does_not_hide_the_agent() {
        let s = task_snapshot(json!({"summary": "no ids"}));
        assert_eq!(s.agents.len(), 1, "the row must survive a bad task");
        assert!(s.agents[0].task.is_none());
        let none = task_snapshot(serde_json::Value::Null);
        assert!(none.agents[0].task.is_none());
    }

    #[test]
    fn a_running_task_reads_label_summary_and_tool() {
        let s = task_snapshot(json!({
            "id": "t1", "session_id": "s1", "summary": "Generate a calculator Canvas",
            "state": "running", "tool": "browser_navigate", "started_at": 0, "approval": null
        }));
        let line = task_line(&s.agents[0]).expect("a line");
        assert_eq!(line.text, "Muse task: Generate a calculator Canvas · opening a page");
        assert_eq!(line.task_id, "t1");
        assert_eq!(line.session_id, "s1");
        assert!(line.approval.is_none());
        // A tool outside the 16 is shown by its name; no tool, no separator.
        let other = task_snapshot(json!({"id": "t1", "session_id": "s1", "summary": "X",
            "state": "running", "tool": "canvas_render"}));
        assert_eq!(task_line(&other.agents[0]).unwrap().text, "Muse task: X · canvas_render");
        let idle = task_snapshot(json!({"id": "t1", "session_id": "s1", "summary": "X", "state": "running"}));
        assert_eq!(task_line(&idle.agents[0]).unwrap().text, "Muse task: X");
    }

    #[test]
    fn the_pairing_label_replaces_muse() {
        let s = snapshot(json!([
            {"channel_id": "c1", "label": "Muse at home", "state": "connected", "since": 0,
             "task": {"id": "t1", "session_id": "s1", "summary": "X", "state": "running"}}
        ]));
        assert_eq!(task_line(&s.agents[0]).unwrap().text, "Muse at home task: X");
        assert_eq!(agent_label(&s.agents[0]), "Muse at home");
    }

    #[test]
    fn an_approval_offers_exactly_the_desktop_choices() {
        let line = |choices: serde_json::Value| {
            let s = task_snapshot(json!({"id": "t1", "session_id": "s1", "summary": "X",
                "state": "awaiting_approval",
                "approval": {"approval_id": "a1", "kind": "tool", "summary": "Click Submit",
                             "choices": choices}}));
            task_line(&s.agents[0]).unwrap().approval.expect("approval line")
        };
        let tool = line(json!(["allow", "allow_always", "deny"]));
        assert_eq!(tool.text, "Muse task needs approval: Click Submit");
        assert_eq!(tool.approval_id, "a1");
        assert_eq!(
            tool.buttons,
            vec![("Allow", "allow"), ("Always allow", "allow_always"), ("Deny", "deny")]
        );
        // A question can only be denied from the desktop (R16); unknown choices are dropped.
        assert_eq!(line(json!(["deny"])).buttons, vec![("Deny", "deny")]);
        assert_eq!(line(json!(["allow", "later", "deny"])).buttons, vec![("Allow", "allow"), ("Deny", "deny")]);
    }

    #[test]
    fn a_task_in_another_state_shows_no_line() {
        let s = task_snapshot(json!({"id": "t1", "session_id": "s1", "summary": "X", "state": "done"}));
        assert!(task_line(&s.agents[0]).is_none());
        let rows = rows_from_status(&s, 10);
        assert!(rows[0].task.is_none());
    }

    #[test]
    fn the_row_carries_its_task_line() {
        let s = task_snapshot(json!({"id": "t1", "session_id": "s1", "summary": "X", "state": "running"}));
        let rows = rows_from_status(&s, 10);
        assert_eq!(rows[0].task.as_ref().map(|t| t.task_id.as_str()), Some("t1"));
    }

    #[test]
    fn task_for_session_finds_the_running_task() {
        let s = snapshot(json!([
            {"channel_id": "c1", "state": "connected", "since": 0},
            {"channel_id": "c2", "label": "Work", "state": "connected", "since": 0,
             "task": {"id": "t2", "session_id": "s2", "summary": "X", "state": "running"}}
        ]));
        let (agent, task) = task_for_session(Some(&s), "s2").expect("found");
        assert_eq!(agent.channel_id, "c2");
        assert_eq!(task.id, "t2");
        assert!(task_for_session(Some(&s), "s1").is_none());
        assert!(task_for_session(Some(&s), "").is_none());
        assert!(task_for_session(None, "s2").is_none());
    }

    #[test]
    fn the_banner_says_who_works_on_this_conversation() {
        let s = task_snapshot(json!({"id": "t1", "session_id": "s1", "summary": "X",
            "state": "running", "tool": "browser_click"}));
        let (agent, task) = task_for_session(Some(&s), "s1").unwrap();
        assert_eq!(task_banner(agent, task), "Muse is working on this task · clicking");
        let waiting = task_snapshot(json!({"id": "t1", "session_id": "s1", "summary": "X",
            "state": "awaiting_approval",
            "approval": {"approval_id": "a1", "summary": "Click Submit", "choices": ["deny"]}}));
        let (agent, task) = task_for_session(Some(&waiting), "s1").unwrap();
        assert_eq!(task_banner(agent, task), "Muse is waiting for approval: Click Submit");
        let quiet = task_snapshot(json!({"id": "t1", "session_id": "s1", "summary": "X", "state": "running"}));
        let (agent, task) = task_for_session(Some(&quiet), "s1").unwrap();
        assert_eq!(task_banner(agent, task), "Muse is working on this task");
    }

    #[test]
    fn the_guarded_approval_toggle_reads_the_snapshot_and_defaults_off() {
        let s = snapshot(json!([
            {"channel_id": "c1", "state": "connected", "since": 0, "muse_may_approve_guarded": true},
            {"channel_id": "c2", "state": "paused", "since": 0}
        ]));
        assert!(s.agents[0].muse_may_approve_guarded);
        assert!(!s.agents[1].muse_may_approve_guarded, "no key (an older daemon) reads as off");
        let rows = rows_from_status(&s, 10);
        assert!(rows[0].muse_may_approve_guarded);
        assert!(!rows[1].muse_may_approve_guarded);
        assert_eq!(GUARDED_TOGGLE_LABEL, "Let Muse approve sensitive actions");
        assert_eq!(
            GUARDED_TOGGLE_HELP,
            "Muse can approve scheduling, packs, model switching, screen sharing and prompt changes for its tasks. Off: only you can, here."
        );
    }

    #[test]
    fn a_must_approve_line_offers_allow_or_deny_and_hints_when_muse_may_answer() {
        let line = |guarded: bool, choices: serde_json::Value| {
            let s = snapshot(json!([{"channel_id": "c1", "label": "Muse at home", "state": "connected",
                "since": 0, "muse_may_approve_guarded": guarded,
                "task": {"id": "t1", "session_id": "s1", "summary": "X", "state": "awaiting_approval",
                         "approval": {"approval_id": "a1", "kind": "tool", "must_approve": true,
                                      "summary": "Run schedule_create", "choices": choices}}}]));
            task_line(&s.agents[0]).unwrap().approval.expect("approval line")
        };
        let off = line(false, json!(["allow", "deny"]));
        assert_eq!(off.buttons, vec![("Allow", "allow"), ("Deny", "deny")]);
        assert_eq!(off.hint, None);
        let on = line(true, json!(["allow", "deny"]));
        assert_eq!(on.buttons, vec![("Allow", "allow"), ("Deny", "deny")]);
        assert_eq!(on.hint, Some("Muse may also answer"), "exact, not relabelled");
        // Never "Always allow" for these, even if a daemon offered it.
        assert_eq!(
            line(true, json!(["allow", "allow_always", "deny"])).buttons,
            vec![("Allow", "allow"), ("Deny", "deny")]
        );
        // An ordinary approval gets no hint, toggle on or not.
        let s = snapshot(json!([{"channel_id": "c1", "state": "connected", "since": 0,
            "muse_may_approve_guarded": true,
            "task": {"id": "t1", "session_id": "s1", "summary": "X", "state": "awaiting_approval",
                     "approval": {"approval_id": "a2", "kind": "tool", "summary": "Click Submit",
                                  "choices": ["allow", "allow_always", "deny"]}}}]));
        let ordinary = task_line(&s.agents[0]).unwrap().approval.unwrap();
        assert_eq!(ordinary.hint, None);
        assert_eq!(ordinary.buttons.len(), 3);
    }

    #[test]
    fn the_toggle_answer_must_confirm_the_state_asked_for() {
        assert!(confirms_guarded(&json!({"enabled": true}), true).is_ok());
        assert!(confirms_guarded(&json!({"enabled": false}), false).is_ok());
        assert!(confirms_guarded(&json!({"enabled": false}), true).is_err());
        assert!(confirms_guarded(&json!({}), true).is_err());
    }

    #[test]
    fn an_offline_agent_with_a_task_still_gets_a_row() {
        // B-F1: the daemon keeps running the task while Muse is disconnected
        // and publishes it on the offline entry.
        let s = snapshot(json!([
            {"channel_id": "c1", "label": "Muse", "state": "offline", "since": 0,
             "task": {"id": "t1", "session_id": "s1", "summary": "X", "state": "running"}},
            {"channel_id": "c2", "state": "offline", "since": 0,
             "task": {"id": "t2", "session_id": "s2", "summary": "Y", "state": "done"}},
            {"channel_id": "c3", "state": "offline", "since": 0}
        ]));
        let rows = rows_from_status(&s, 10);
        assert_eq!(rows.len(), 1, "only the offline entry with a live task");
        assert_eq!(rows[0].channel_id, "c1");
        assert_eq!(rows[0].detail, "offline");
        assert!(!rows[0].paused);
        assert_eq!(rows[0].task.as_ref().map(|t| t.task_id.as_str()), Some("t1"));
    }

    #[test]
    fn the_task_tool_shows_the_short_word_for_every_m2_tool() {
        for (name, word) in TOOL_WORDS {
            let s = task_snapshot(json!({"id": "t1", "session_id": "s1", "summary": "X",
                "state": "running", "tool": name}));
            assert_eq!(task_line(&s.agents[0]).unwrap().text, format!("Muse task: X · {word}"));
        }
    }

    #[test]
    fn parse_reads_the_exact_shape_of_the_daemons_status_json() {
        // Copied from the daemon: TaskRecord::status_json inside an entry built
        // by agent_status.rs (muse_may_approve_guarded, since, tool, approval, task).
        let s = parse_paired_agents(&json!({"agents": [{
            "channel_id": "ctl-1", "label": "Muse at home", "state": "connected",
            "muse_may_approve_guarded": true, "since": 1792000000,
            "tool": "browser_click", "approval": null,
            "task": {
                "id": "task-9", "session_id": "sess-9", "summary": "Book a table",
                "state": "awaiting_approval", "tool": "schedule_create",
                "started_at": 1792000001,
                "approval": {"approval_id": "ap-1", "kind": "tool",
                             "summary": "Run schedule_create", "choices": ["allow", "deny"],
                             "started_at": 1792000100, "expires_at": 1792001900,
                             "must_approve": true}
            }
        }]})).unwrap();
        let e = &s.agents[0];
        assert!(e.muse_may_approve_guarded);
        let t = e.task.as_ref().expect("task");
        assert_eq!((t.id.as_str(), t.session_id.as_str(), t.tool.as_deref()), ("task-9", "sess-9", Some("schedule_create")));
        assert_eq!(t.started_at, 1792000001);
        let ap = t.approval.as_ref().unwrap();
        assert!(ap.must_approve);
        assert_eq!((ap.started_at, ap.expires_at), (1792000100, 1792001900));
        let line = task_line(e).unwrap();
        assert_eq!(line.approval.unwrap().hint, Some(MUSE_MAY_ALSO_ANSWER));
        // A task: null entry (daemon shape when idle) parses with no task.
        let idle = parse_paired_agents(&json!({"agents": [{"channel_id": "a", "label": null,
            "state": "offline", "muse_may_approve_guarded": false, "since": 200,
            "tool": null, "approval": null, "task": null}]})).unwrap();
        assert!(idle.agents[0].task.is_none());
    }
}
