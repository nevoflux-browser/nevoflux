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

/// What one stream frame does in the sidebar (M3 §6.1, §6.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StreamRoute {
    /// Render it in the conversation on screen.
    pub render: bool,
    /// Ask for the session list again: a session finished off screen, or a
    /// delegated turn finished on screen (ruling B-M4), so the list shows
    /// how it ended.
    pub refresh_list: bool,
}

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

    /// Route a stream frame of `session` while `current` is on screen.
    /// `delegated` says a paired agent's task runs in `session`.
    pub fn route_stream(
        &mut self,
        session: &str,
        current: &str,
        done: bool,
        delegated: bool,
    ) -> StreamRoute {
        if self.in_view(session, current) {
            if done {
                self.note_done(session);
            }
            StreamRoute { render: true, refresh_list: done && delegated }
        } else {
            StreamRoute { render: false, refresh_list: self.note_elsewhere(session, done) }
        }
    }

    /// A plan arrived for `plan.session_id` while `current` is on screen.
    /// Returns true when its card should be shown now. A delegated
    /// session's plan is answered on the status bar, so it is neither shown
    /// nor kept for later (contract R21); any other is kept until answered.
    pub fn offer_plan(&mut self, plan: PlanProposalPayload, current: &str, delegated: bool) -> bool {
        if delegated {
            return false;
        }
        let show = self.in_view(&plan.session_id, current);
        self.park_plan(plan);
        show
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
    fn a_delegated_sessions_plan_is_neither_shown_nor_kept() {
        // Contract R21: it is answered on the status bar, never as a card,
        // not even later when its conversation is opened.
        let mut r = SessionRouting::default();
        assert!(!r.offer_plan(plan("muse-task"), "muse-task", true));
        assert!(!r.offer_plan(plan("muse-task"), "s-a", true));
        assert_eq!(r.parked_plan("muse-task"), None);
        assert_eq!(r, SessionRouting::default());
    }

    #[test]
    fn a_plan_shows_on_screen_and_waits_off_screen() {
        let mut r = SessionRouting::default();
        assert!(r.offer_plan(plan("s-a"), "s-a", false), "on screen: show the card");
        assert!(r.parked_plan("s-a").is_some(), "kept until answered");
        assert!(!r.offer_plan(plan("s-b"), "s-a", false), "off screen: no card now");
        assert_eq!(r.parked_plan("s-b"), Some(plan("s-b")), "shown when s-b is opened");
        // An older daemon's plan without a session shows where the person is.
        assert!(r.offer_plan(plan(""), "s-a", false));
        assert_eq!(r.parked_plan(""), None);
    }

    #[test]
    fn a_stream_frame_renders_only_in_its_conversation() {
        let mut r = SessionRouting::default();
        let shown = r.route_stream("s-a", "s-a", false, false);
        assert_eq!(shown, StreamRoute { render: true, refresh_list: false });
        let elsewhere = r.route_stream("muse-task", "s-a", false, true);
        assert_eq!(elsewhere, StreamRoute { render: false, refresh_list: false });
        assert!(r.is_running("muse-task"));
        let finished = r.route_stream("muse-task", "s-a", true, true);
        assert_eq!(finished, StreamRoute { render: false, refresh_list: true });
        assert!(!r.is_running("muse-task"));
        // A frame without a session and an adopted canvas chat render here.
        assert!(r.route_stream("", "s-a", false, false).render);
        r.adopt("cs_1");
        assert!(r.route_stream("cs_1", "s-a", false, false).render);
    }

    #[test]
    fn a_watched_delegated_turn_ending_refreshes_the_list() {
        // Ruling B-M4: the list learns the task ended while it was on screen,
        // so it does not later show as finished and unread.
        let mut r = SessionRouting::default();
        r.route_stream("muse-task", "s-a", false, true);
        let done = r.route_stream("muse-task", "muse-task", true, true);
        assert_eq!(done, StreamRoute { render: true, refresh_list: true });
        assert!(!r.is_running("muse-task"));
        // The person's own turn ending on screen does not refresh it.
        let own = r.route_stream("s-a", "s-a", true, false);
        assert_eq!(own, StreamRoute { render: true, refresh_list: false });
    }

    #[test]
    fn a_plan_without_a_session_is_not_kept() {
        let mut r = SessionRouting::default();
        r.park_plan(plan(""));
        assert_eq!(r, SessionRouting::default());
    }
}
