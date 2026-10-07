/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! `/pair-agent`: pair an AI agent (such as Muse) with this machine (design §4.1).
//!
//! Text, like `/pair-device`, and for the same reason: it is used once in a
//! long while by somebody already typing. The block goes in a code block so a
//! copy carries exactly its four lines — the agent reads them by prefix — and
//! never into a link, because the code in it derives the channel key.

use crate::messaging::signin::needs_sign_in;
use crate::state::Message;
use dioxus::prelude::*;

/// The block out of a `remote.pair_agent` answer. An empty one is a daemon
/// that did not do what was asked, and showing an empty code block would read
/// as success.
pub fn block_from(data: &serde_json::Value) -> Result<String, String> {
    data.get("block")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .map(str::to_string)
        .ok_or_else(|| "the daemon answered without a pairing block".to_string())
}

/// What the sidebar says once the agent pairing exists.
///
/// Built from one string per paragraph joined with explicit `\n`, so every line
/// starts at column 0: the sidebar renders CommonMark, where a fence indented
/// four columns is an indented code block, not a fence.
pub fn compose_agent_pairing_message(block: &str) -> String {
    let intro = "✅ An AI agent can now be paired with this machine.";
    let copy = "Copy this whole block and paste it into the agent (for example, tell Muse \
\"install the NevoFlux client and pair with this\"):";
    let approve = "Approving at **nevoflux.app/device** signs the agent in to your NevoFlux \
account.";
    let limit = "Whoever runs the agent gets the same reach it does: once browser tools are \
enabled for agents (in a later update), it will act in this browser within the current \
execution tier and answer its own approval prompts. Pair only an agent you trust.";
    let after = "The code is shown once — copy the block now. `/devices` lists this pairing; \
`/unpair <id>` takes it back.";
    format!("{intro}\n\n{copy}\n\n```\n{block}```\n\n{approve}\n\n{limit} {after}")
}

/// What the sidebar says when pairing did not happen.
pub fn compose_agent_pairing_failure(reason: &str) -> String {
    if needs_sign_in(reason) {
        return "Could not pair an agent: this machine is not signed in to NevoFlux (the sign-in may have expired).\n\nRun `/remote-control` to sign in, then run `/pair-agent` again.".to_string();
    }
    format!(
        "Could not pair an agent: {reason}\n\n\
If this says the command is unknown, the NevoFlux agent on this machine needs an update."
    )
}

/// Run the pairing flow and report it into the transcript.
pub async fn run(mut messages: Signal<Vec<Message>>) {
    let text = match crate::messaging::remote_pair_agent().await {
        Ok(block) => compose_agent_pairing_message(&block),
        Err(e) => compose_agent_pairing_failure(&e),
    };
    messages.write().push(Message::assistant_markdown(text));
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLOCK: &str = "NEVOFLUX_AGENT_PAIRING\nrelay: wss://relay.nevoflux.app\nchannel: 2f1c4a90-7b3e-4d1a-9c58-0e6a2b7d4f31\ncode: A-BCDE-FGHJ-KMNP\n";

    #[test]
    fn the_block_is_shown_whole_inside_a_code_block() {
        let text = compose_agent_pairing_message(BLOCK);
        let start = text.find("```\n").expect("opens a code block") + 4;
        let end = text[start..].find("```").expect("closes it") + start;
        assert_eq!(&text[start..end], BLOCK, "copied verbatim, nothing added inside");
    }

    #[test]
    fn the_fence_is_a_real_fence_and_nothing_is_indented_into_code() {
        // In CommonMark a fence indented 4+ columns is an indented code block,
        // and stray indentation anywhere turns prose into code.
        let text = compose_agent_pairing_message(BLOCK);
        let lines: Vec<&str> = text.split('\n').collect();
        let fences: Vec<usize> = lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.trim() == "```")
            .map(|(i, _)| i)
            .collect();
        assert_eq!(fences.len(), 2, "exactly one fenced block: {text}");
        assert_eq!(lines[fences[0]], "```", "opening fence at column 0");
        assert_eq!(lines[fences[1]], "```", "closing fence at column 0");
        for (i, l) in lines.iter().enumerate() {
            if i > fences[0] && i < fences[1] {
                continue;
            }
            assert!(
                !l.starts_with("    ") && !l.starts_with('\t'),
                "line {i} is indented: {l:?}"
            );
        }
        let failure = compose_agent_pairing_failure("x");
        for l in failure.split('\n') {
            assert!(!l.starts_with("    ") && !l.starts_with('\t'), "{l:?}");
        }
    }

    #[test]
    fn the_message_says_what_handing_it_over_means() {
        let text = compose_agent_pairing_message(BLOCK);
        assert!(text.contains("nevoflux.app/device"), "points at the approval step");
        assert!(text.contains("/devices"), "says where the pairing is listed");
        assert!(text.contains("/unpair"), "says how to take it back");
        let lower = text.to_lowercase();
        assert!(lower.contains("execution tier"), "states the limit");
        assert!(lower.contains("approval prompts"), "says it answers its own prompts");
        assert!(lower.contains("trust"), "asks for trust");
        assert!(lower.contains("whoever runs the agent"), "names the operator");
        assert!(!lower.contains("see it later"), "the code cannot be seen later");
        assert!(lower.contains("copy the block now"), "tells them to copy now");
    }

    #[test]
    fn a_failure_names_the_reason() {
        let text = compose_agent_pairing_failure("unknown command: remote.pair_agent");
        assert!(text.contains("unknown command: remote.pair_agent"));
        assert!(text.to_lowercase().contains("update"), "suggests the likely fix");
    }

    #[test]
    fn a_missing_sign_in_says_to_sign_in_again() {
        for reason in [
            "log in first",
            "Invalid request: no JWT in set-auth-jwt header or token body",
        ] {
            let text = compose_agent_pairing_failure(reason);
            assert!(text.contains("/remote-control"), "{text}");
            assert!(text.contains("/pair-agent"), "{text}");
            assert!(text.contains("not signed in"), "{text}");
            assert!(!text.to_lowercase().contains("needs an update"), "{text}");
            for l in text.split('\n') {
                assert!(!l.starts_with("    "), "{l:?}");
            }
        }
    }

    #[test]
    fn an_empty_block_is_a_failure_not_a_success() {
        assert!(block_from(&serde_json::json!({"block": ""})).is_err());
        assert!(block_from(&serde_json::json!({})).is_err());
        assert_eq!(
            block_from(&serde_json::json!({"block": BLOCK})).unwrap(),
            BLOCK
        );
    }
}
