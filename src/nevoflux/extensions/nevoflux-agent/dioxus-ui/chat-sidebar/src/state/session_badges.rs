/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! The history list's view of delegated conversations (M3 §6.2).
//!
//! `session.list` says who started a session (`origin`: a paired agent) and
//! whether a turn runs in it (`runtime`). "Finished, unread" is the sidebar's
//! own (contract R18): it remembers the `updated_at` each delegated session
//! had when it was last on screen, in localStorage so the mark survives the
//! sidebar being closed while the agent works. Everything but `load_seen` /
//! `save_seen` is pure and tested natively.

use std::collections::BTreeMap;

use serde::Deserialize;

use super::paired_agents::{agent_label, PairedAgentsSnapshot, DEFAULT_LABEL};
use super::SessionSummary;

/// localStorage key of the seen map.
pub const SEEN_KEY: &str = "nevoflux.sidebar.museSeen";

/// At most this many sessions are remembered (the least recent go first).
pub const SEEN_CAP: usize = 500;

/// Session id -> the `updated_at` it had when last on screen.
pub type SeenMap = BTreeMap<String, u64>;

/// Who started a session, when a paired agent did (contract §3).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct SessionOrigin {
    pub kind: String,
    /// The pairing's control channel id.
    #[serde(default)]
    pub pairing: String,
}

/// A turn running in a session, from `session.list` `runtime`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRuntime {
    Running,
    AwaitingApproval,
}

/// The state shown next to a session in the history list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionBadge {
    Running,
    AwaitingApproval,
    Finished,
}

impl SessionBadge {
    pub fn label(self) -> &'static str {
        match self {
            Self::Running => "Running",
            Self::AwaitingApproval => "Needs approval",
            Self::Finished => "Finished",
        }
    }

    pub fn class(self) -> &'static str {
        match self {
            Self::Running => "history-badge running",
            Self::AwaitingApproval => "history-badge awaiting",
            Self::Finished => "history-badge finished",
        }
    }
}

/// A session's origin, only when a paired agent started it.
pub fn parse_origin(value: Option<&serde_json::Value>) -> Option<SessionOrigin> {
    let origin: SessionOrigin = serde_json::from_value(value?.clone()).ok()?;
    (origin.kind == "muse").then_some(origin)
}

/// A session's runtime; anything this build does not know is none.
pub fn parse_runtime(value: Option<&serde_json::Value>) -> Option<SessionRuntime> {
    match value?.as_str()? {
        "running" => Some(SessionRuntime::Running),
        "awaiting_approval" => Some(SessionRuntime::AwaitingApproval),
        _ => None,
    }
}

/// The badge text for the pairing that started a session: its label, or "Muse".
pub fn origin_label(origin: &SessionOrigin, snapshot: Option<&PairedAgentsSnapshot>) -> String {
    snapshot
        .and_then(|s| s.agents.iter().find(|a| a.channel_id == origin.pairing))
        .map(agent_label)
        .unwrap_or_else(|| DEFAULT_LABEL.to_string())
}

/// The badge for a session: waiting for an approval beats running, running
/// beats finished. `running_here` is the sidebar's own knowledge (a frame
/// from it arrived off screen); `task_state` is the live status snapshot's.
pub fn session_badge(
    runtime: Option<SessionRuntime>,
    running_here: bool,
    task_state: Option<&str>,
    unread: bool,
) -> Option<SessionBadge> {
    if task_state == Some("awaiting_approval") || runtime == Some(SessionRuntime::AwaitingApproval) {
        return Some(SessionBadge::AwaitingApproval);
    }
    if task_state == Some("running") || runtime == Some(SessionRuntime::Running) || running_here {
        return Some(SessionBadge::Running);
    }
    unread.then_some(SessionBadge::Finished)
}

/// The stored map, or `None` when there is none or it cannot be read.
pub fn parse_seen(raw: Option<&str>) -> Option<SeenMap> {
    serde_json::from_str(raw?).ok()
}

/// Remember `updated_at` for `id` (never moves backwards).
pub fn mark_seen(seen: &mut SeenMap, id: &str, updated_at: u64) {
    let entry = seen.entry(id.to_string()).or_insert(0);
    if updated_at > *entry {
        *entry = updated_at;
    }
}

/// How far past a watched turn's end its closing writes may still move
/// `updated_at`: the daemon ends the task about 1 s after the done frame
/// (its turn-end grace is 5 s), and `updated_at` is in seconds.
pub const WATCHED_GRACE_SECS: u64 = 10;

/// A delegated turn ended while its conversation was on screen: it counts as
/// read, including the writes that close the task just after the done frame
/// (the list refreshed at done can still say "running"). `now_secs` is the
/// wall clock in seconds, like `updated_at`.
pub fn mark_watched_end(seen: &mut SeenMap, id: &str, now_secs: u64) {
    if !id.is_empty() {
        mark_seen(seen, id, now_secs.saturating_add(WATCHED_GRACE_SECS));
    }
}

/// The map after a list load: with no stored map, every listed delegated
/// session counts as seen (a baseline, so old ones do not all light up); the
/// conversation on screen is always seen.
pub fn reconcile_seen(seen: Option<SeenMap>, sessions: &[SessionSummary], current: &str) -> SeenMap {
    let baseline = seen.is_none();
    let mut seen = seen.unwrap_or_default();
    for s in sessions.iter().filter(|s| s.origin.is_some()) {
        if baseline || s.id == current {
            mark_seen(&mut seen, &s.id, s.updated_at);
        }
    }
    while seen.len() > SEEN_CAP {
        let oldest = seen.iter().min_by_key(|(_, at)| **at).map(|(id, _)| id.clone());
        match oldest {
            Some(id) => {
                seen.remove(&id);
            }
            None => break,
        }
    }
    seen
}

/// Whether a delegated session finished since it was last on screen.
pub fn is_unread(session: &SessionSummary, seen: &SeenMap, current: &str) -> bool {
    session.origin.is_some()
        && session.id != current
        && session.runtime.is_none()
        && seen.get(&session.id).map_or(true, |&at| session.updated_at > at)
}

/// The stored seen map (wasm only; `None` when absent or unreadable).
pub fn load_seen() -> Option<SeenMap> {
    let storage = web_sys::window()?.local_storage().ok()??;
    let raw = storage.get_item(SEEN_KEY).ok()?;
    parse_seen(raw.as_deref())
}

/// Store the seen map (wasm only; failures are ignored: the mark is a courtesy).
pub fn save_seen(seen: &SeenMap) {
    let Some(storage) = web_sys::window().and_then(|w| w.local_storage().ok().flatten()) else {
        return;
    };
    if let Ok(text) = serde_json::to_string(seen) {
        let _ = storage.set_item(SEEN_KEY, &text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::paired_agents::parse_paired_agents;
    use serde_json::json;

    fn summary(id: &str, updated_at: u64, muse: bool, runtime: Option<SessionRuntime>) -> SessionSummary {
        SessionSummary {
            id: id.to_string(),
            title: None,
            updated_at,
            message_count: 2,
            pinned: false,
            origin: muse.then(|| SessionOrigin { kind: "muse".into(), pairing: "c1".into() }),
            runtime,
        }
    }

    #[test]
    fn only_a_muse_origin_marks_a_session() {
        let o = parse_origin(Some(&json!({"kind": "muse", "pairing": "c1"}))).expect("muse");
        assert_eq!(o.pairing, "c1");
        assert!(parse_origin(Some(&json!({"kind": "user", "pairing": "c1"}))).is_none());
        assert!(parse_origin(Some(&json!(null))).is_none());
        assert!(parse_origin(None).is_none());
    }

    #[test]
    fn the_list_reads_runtime() {
        assert_eq!(parse_runtime(Some(&json!("running"))), Some(SessionRuntime::Running));
        assert_eq!(parse_runtime(Some(&json!("awaiting_approval"))), Some(SessionRuntime::AwaitingApproval));
        assert_eq!(parse_runtime(Some(&json!("paused"))), None);
        assert_eq!(parse_runtime(Some(&json!(null))), None);
        assert_eq!(parse_runtime(None), None);
    }

    #[test]
    fn the_badge_label_is_the_pairing_label_or_muse() {
        let snap = parse_paired_agents(&json!({"agents": [
            {"channel_id": "c1", "label": "Muse at home", "state": "connected", "since": 0}
        ]}))
        .unwrap();
        let o = SessionOrigin { kind: "muse".into(), pairing: "c1".into() };
        assert_eq!(origin_label(&o, Some(&snap)), "Muse at home");
        let gone = SessionOrigin { kind: "muse".into(), pairing: "c9".into() };
        assert_eq!(origin_label(&gone, Some(&snap)), "Muse");
        assert_eq!(origin_label(&o, None), "Muse");
    }

    #[test]
    fn waiting_beats_running_beats_finished() {
        use SessionBadge::*;
        assert_eq!(session_badge(Some(SessionRuntime::Running), false, Some("awaiting_approval"), true), Some(AwaitingApproval));
        assert_eq!(session_badge(Some(SessionRuntime::AwaitingApproval), false, None, false), Some(AwaitingApproval));
        assert_eq!(session_badge(None, true, None, true), Some(Running));
        assert_eq!(session_badge(None, false, Some("running"), false), Some(Running));
        assert_eq!(session_badge(Some(SessionRuntime::Running), false, None, false), Some(Running));
        assert_eq!(session_badge(None, false, None, true), Some(Finished));
        assert_eq!(session_badge(None, false, None, false), None);
        assert_eq!(AwaitingApproval.label(), "Needs approval");
        assert_eq!(Finished.class(), "history-badge finished");
    }

    #[test]
    fn the_first_load_takes_every_delegated_session_as_seen() {
        let list = [summary("a", 100, true, None), summary("b", 200, true, None), summary("u", 300, false, None)];
        let seen = reconcile_seen(None, &list, "x");
        assert_eq!(seen.get("a"), Some(&100));
        assert_eq!(seen.get("b"), Some(&200));
        assert!(seen.get("u").is_none(), "the person's own sessions are not tracked");
        assert!(list.iter().all(|s| !is_unread(s, &seen, "x")));
    }

    #[test]
    fn a_new_delegated_session_is_unread_once_it_finishes() {
        let seen = reconcile_seen(Some(SeenMap::new()), &[], "x");
        let running = summary("t", 500, true, Some(SessionRuntime::Running));
        assert!(!is_unread(&running, &seen, "x"), "running is not finished");
        let done = summary("t", 520, true, None);
        assert!(is_unread(&done, &seen, "x"));
        // The sidebar was closed meanwhile: the stored map comes back the same.
        let stored = serde_json::to_string(&seen).unwrap();
        let reloaded = parse_seen(Some(&stored)).unwrap();
        assert!(is_unread(&done, &reloaded, "x"));
    }

    #[test]
    fn opening_a_session_clears_its_mark_until_it_changes_again() {
        let mut seen = SeenMap::new();
        let done = summary("t", 520, true, None);
        mark_seen(&mut seen, "t", 520);
        assert!(!is_unread(&done, &seen, "x"));
        assert!(is_unread(&summary("t", 600, true, None), &seen, "x"), "a follow-up task finished");
        mark_seen(&mut seen, "t", 10);
        assert_eq!(seen.get("t"), Some(&520), "never moves backwards");
    }

    #[test]
    fn the_conversation_on_screen_is_never_unread() {
        let list = [summary("t", 520, true, None)];
        let seen = reconcile_seen(Some(SeenMap::new()), &list, "t");
        assert_eq!(seen.get("t"), Some(&520));
        assert!(!is_unread(&list[0], &seen, "t"));
    }

    #[test]
    fn a_turn_that_ended_on_screen_stays_read_after_the_task_closes() {
        // The list refreshed at done still says "running" (updated_at 1000)
        // while the person watches; the daemon closes the task a second later.
        let refreshed = [summary("t", 1000, true, Some(SessionRuntime::Running))];
        let mut seen = reconcile_seen(Some(SeenMap::new()), &refreshed, "t");
        mark_watched_end(&mut seen, "t", 1000);
        // Later, from another conversation, the closed task has moved on a bit.
        let closed = summary("t", 1001, true, None);
        let seen = reconcile_seen(Some(seen), std::slice::from_ref(&closed), "x");
        assert!(!is_unread(&closed, &seen, "x"), "watched, so read");
        // A follow-up task finishing later is new again.
        assert!(is_unread(&summary("t", 1000 + WATCHED_GRACE_SECS + 60, true, None), &seen, "x"));
        // No session id, no mark.
        let mut empty = SeenMap::new();
        mark_watched_end(&mut empty, "", 1000);
        assert!(empty.is_empty());
    }

    #[test]
    fn a_bad_origin_never_drops_the_row_it_is_on() {
        // Lenient: an origin of the wrong shape is just "not delegated".
        assert!(parse_origin(Some(&json!("muse"))).is_none());
        assert!(parse_origin(Some(&json!({"pairing": "c1"}))).is_none());
        assert!(parse_origin(Some(&json!({"kind": 5}))).is_none());
        let o = parse_origin(Some(&json!({"kind": "muse"}))).expect("pairing defaults");
        assert_eq!(o.pairing, "");
        assert_eq!(parse_runtime(Some(&json!(3))), None);
    }

    #[test]
    fn garbled_storage_starts_a_fresh_baseline_instead_of_marking_everything() {
        assert!(parse_seen(Some("{not json")).is_none());
        assert!(parse_seen(None).is_none());
        let list = [summary("a", 100, true, None)];
        let seen = reconcile_seen(parse_seen(Some("{not json")), &list, "x");
        assert!(!is_unread(&list[0], &seen, "x"));
    }

    #[test]
    fn the_seen_map_forgets_the_least_recent_past_the_cap() {
        let mut seen = SeenMap::new();
        for i in 0..(SEEN_CAP as u64 + 3) {
            mark_seen(&mut seen, &format!("s{i}"), 1000 + i);
        }
        let seen = reconcile_seen(Some(seen), &[], "x");
        assert_eq!(seen.len(), SEEN_CAP);
        assert!(seen.get("s0").is_none());
        assert!(seen.get(&format!("s{}", SEEN_CAP + 2)).is_some());
    }
}
