/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! `/devices` and `/unpair`: what can reach this machine, and how to stop it.
//!
//! The daemon has had `remote.pairings` and `remote.unpair` since pairings
//! became durable, and nothing called either. That left revoking a device as a
//! thing the design provides and the product does not, which matters more here
//! than for most missing buttons: a pairing survives restarts by design, so a
//! phone that is lost, sold, or simply mistyped goes on being able to reach
//! this machine until somebody edits `pairings.json` by hand.
//!
//! Text rather than a panel, for the same reason `/pair-device` is text: these
//! commands are used once in a long while, by somebody who is already typing.

use crate::state::Message;
use dioxus::prelude::*;

/// How many characters of a channel id are enough to name one device.
///
/// Eight. They are UUIDs, so eight is far past the point where two collide, and
/// short enough to be typed off the line above without care.
const HANDLE_LEN: usize = 8;

/// One row as the daemon sends it, narrowed to what is shown.
///
/// Deliberately not the pairing: `remote.pairings` carries no key and no code,
/// because answering "what can reach this machine" needs neither.
#[derive(Debug)]
pub struct Device {
    pub handle: String,
    pub label: Option<String>,
    pub created_at: i64,
    pub can_be_woken: bool,
    /// An AI agent rather than a phone. Agents are reached over their own MCP
    /// channel and are never woken, so the wake state is not shown for them.
    pub is_agent: bool,
    /// The full channel id. Only the handle is ever shown; this is what goes
    /// back to the daemon (`/agent-tier`).
    pub channel_id: String,
    /// An agent's execution tier as the daemon reports it (M2a §2.6). `None`
    /// for phones, and for agents on a daemon too old to report one — shown
    /// as nothing rather than as a guess.
    pub tier: Option<String>,
    /// An agent's live state from the sticky status snapshot (M2b §3.4):
    /// `connected`, `paused` or `offline`. `None` for phones, and when the
    /// sidebar has no snapshot yet — shown as nothing, not as offline.
    pub presence: Option<&'static str>,
}

/// Read the daemon's rows into something the rest of this module can hold.
pub fn read_rows(rows: &[serde_json::Value]) -> Vec<Device> {
    rows.iter()
        .filter_map(|r| {
            let id = r.get("control_channel_id")?.as_str()?;
            Some(Device {
                handle: id.chars().take(HANDLE_LEN).collect(),
                label: r
                    .get("label")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
                created_at: r.get("created_at").and_then(|v| v.as_i64()).unwrap_or(0),
                can_be_woken: r
                    .get("can_be_woken")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                is_agent: r.get("kind").and_then(|v| v.as_str()) == Some("agent"),
                channel_id: id.to_string(),
                tier: r
                    .get("tier")
                    .and_then(|v| v.as_str())
                    .filter(|s| !s.is_empty())
                    .map(str::to_string),
                presence: None,
            })
        })
        .collect()
}

/// Fill in each agent's live state from the status snapshot, if there is one.
pub fn apply_presence(
    devices: &mut [Device],
    status: Option<&crate::state::PairedAgentsSnapshot>,
) {
    let Some(status) = status else {
        return;
    };
    for d in devices.iter_mut().filter(|d| d.is_agent) {
        d.presence = crate::state::presence_word(status, &d.channel_id);
    }
}

/// How long ago, in the roughest terms that are still true.
fn when(created_at: i64, now_secs: i64) -> String {
    let days = (now_secs - created_at).max(0) / 86_400;
    match days {
        0 => "today".to_string(),
        1 => "yesterday".to_string(),
        d => format!("{d} days ago"),
    }
}

/// The list, as it goes into the transcript.
///
/// Split from the command so the wording is testable without a daemon behind
/// it — and the handle has to survive into the text, because it is the only
/// thing `/unpair` can be given.
pub fn compose_list(devices: &[Device], now_secs: i64) -> String {
    if devices.is_empty() {
        return "Nothing is paired with this machine.\n\n\
                Run `/pair-device` to add a phone, or `/pair-agent` to add an AI agent."
            .to_string();
    }
    let mut out = String::from("Paired with this machine:\n\n");
    for d in devices {
        if d.is_agent {
            let name = d.label.clone().unwrap_or_else(|| "AI agent".into());
            let tier = d
                .tier
                .as_deref()
                .map(|t| format!(" · tier `{t}`"))
                .unwrap_or_default();
            let presence = d.presence.map(|p| format!(" · {p}")).unwrap_or_default();
            out.push_str(&format!(
                "- **{name}** · `{}` · paired {}{tier}{presence}\n",
                d.handle,
                when(d.created_at, now_secs)
            ));
            continue;
        }
        let name = d.label.clone().unwrap_or_else(|| "unnamed device".into());
        // The wake state is worth a word: a device that cannot be woken is one
        // whose notifications quietly stopped, which is the failure this whole
        // path is least likely to notice.
        let woken = if d.can_be_woken {
            "can be woken"
        } else {
            "cannot be woken"
        };
        out.push_str(&format!(
            "- **{name}** · `{}` · paired {} · {woken}\n",
            d.handle,
            when(d.created_at, now_secs)
        ));
    }
    out.push_str("\nTo revoke one: `/unpair <id>` — the short id above.");
    if devices.iter().any(|d| d.is_agent) {
        out.push_str("\n\nTo change what an agent may do: `/agent-tier <id> <tier>`.");
    }
    out
}

/// Which device a typed handle means.
///
/// A prefix rather than an exact id, because the id is a UUID and nobody is
/// going to type one. Ambiguity is refused rather than guessed: revoking is not
/// undoable, and the cost of picking wrong is somebody's phone going dark with
/// no way to tell why.
pub fn resolve<'a>(devices: &'a [Device], typed: &str) -> Result<&'a Device, String> {
    let want = typed.trim().trim_matches('`').to_lowercase();
    if want.is_empty() {
        return Err("Say which one: `/unpair <id>`, using a short id from `/devices`.".into());
    }
    let hits: Vec<&Device> = devices
        .iter()
        .filter(|d| d.handle.to_lowercase().starts_with(&want))
        .collect();
    match hits.len() {
        1 => Ok(hits[0]),
        0 => Err(format!(
            "Nothing paired here starts with `{want}`. Run `/devices` for the list."
        )),
        n => Err(format!(
            "`{want}` matches {n} devices. Type more of the id."
        )),
    }
}

/// What is said once a device is gone.
pub fn compose_revoked(d: &Device) -> String {
    if d.is_agent {
        return format!(
            "✅ **AI agent** `{}` is unpaired: its channel is closed and its key deleted. It \
can no longer reach this machine.",
            d.handle
        );
    }
    let name = d.label.clone().unwrap_or_else(|| "That device".into());
    format!(
        "✅ **{name}** (`{}`) can no longer reach this machine.\n\n\
         Its channels are closed and its push subscription is gone with it — an endpoint left \
         behind is a standing way to make somebody's phone buzz. Pairing it again means a new \
         code.",
        d.handle
    )
}

fn now_secs() -> i64 {
    (js_sys::Date::now() / 1000.0) as i64
}

/// Run `/devices`. `status` is the sidebar's latest agent status snapshot.
pub async fn list(
    mut messages: Signal<Vec<Message>>,
    status: Option<crate::state::PairedAgentsSnapshot>,
) {
    match crate::messaging::remote_pairings().await {
        Ok(rows) => {
            let mut devices = read_rows(&rows);
            apply_presence(&mut devices, status.as_ref());
            messages
                .write()
                .push(Message::assistant_markdown(compose_list(
                    &devices,
                    now_secs(),
                )));
        }
        Err(e) => {
            messages.write().push(Message::assistant_markdown(format!(
                "Could not read the paired devices: {e}"
            )));
        }
    }
}

/// Run `/unpair <id>`.
pub async fn unpair(mut messages: Signal<Vec<Message>>, typed: String) {
    // Resolved against a fresh list rather than a remembered one: the transcript
    // may be showing a listing from before something else changed.
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
    let target = match resolve(&devices, &typed) {
        Ok(d) => d,
        Err(why) => {
            messages.write().push(Message::assistant_markdown(why));
            return;
        }
    };
    let (handle, label) = (target.handle.clone(), target.label.clone());
    let full = rows
        .iter()
        .filter_map(|r| r.get("control_channel_id")?.as_str())
        .find(|id| id.starts_with(&handle))
        .unwrap_or_default()
        .to_string();

    match crate::messaging::remote_unpair(&full).await {
        Ok(true) => {
            let d = Device {
                handle,
                label,
                created_at: 0,
                can_be_woken: false,
                is_agent: target.is_agent,
                channel_id: full.clone(),
                tier: None,
                presence: None,
            };
            messages
                .write()
                .push(Message::assistant_markdown(compose_revoked(&d)));
        }
        Ok(false) => {
            messages.write().push(Message::assistant_markdown(format!(
                "`{handle}` was already gone — nothing to revoke."
            )));
        }
        Err(e) => {
            messages.write().push(Message::assistant_markdown(format!(
                "Could not revoke `{handle}`: {e}"
            )));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(handle: &str, label: Option<&str>) -> Device {
        Device {
            handle: handle.into(),
            label: label.map(str::to_string),
            created_at: 0,
            can_be_woken: false,
            is_agent: false,
            channel_id: format!("{handle}-0000-0000-0000-000000000000"),
            tier: None,
            presence: None,
        }
    }

    fn agents_status(entries: serde_json::Value) -> crate::state::PairedAgentsSnapshot {
        crate::state::parse_paired_agents(&serde_json::json!({ "agents": entries }))
            .expect("agents array")
    }

    #[test]
    fn the_listing_carries_the_handle_unpair_needs() {
        // The only route from the list to the command is the reader copying
        // this token, so it has to be in the text and it has to be the same one
        // `resolve` accepts.
        let devices = vec![dev("3c12b59a", Some("Pixel"))];
        let text = compose_list(&devices, 0);
        assert!(text.contains("3c12b59a"));
        assert!(text.contains("Pixel"));
        assert!(resolve(&devices, "3c12b59a").is_ok());
    }

    #[test]
    fn an_unnamed_device_is_still_addressable() {
        let devices = vec![dev("ba4f7304", None)];
        let text = compose_list(&devices, 0);
        assert!(text.contains("ba4f7304"));
        assert!(text.contains("unnamed"));
    }

    #[test]
    fn a_prefix_is_enough_but_an_ambiguous_one_is_refused() {
        // Refused rather than guessed: revoking is not undoable, and the cost of
        // picking the wrong row is a phone that goes dark for no stated reason.
        let devices = vec![dev("3c12b59a", None), dev("3c99ffff", None)];
        assert!(resolve(&devices, "3c12").is_ok());
        let err = resolve(&devices, "3c").unwrap_err();
        assert!(err.contains("matches 2"), "{err}");
    }

    #[test]
    fn a_handle_pasted_with_its_backticks_still_resolves() {
        // The listing prints it in code ticks, so that is how it gets copied.
        let devices = vec![dev("3c12b59a", None)];
        assert!(resolve(&devices, "`3c12b59a`").is_ok());
        assert!(resolve(&devices, "  3C12B59A  ").is_ok());
    }

    #[test]
    fn nothing_paired_says_what_to_do_instead() {
        let text = compose_list(&[], 0);
        assert!(text.contains("/pair-device"));
    }

    #[test]
    fn reading_rows_tolerates_a_daemon_that_omits_the_optional_fields() {
        let rows = vec![serde_json::json!({
            "control_channel_id": "1605d75c-9e53-455c-8f72-97c61d35b3b9"
        })];
        let devices = read_rows(&rows);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].handle, "1605d75c");
        assert!(devices[0].label.is_none());
        assert!(!devices[0].can_be_woken);
    }

    #[test]
    fn how_long_ago_reads_as_words() {
        assert_eq!(when(1_000_000, 1_000_000), "today");
        assert_eq!(when(1_000_000, 1_000_000 + 86_400), "yesterday");
        assert_eq!(when(1_000_000, 1_000_000 + 3 * 86_400), "3 days ago");
        // A clock that went backwards is not a negative age.
        assert_eq!(when(1_000_000, 0), "today");
    }

    #[test]
    fn revoking_says_the_subscription_went_with_it() {
        // The one consequence somebody might not expect, and the one that
        // matters: an endpoint left behind is a standing capability.
        let text = compose_revoked(&dev("3c12b59a", Some("Pixel")));
        assert!(text.contains("Pixel"));
        assert!(text.to_lowercase().contains("push subscription"));
    }

    #[test]
    fn revoking_an_agent_uses_agent_wording() {
        let mut a = dev("abcdef01", None);
        a.is_agent = true;
        let text = compose_revoked(&a);
        assert!(text.contains("**AI agent** `abcdef01`"), "{text}");
        assert!(text.contains("key deleted"));
        assert!(!text.to_lowercase().contains("push subscription"));
        assert!(!text.to_lowercase().contains("phone"));
        for l in text.split('\n') {
            assert!(!l.starts_with("    "), "{l:?}");
        }
    }

    #[test]
    fn rows_without_kind_read_as_devices() {
        let rows = vec![serde_json::json!({"control_channel_id": "abcdef0123", "created_at": 0})];
        assert!(!read_rows(&rows)[0].is_agent);
    }

    #[test]
    fn an_agent_row_says_agent_and_not_wake_state() {
        let rows = vec![serde_json::json!({
            "control_channel_id": "abcdef0123", "created_at": 0, "kind": "agent"
        })];
        let devices = read_rows(&rows);
        assert!(devices[0].is_agent);
        let text = compose_list(&devices, 0);
        assert!(text.contains("AI agent"));
        assert!(!text.contains("woken"), "wake state means nothing for an agent: {text}");
        assert!(text.contains("`abcdef01`"), "the handle still survives for /unpair");
    }

    #[test]
    fn the_empty_list_mentions_both_ways_to_pair() {
        let text = compose_list(&[], 0);
        assert!(text.contains("/pair-device"));
        assert!(text.contains("/pair-agent"));
    }

    #[test]
    fn an_agent_row_shows_its_tier_and_how_to_change_it() {
        let rows = vec![serde_json::json!({
            "control_channel_id": "abcdef0123", "created_at": 0, "kind": "agent",
            "tier": "browser-auto"
        })];
        let devices = read_rows(&rows);
        assert_eq!(devices[0].tier.as_deref(), Some("browser-auto"));
        assert_eq!(devices[0].channel_id, "abcdef0123", "the full id, for the daemon");
        let text = compose_list(&devices, 0);
        assert!(text.contains("tier `browser-auto`"), "{text}");
        assert!(text.contains("/agent-tier <id> <tier>"), "{text}");
        for l in text.split('\n') {
            assert!(!l.starts_with("    "), "{l:?}");
        }
    }

    #[test]
    fn an_agent_row_from_an_older_daemon_shows_no_tier_rather_than_a_guess() {
        let rows = vec![serde_json::json!({
            "control_channel_id": "abcdef0123", "created_at": 0, "kind": "agent"
        })];
        let devices = read_rows(&rows);
        assert!(devices[0].tier.is_none());
        assert!(!compose_list(&devices, 0).contains("tier `"));
    }

    #[test]
    fn an_agent_row_ends_with_its_live_state() {
        let rows = vec![
            serde_json::json!({"control_channel_id": "aaaa1111-0000", "created_at": 0,
                               "kind": "agent", "tier": "read-only"}),
            serde_json::json!({"control_channel_id": "bbbb2222-0000", "created_at": 0,
                               "kind": "agent"}),
            serde_json::json!({"control_channel_id": "cccc3333-0000", "created_at": 0,
                               "kind": "agent"}),
        ];
        let status = agents_status(serde_json::json!([
            {"channel_id": "aaaa1111-0000", "state": "connected", "since": 0},
            {"channel_id": "bbbb2222-0000", "state": "paused", "since": 0},
            {"channel_id": "cccc3333-0000", "state": "offline", "since": 0}
        ]));
        let mut devices = read_rows(&rows);
        apply_presence(&mut devices, Some(&status));
        let text = compose_list(&devices, 0);
        let lines: Vec<&str> = text.lines().filter(|l| l.starts_with("- ")).collect();
        assert!(lines[0].ends_with("tier `read-only` · connected"), "{text}");
        assert!(lines[1].ends_with("· paused"), "{text}");
        assert!(lines[2].ends_with("· offline"), "{text}");
        for l in text.split('\n') {
            assert!(!l.starts_with("    "), "{l:?}");
        }
    }

    #[test]
    fn no_snapshot_means_no_presence_rather_than_offline() {
        let rows = vec![serde_json::json!({"control_channel_id": "aaaa1111-0000",
                                           "created_at": 0, "kind": "agent"})];
        let mut devices = read_rows(&rows);
        apply_presence(&mut devices, None);
        let text = compose_list(&devices, 0);
        for w in ["connected", "paused", "offline"] {
            assert!(!text.contains(&format!("· {w}")), "{text}");
        }
    }

    #[test]
    fn phones_never_get_a_presence() {
        let rows = vec![serde_json::json!({"control_channel_id": "dddd4444-0000",
                                           "created_at": 0, "kind": "device", "label": "Pixel"})];
        let status = agents_status(serde_json::json!([
            {"channel_id": "dddd4444-0000", "state": "connected", "since": 0}
        ]));
        let mut devices = read_rows(&rows);
        apply_presence(&mut devices, Some(&status));
        assert!(devices[0].presence.is_none());
        assert!(compose_list(&devices, 0).contains("· cannot be woken\n"));
    }

    #[test]
    fn a_list_of_phones_only_does_not_mention_agent_tiers() {
        let text = compose_list(&[dev("3c12b59a", Some("Pixel"))], 0);
        assert!(!text.contains("/agent-tier"), "{text}");
        assert!(!text.contains("tier `"), "{text}");
    }
}
