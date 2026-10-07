/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! `/pair-agent`: pair an AI agent (such as Muse) with this machine (design §4.1).
//!
//! Text, like `/pair-device`, and for the same reason: it is used once in a
//! long while by somebody already typing. The block goes in a code block so a
//! copy carries exactly its four lines — the agent reads them by prefix — and
//! never into a link, because the code in it derives the channel key.

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
pub fn compose_agent_pairing_message(block: &str) -> String {
    format!(
        "✅ An AI agent can now be paired with this machine.

         Copy this whole block and paste it into the agent (for example, tell Muse          \"install the NevoFlux client and pair with this\"):

         ```
{block}```

         The agent will then ask you to approve it at **nevoflux.app/device** with          your NevoFlux account.

         Once connected it can use this browser within the current execution tier,          and it answers its own approval prompts — pair only an agent you trust.          The code is shown once. See it later with `/devices`; take it back with          `/unpair <id>`."
    )
}

/// What the sidebar says when pairing did not happen.
pub fn compose_agent_pairing_failure(reason: &str) -> String {
    format!(
        "Could not pair an agent: {reason}

         If this says the command is unknown, the NevoFlux agent on this machine          needs an update."
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
    fn the_message_says_what_handing_it_over_means() {
        let text = compose_agent_pairing_message(BLOCK);
        assert!(text.contains("nevoflux.app/device"), "points at the approval step");
        assert!(text.contains("/devices"), "says how to see it later");
        assert!(text.contains("/unpair"), "says how to take it back");
        assert!(text.to_lowercase().contains("execution tier"), "states the limit");
    }

    #[test]
    fn a_failure_names_the_reason() {
        let text = compose_agent_pairing_failure("unknown command: remote.pair_agent");
        assert!(text.contains("unknown command: remote.pair_agent"));
        assert!(text.to_lowercase().contains("update"), "suggests the likely fix");
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
