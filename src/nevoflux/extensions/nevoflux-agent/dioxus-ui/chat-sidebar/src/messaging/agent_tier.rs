/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! `/agent-tier <id> <tier>`: how much one paired AI agent may do (M2a §2.6).
//!
//! Each agent pairing has its own tier, separate from the global "Agent
//! execution" setting, so raising one never widens the other, and a new
//! pairing starts at `read-only`. The tier is a ceiling: a call above it is
//! refused, never asked about. Text, like `/devices` and `/unpair`, and the
//! agent is named the same way — a short id from `/devices`.

use crate::messaging::devices::{read_rows, Device};
use crate::state::Message;
use dioxus::prelude::*;

/// The tiers, least to most. Exactly the daemon's `ExecutionTier` strings.
pub const TIERS: [&str; 4] = [
    "read-only",
    "browser-auto",
    "browser-auto-local-read",
    "full-auto",
];

fn tier_list() -> String {
    TIERS
        .iter()
        .map(|t| format!("`{t}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

fn usage() -> String {
    [
        "Say which agent and which tier: `/agent-tier <id> <tier>`, using the 8-character id shown by `/devices`.".to_string(),
        format!("Tiers: {}.", tier_list()),
    ]
    .join("\n\n")
}

/// The tier a typed word means, if any. Case and code ticks are forgiven;
/// spelling is not, because a near miss must not reach the daemon.
pub fn normalize_tier(typed: &str) -> Option<&'static str> {
    let want = typed.trim().trim_matches('`').to_lowercase();
    TIERS.iter().copied().find(|t| *t == want)
}

/// What follows `/agent-tier`, split into the typed id and the tier.
pub fn parse_args(rest: &str) -> Result<(String, &'static str), String> {
    let parts: Vec<&str> = rest.split_whitespace().collect();
    if parts.len() != 2 {
        return Err(usage());
    }
    let tier = normalize_tier(parts[1]).ok_or_else(|| {
        format!(
            "`{}` is not a tier. Use one of: {}.",
            parts[1].trim_matches('`'),
            tier_list()
        )
    })?;
    Ok((parts[0].to_string(), tier))
}

/// Which agent a typed handle means. Same prefix matching as `/unpair`, but
/// only agents count: a prefix a phone happens to share picks the agent, and
/// a phone's own id is refused by name rather than as "not found". An empty
/// id is refused outright — as a prefix it would match the only agent paired
/// and change its tier without anyone having named it.
pub fn resolve_agent<'a>(devices: &'a [Device], typed: &str) -> Result<&'a Device, String> {
    let want = typed.trim().trim_matches('`').trim().to_lowercase();
    if want.is_empty() {
        return Err(usage());
    }
    let hits: Vec<&Device> = devices
        .iter()
        .filter(|d| d.handle.to_lowercase().starts_with(&want))
        .collect();
    let agents: Vec<&Device> = hits.iter().copied().filter(|d| d.is_agent).collect();
    match agents.len() {
        1 => Ok(agents[0]),
        0 if !hits.is_empty() => Err(format!(
            "`{want}` is a paired phone, not an AI agent. Tiers apply to agents only."
        )),
        0 => Err(format!(
            "No AI agent paired here starts with `{want}`. Run `/devices` for the list."
        )),
        n => Err(format!("`{want}` matches {n} agents. Type more of the id.")),
    }
}

/// The tier out of a `remote.agent_set_tier` answer. Anything but one of the
/// four is a daemon that did not do what was asked.
pub fn tier_from(data: &serde_json::Value) -> Result<String, String> {
    data.get("tier")
        .and_then(|v| v.as_str())
        .and_then(normalize_tier)
        .map(str::to_string)
        .ok_or_else(|| "the daemon did not confirm the new tier".to_string())
}

/// What a tier lets an agent do with its tools, in one sentence.
pub fn what_it_allows(tier: &str) -> &'static str {
    match tier {
        "read-only" => {
            "it can open any web page and read any of your open tabs, including ones where you are signed in (mail, banking), and it can see which tabs you have open. It cannot click or type: those calls are refused."
        }
        "browser-auto" => {
            "it can also click, type, fill, scroll, press keys and switch tabs on any page, including signed-in ones. That means anything you could do there: submitting forms, sending messages, buying things, deleting data."
        }
        "browser-auto-local-read" => {
            "the same as `browser-auto` for an agent. This tier exists for the local sidebar and adds nothing over the agent channel."
        }
        "full-auto" => {
            "it can also upload files from this computer to a page. This is the only tier where the agent can send your local files to a website, and the agent chooses which file goes where."
        }
        _ => "",
    }
}

/// What the sidebar says once the daemon holds the new tier.
pub fn compose_tier_set(handle: &str, tier: &str) -> String {
    [
        format!(
            "✅ **AI agent** `{handle}` is now `{tier}`: {}",
            what_it_allows(tier)
        ),
        "This applies from its next call. Calls above the tier are refused, not asked about."
            .to_string(),
    ]
    .join("\n\n")
}

/// What the sidebar says when the tier did not change. An old daemon answers
/// `Unknown command: ...`; that is told apart from the daemon refusing.
pub fn compose_tier_failure(reason: &str) -> String {
    let head = format!("Could not change the tier: {reason}");
    if reason.to_lowercase().contains("unknown command") {
        return [
            head,
            "The NevoFlux agent on this machine needs an update for `/agent-tier`.".to_string(),
        ]
        .join("\n\n");
    }
    head
}

/// Run `/agent-tier <rest>`.
pub async fn run(mut messages: Signal<Vec<Message>>, rest: String) {
    let (typed, tier) = match parse_args(&rest) {
        Ok(v) => v,
        Err(why) => {
            messages.write().push(Message::assistant_markdown(why));
            return;
        }
    };
    // Resolved against a fresh list, as `/unpair` does: the transcript may be
    // showing a listing from before something changed.
    let rows = match crate::messaging::remote_pairings().await {
        Ok(rows) => rows,
        Err(e) => {
            messages.write().push(Message::assistant_markdown(format!(
                "Could not read the paired devices: {e}"
            )));
            return;
        }
    };
    let devices = read_rows(&rows);
    let target = match resolve_agent(&devices, &typed) {
        Ok(d) => d,
        Err(why) => {
            messages.write().push(Message::assistant_markdown(why));
            return;
        }
    };
    let text = match crate::messaging::remote_agent_set_tier(&target.channel_id, tier).await {
        Ok(now) => compose_tier_set(&target.handle, &now),
        Err(e) => compose_tier_failure(&e),
    };
    messages.write().push(Message::assistant_markdown(text));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messaging::devices::read_rows;

    fn rows() -> Vec<serde_json::Value> {
        vec![
            serde_json::json!({"control_channel_id": "abcd1111-aaaa-bbbb-cccc-000000000001",
                               "created_at": 0, "kind": "agent", "tier": "read-only"}),
            serde_json::json!({"control_channel_id": "abcd2222-aaaa-bbbb-cccc-000000000002",
                               "created_at": 0, "kind": "device", "label": "Pixel"}),
            serde_json::json!({"control_channel_id": "ef013333-aaaa-bbbb-cccc-000000000003",
                               "created_at": 0, "kind": "agent"}),
            serde_json::json!({"control_channel_id": "ef014444-aaaa-bbbb-cccc-000000000004",
                               "created_at": 0, "kind": "agent"}),
        ]
    }

    fn not_indented(text: &str) {
        for l in text.split('\n') {
            assert!(!l.starts_with("    ") && !l.starts_with('\t'), "{l:?} in {text}");
        }
    }

    #[test]
    fn the_tiers_are_exactly_the_daemon_strings() {
        assert_eq!(
            TIERS,
            ["read-only", "browser-auto", "browser-auto-local-read", "full-auto"]
        );
    }

    #[test]
    fn an_id_and_a_tier_parse() {
        assert_eq!(
            parse_args(" abcd1111  browser-auto ").unwrap(),
            ("abcd1111".to_string(), "browser-auto")
        );
        assert_eq!(parse_args("abcd `FULL-AUTO`").unwrap().1, "full-auto");
    }

    #[test]
    fn a_typo_is_refused_and_the_tiers_are_listed() {
        let err = parse_args("abcd1111 browser_auto").unwrap_err();
        assert!(err.contains("`browser_auto` is not a tier"), "{err}");
        for t in TIERS {
            assert!(err.contains(&format!("`{t}`")), "{err}");
        }
        not_indented(&err);
    }

    #[test]
    fn missing_or_extra_words_get_the_usage() {
        for rest in ["", "abcd1111", "abcd1111 read-only now"] {
            let err = parse_args(rest).unwrap_err();
            assert!(err.contains("/agent-tier <id> <tier>"), "{err}");
            assert!(err.contains("/devices"), "{err}");
            assert!(err.contains("8-character id"), "{err}");
            not_indented(&err);
        }
    }

    #[test]
    fn a_prefix_shared_with_a_phone_picks_the_agent() {
        let devices = read_rows(&rows());
        let d = resolve_agent(&devices, "abcd").unwrap();
        assert!(d.is_agent);
        assert_eq!(d.channel_id, "abcd1111-aaaa-bbbb-cccc-000000000001");
    }

    #[test]
    fn a_phone_is_not_given_a_tier() {
        let devices = read_rows(&rows());
        let err = resolve_agent(&devices, "abcd2222").unwrap_err();
        assert!(err.contains("phone"), "{err}");
        assert!(err.contains("agents only"), "{err}");
    }

    #[test]
    fn ambiguous_and_unknown_agents_are_refused() {
        let devices = read_rows(&rows());
        let err = resolve_agent(&devices, "ef01").unwrap_err();
        assert!(err.contains("matches 2 agents"), "{err}");
        let err = resolve_agent(&devices, "`9999`").unwrap_err();
        assert!(err.contains("starts with `9999`"), "{err}");
        assert!(err.contains("/devices"), "{err}");
        assert!(resolve_agent(&devices, " `EF013333` ").is_ok());
    }

    #[test]
    fn an_empty_id_never_picks_the_only_agent() {
        // With one agent paired, an empty prefix would match it and silently
        // change its tier.
        let devices = read_rows(&[serde_json::json!({
            "control_channel_id": "abcd1111-aaaa-bbbb-cccc-000000000001",
            "created_at": 0, "kind": "agent"
        })]);
        for typed in ["", "  ", "``", " ` ` "] {
            let err = resolve_agent(&devices, typed).unwrap_err();
            assert!(err.contains("/agent-tier <id> <tier>"), "{err}");
            assert!(err.contains("/devices"), "{err}");
            not_indented(&err);
        }
    }

    #[test]
    fn the_daemon_must_confirm_a_real_tier() {
        assert_eq!(
            tier_from(&serde_json::json!({"tier": "full-auto"})).unwrap(),
            "full-auto"
        );
        assert!(tier_from(&serde_json::json!({})).is_err());
        assert!(tier_from(&serde_json::json!({"tier": "auto"})).is_err());
    }

    #[test]
    fn the_confirmation_names_the_agent_the_tier_and_what_it_allows() {
        for t in TIERS {
            let text = compose_tier_set("abcd1111", t);
            assert!(text.contains("`abcd1111`"), "{text}");
            assert!(text.contains(&format!("`{t}`")), "{text}");
            assert!(!what_it_allows(t).is_empty(), "{t}");
            assert!(text.contains(what_it_allows(t)), "{text}");
            assert!(text.contains("next call"), "{text}");
            not_indented(&text);
        }
        assert!(what_it_allows("read-only").contains("signed in"));
        assert!(what_it_allows("read-only").contains("cannot click or type"));
        assert!(what_it_allows("browser-auto").contains("submitting forms"));
        assert!(what_it_allows("browser-auto").contains("signed-in"));
        assert!(what_it_allows("browser-auto-local-read").contains("same as `browser-auto`"));
        assert!(what_it_allows("full-auto").contains("only tier"));
        assert!(what_it_allows("full-auto").contains("chooses which file"));
        assert!(what_it_allows("full-auto").contains("upload"));
        for t in TIERS {
            not_indented(what_it_allows(t));
        }
    }

    #[test]
    fn an_old_daemon_is_told_apart_from_a_refusal() {
        let old = compose_tier_failure("Unknown command: remote.agent_set_tier");
        assert!(old.contains("Unknown command: remote.agent_set_tier"), "{old}");
        assert!(old.to_lowercase().contains("needs an update"), "{old}");
        not_indented(&old);
        let other = compose_tier_failure("no paired agent with that id");
        assert!(other.contains("no paired agent with that id"), "{other}");
        assert!(!other.to_lowercase().contains("update"), "{other}");
        not_indented(&other);
    }
}
