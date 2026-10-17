/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Which conversation a frame belongs to (M3 §6.1).
//!
//! The daemon sends every session's output down the one connection: the
//! person's own chats, canvas chats, and tasks a paired agent delegated. The
//! sidebar shows one conversation at a time, so each session-scoped frame is
//! rendered only when it belongs there; another session's frame just marks
//! that session as running in the history list. A plan proposed in a
//! conversation that is not on screen waits here until it is answered, so
//! opening that conversation shows it again. Pure, tested with
//! `cargo test -p chat-sidebar --target x86_64-pc-windows-msvc --lib`.

use std::collections::{BTreeMap, BTreeSet};

use shared_protocol::PlanProposalPayload;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SessionRouting {
    /// Canvas chats (`canvas_chat_inject`): the sidebar has always shown
    /// them in whichever conversation is on screen.
    adopted: BTreeSet<String>,
    /// Sessions with a turn running off screen.
    running: BTreeSet<String>,
    /// The unanswered plan each session waits on.
    plans: BTreeMap<String, PlanProposalPayload>,
}

impl SessionRouting {
    /// Whether a frame for `frame_session` renders in the conversation
    /// `current`. A frame without a session (an older daemon, a generic
    /// error) renders as it always did.
    pub fn in_view(&self, frame_session: &str, current: &str) -> bool {
        frame_session.is_empty() || frame_session == current || self.adopted.contains(frame_session)
    }

    /// Show this canvas chat's frames wherever the sidebar is.
    pub fn adopt(&mut self, session: &str) {
        if !session.is_empty() {
            self.adopted.insert(session.to_string());
        }
    }

    /// An off-screen session sent a stream frame. Returns true when it
    /// finished (the list should be refreshed to show how it ended).
    pub fn note_elsewhere(&mut self, session: &str, done: bool) -> bool {
        if session.is_empty() {
            return false;
        }
        if done {
            self.running.remove(session);
            true
        } else {
            self.running.insert(session.to_string());
            false
        }
    }

    /// The session on screen finished a turn.
    pub fn note_done(&mut self, session: &str) {
        self.running.remove(session);
    }

    /// Whether a turn is running in `session` off screen.
    pub fn is_running(&self, session: &str) -> bool {
        self.running.contains(session)
    }

    /// Keep a plan until it is answered. One without a session cannot be
    /// matched to a conversation later, so it is not kept.
    pub fn park_plan(&mut self, plan: PlanProposalPayload) {
        if !plan.session_id.is_empty() {
            self.plans.insert(plan.session_id.clone(), plan);
        }
    }

    /// The plan `session` is waiting on, if any.
    pub fn parked_plan(&self, session: &str) -> Option<PlanProposalPayload> {
        self.plans.get(session).cloned()
    }

    /// The plan was answered (here or elsewhere).
    pub fn drop_plan(&mut self, session: &str) {
        self.plans.remove(session);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_protocol::PlanStep;

    fn plan(session: &str) -> PlanProposalPayload {
        PlanProposalPayload {
            summary: "Open the page".into(),
            steps: vec![PlanStep { description: "navigate".into(), model: None }],
            session_id: session.into(),
        }
    }

    #[test]
    fn the_open_conversations_frames_render() {
        let r = SessionRouting::default();
        assert!(r.in_view("s-a", "s-a"));
    }

    #[test]
    fn another_sessions_stream_title_plan_and_artifact_frames_stay_out() {
        // stream_chunk (text, thinking, tool events, session_title),
        // plan_proposal and artifact_* all route through in_view.
        let r = SessionRouting::default();
        assert!(!r.in_view("muse-task", "s-a"));
        assert!(!r.in_view("s-b", "s-a"));
    }

    #[test]
    fn a_frame_without_a_session_renders_as_before() {
        let r = SessionRouting::default();
        assert!(r.in_view("", "s-a"));
    }

    #[test]
    fn canvas_chats_render_wherever_they_were_injected() {
        let mut r = SessionRouting::default();
        r.adopt("cs_1");
        r.adopt("");
        assert!(r.in_view("cs_1", "s-a"));
        assert!(r.in_view("cs_1", "s-b"));
        assert!(!r.in_view("cs_2", "s-a"));
    }

    #[test]
    fn a_session_running_off_screen_is_marked_until_it_finishes() {
        let mut r = SessionRouting::default();
        assert!(!r.note_elsewhere("muse-task", false));
        assert!(r.is_running("muse-task"));
        assert!(r.note_elsewhere("muse-task", true), "finishing asks for a list refresh");
        assert!(!r.is_running("muse-task"));
        assert!(!r.note_elsewhere("", false));
        // Opened mid-run, it finishes on screen.
        r.note_elsewhere("s-b", false);
        r.note_done("s-b");
        assert!(!r.is_running("s-b"));
    }

    #[test]
    fn a_plan_waits_in_its_session_until_answered() {
        let mut r = SessionRouting::default();
        r.park_plan(plan("s-b"));
        assert_eq!(r.parked_plan("s-b"), Some(plan("s-b")));
        assert_eq!(r.parked_plan("s-a"), None);
        // Opening the conversation twice shows it twice: it stays until answered.
        assert!(r.parked_plan("s-b").is_some());
        r.drop_plan("s-b");
        assert_eq!(r.parked_plan("s-b"), None);
    }

    #[test]
    fn a_plan_without_a_session_is_not_kept() {
        let mut r = SessionRouting::default();
        r.park_plan(plan(""));
        assert_eq!(r, SessionRouting::default());
    }
}
