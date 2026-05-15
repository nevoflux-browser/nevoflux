//! Permission checking for EventBus publish/subscribe operations.
//!
//! Enforces topic-prefix-based permission rules that control which
//! identities may publish to or subscribe to specific topic prefixes.

use super::types::{PublisherIdentity, SubscriberIdentity, TopicPattern};

/// Result of a permission check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionResult {
    /// The operation is allowed.
    Allowed,
    /// The operation is denied, with a human-readable reason.
    Denied(String),
}

impl PermissionResult {
    /// Returns `true` if the operation is allowed.
    pub fn is_allowed(&self) -> bool {
        matches!(self, PermissionResult::Allowed)
    }

    /// Returns `true` if the operation is denied.
    pub fn is_denied(&self) -> bool {
        matches!(self, PermissionResult::Denied(_))
    }
}

/// Extracts the first colon-separated segment from a topic string.
fn topic_prefix(topic: &str) -> &str {
    topic.split(':').next().unwrap_or(topic)
}

/// Stateless permission checker for EventBus operations.
///
/// All methods are static; no instance state is needed.
pub struct PermissionChecker;

impl PermissionChecker {
    /// Check whether `publisher` may publish to `topic`.
    ///
    /// # Permission matrix (publish)
    ///
    /// | Prefix     | Allowed publishers       |
    /// |------------|--------------------------|
    /// | `task`     | Internal (Daemon) only   |
    /// | `agent`    | Agent + Internal         |
    /// | `ui`       | Extension + Internal     |
    /// | `system`   | Internal (Daemon) only   |
    /// | `mcp`      | Mcp + Internal           |
    /// | `wasm`     | Wasm + Internal          |
    /// | (unknown)  | Internal (Daemon) only   |
    pub fn check_publish(topic: &str, publisher: &PublisherIdentity) -> PermissionResult {
        // Internal (Daemon) can always publish.
        if matches!(publisher, PublisherIdentity::Internal) {
            return PermissionResult::Allowed;
        }

        let prefix = topic_prefix(topic);

        match prefix {
            "task" | "system" => PermissionResult::Denied(format!(
                "{} may not publish to '{prefix}:*' topics",
                publisher_label(publisher),
            )),
            "agent" => match publisher {
                PublisherIdentity::Agent { .. } => PermissionResult::Allowed,
                _ => PermissionResult::Denied(format!(
                    "{} may not publish to 'agent:*' topics",
                    publisher_label(publisher),
                )),
            },
            "ui" => match publisher {
                PublisherIdentity::Extension { .. } => PermissionResult::Allowed,
                _ => PermissionResult::Denied(format!(
                    "{} may not publish to 'ui:*' topics",
                    publisher_label(publisher),
                )),
            },
            "mcp" => match publisher {
                PublisherIdentity::Mcp { .. } => PermissionResult::Allowed,
                _ => PermissionResult::Denied(format!(
                    "{} may not publish to 'mcp:*' topics",
                    publisher_label(publisher),
                )),
            },
            "wasm" => match publisher {
                PublisherIdentity::Wasm { .. } => PermissionResult::Allowed,
                _ => PermissionResult::Denied(format!(
                    "{} may not publish to 'wasm:*' topics",
                    publisher_label(publisher),
                )),
            },
            _ => PermissionResult::Denied(format!(
                "{} may not publish to unknown prefix '{prefix}:*' topics",
                publisher_label(publisher),
            )),
        }
    }

    /// Check whether `subscriber` may subscribe to `pattern`.
    ///
    /// Uses prefix-based permission uniformly across Exact / Wildcard /
    /// DoubleWildcard: whoever can subscribe to `ui:foo` exactly may also
    /// subscribe to `ui:*` or `ui:**`. Bare-wildcard patterns without a
    /// leading prefix segment (e.g. `*` or `**`) remain Internal-only —
    /// they cross permission boundaries.
    ///
    /// # Permission matrix (subscribe, by leading prefix)
    ///
    /// | Prefix     | Allowed subscribers              |
    /// |------------|----------------------------------|
    /// | `task`     | Agent + Internal                 |
    /// | `agent`    | Agent + Internal                 |
    /// | `ui`       | Extension + Internal             |
    /// | `system`   | All                              |
    /// | `jobs`     | All                              |
    /// | `mcp`      | Agent + Internal                 |
    /// | `wasm`     | Agent + Internal                 |
    /// | (unknown)  | Internal only                    |
    /// | empty      | Internal only (bare `*`/`**`)    |
    pub fn check_subscribe(
        pattern: &TopicPattern,
        subscriber: &SubscriberIdentity,
    ) -> PermissionResult {
        // Internal (Daemon) can always subscribe.
        if matches!(subscriber, SubscriberIdentity::Internal) {
            return PermissionResult::Allowed;
        }

        // Extract the leading prefix segment (everything before the first ':').
        // For Wildcard/DoubleWildcard this is the first segment of the pattern;
        // for a bare wildcard like "*" / "**" the prefix is empty.
        let prefix = match pattern {
            TopicPattern::Exact(t) => topic_prefix(t),
            TopicPattern::Wildcard(pat) => {
                let first = pat.split(':').next().unwrap_or("");
                if first == "*" {
                    ""
                } else {
                    first
                }
            }
            TopicPattern::DoubleWildcard(p) => p.as_str(),
        };

        match prefix {
            "" => PermissionResult::Denied(format!(
                "{} may not subscribe to cross-prefix wildcard patterns",
                subscriber_label(subscriber),
            )),
            "system" | "jobs" => PermissionResult::Allowed,
            "task" | "agent" | "mcp" | "wasm" => match subscriber {
                SubscriberIdentity::Agent { .. } => PermissionResult::Allowed,
                _ => PermissionResult::Denied(format!(
                    "{} may not subscribe to '{prefix}:*' topics",
                    subscriber_label(subscriber),
                )),
            },
            "ui" => match subscriber {
                SubscriberIdentity::Extension { .. } => PermissionResult::Allowed,
                _ => PermissionResult::Denied(format!(
                    "{} may not subscribe to 'ui:*' topics",
                    subscriber_label(subscriber),
                )),
            },
            _ => PermissionResult::Denied(format!(
                "{} may not subscribe to unknown prefix '{prefix}:*' topics",
                subscriber_label(subscriber),
            )),
        }
    }
}

/// Human-readable label for a publisher identity.
fn publisher_label(publisher: &PublisherIdentity) -> &'static str {
    match publisher {
        PublisherIdentity::Internal => "Daemon",
        PublisherIdentity::Agent { .. } => "Agent",
        PublisherIdentity::Extension { .. } => "Extension",
        PublisherIdentity::Wasm { .. } => "Wasm",
        PublisherIdentity::Mcp { .. } => "Mcp",
    }
}

/// Human-readable label for a subscriber identity.
fn subscriber_label(subscriber: &SubscriberIdentity) -> &'static str {
    match subscriber {
        SubscriberIdentity::Internal => "Daemon",
        SubscriberIdentity::Agent { .. } => "Agent",
        SubscriberIdentity::Extension { .. } => "Extension",
        SubscriberIdentity::Wasm { .. } => "Wasm",
        SubscriberIdentity::Mcp { .. } => "Mcp",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Helpers ─────────────────────────────────────────────────────

    fn agent() -> PublisherIdentity {
        PublisherIdentity::Agent {
            session_id: "s1".into(),
        }
    }

    fn extension() -> PublisherIdentity {
        PublisherIdentity::Extension {
            proxy_id: "p1".into(),
        }
    }

    fn mcp() -> PublisherIdentity {
        PublisherIdentity::Mcp {
            server_id: "m1".into(),
        }
    }

    fn wasm() -> PublisherIdentity {
        PublisherIdentity::Wasm {
            plugin_id: "w1".into(),
        }
    }

    fn sub_agent() -> SubscriberIdentity {
        SubscriberIdentity::Agent {
            session_id: "s1".into(),
        }
    }

    fn sub_extension() -> SubscriberIdentity {
        SubscriberIdentity::Extension {
            proxy_id: "p1".into(),
        }
    }

    fn sub_mcp() -> SubscriberIdentity {
        SubscriberIdentity::Mcp {
            server_id: "m1".into(),
        }
    }

    fn sub_wasm() -> SubscriberIdentity {
        SubscriberIdentity::Wasm {
            plugin_id: "w1".into(),
        }
    }

    // ── Publish permission tests ────────────────────────────────────

    #[test]
    fn publish_internal_always_allowed() {
        let internal = PublisherIdentity::Internal;
        assert!(PermissionChecker::check_publish("task:status", &internal).is_allowed());
        assert!(PermissionChecker::check_publish("agent:heartbeat", &internal).is_allowed());
        assert!(PermissionChecker::check_publish("ui:render", &internal).is_allowed());
        assert!(PermissionChecker::check_publish("system:shutdown", &internal).is_allowed());
        assert!(PermissionChecker::check_publish("mcp:request", &internal).is_allowed());
        assert!(PermissionChecker::check_publish("wasm:event", &internal).is_allowed());
        assert!(PermissionChecker::check_publish("unknown:topic", &internal).is_allowed());
    }

    #[test]
    fn publish_task_denied_for_non_daemon() {
        assert!(PermissionChecker::check_publish("task:status", &agent()).is_denied());
        assert!(PermissionChecker::check_publish("task:created", &extension()).is_denied());
        assert!(PermissionChecker::check_publish("task:done", &mcp()).is_denied());
        assert!(PermissionChecker::check_publish("task:run", &wasm()).is_denied());
    }

    #[test]
    fn publish_agent_allowed_for_agent_and_daemon() {
        assert!(PermissionChecker::check_publish("agent:heartbeat", &agent()).is_allowed());
        assert!(
            PermissionChecker::check_publish("agent:heartbeat", &PublisherIdentity::Internal)
                .is_allowed()
        );
    }

    #[test]
    fn publish_agent_denied_for_others() {
        assert!(PermissionChecker::check_publish("agent:heartbeat", &extension()).is_denied());
        assert!(PermissionChecker::check_publish("agent:heartbeat", &mcp()).is_denied());
        assert!(PermissionChecker::check_publish("agent:heartbeat", &wasm()).is_denied());
    }

    #[test]
    fn publish_ui_allowed_for_extension_and_daemon() {
        assert!(PermissionChecker::check_publish("ui:render", &extension()).is_allowed());
        assert!(
            PermissionChecker::check_publish("ui:render", &PublisherIdentity::Internal)
                .is_allowed()
        );
    }

    #[test]
    fn publish_ui_denied_for_others() {
        assert!(PermissionChecker::check_publish("ui:render", &agent()).is_denied());
        assert!(PermissionChecker::check_publish("ui:render", &mcp()).is_denied());
        assert!(PermissionChecker::check_publish("ui:render", &wasm()).is_denied());
    }

    #[test]
    fn publish_system_denied_for_non_daemon() {
        assert!(PermissionChecker::check_publish("system:shutdown", &agent()).is_denied());
        assert!(PermissionChecker::check_publish("system:shutdown", &extension()).is_denied());
    }

    #[test]
    fn publish_mcp_allowed_for_mcp_and_daemon() {
        assert!(PermissionChecker::check_publish("mcp:request", &mcp()).is_allowed());
        assert!(
            PermissionChecker::check_publish("mcp:request", &PublisherIdentity::Internal)
                .is_allowed()
        );
    }

    #[test]
    fn publish_mcp_denied_for_others() {
        assert!(PermissionChecker::check_publish("mcp:request", &agent()).is_denied());
        assert!(PermissionChecker::check_publish("mcp:request", &extension()).is_denied());
    }

    #[test]
    fn publish_wasm_allowed_for_wasm_and_daemon() {
        assert!(PermissionChecker::check_publish("wasm:event", &wasm()).is_allowed());
        assert!(
            PermissionChecker::check_publish("wasm:event", &PublisherIdentity::Internal)
                .is_allowed()
        );
    }

    #[test]
    fn publish_wasm_denied_for_others() {
        assert!(PermissionChecker::check_publish("wasm:event", &agent()).is_denied());
        assert!(PermissionChecker::check_publish("wasm:event", &extension()).is_denied());
    }

    #[test]
    fn publish_unknown_prefix_denied_for_non_daemon() {
        assert!(PermissionChecker::check_publish("custom:topic", &agent()).is_denied());
        assert!(PermissionChecker::check_publish("custom:topic", &extension()).is_denied());
    }

    // ── Subscribe permission tests ──────────────────────────────────

    #[test]
    fn subscribe_internal_always_allowed() {
        let internal = SubscriberIdentity::Internal;
        let patterns = [
            TopicPattern::exact("task:status"),
            TopicPattern::exact("system:boot"),
            TopicPattern::wildcard("task:*"),
            TopicPattern::double_wildcard(""),
        ];
        for pat in &patterns {
            assert!(
                PermissionChecker::check_subscribe(pat, &internal).is_allowed(),
                "Internal should be allowed for pattern {:?}",
                pat
            );
        }
    }

    #[test]
    fn subscribe_task_allowed_for_agent() {
        let pat = TopicPattern::exact("task:status");
        assert!(PermissionChecker::check_subscribe(&pat, &sub_agent()).is_allowed());
    }

    #[test]
    fn subscribe_task_denied_for_extension() {
        let pat = TopicPattern::exact("task:status");
        assert!(PermissionChecker::check_subscribe(&pat, &sub_extension()).is_denied());
    }

    #[test]
    fn subscribe_system_allowed_for_all() {
        let pat = TopicPattern::exact("system:shutdown");
        assert!(PermissionChecker::check_subscribe(&pat, &sub_agent()).is_allowed());
        assert!(PermissionChecker::check_subscribe(&pat, &sub_extension()).is_allowed());
        assert!(PermissionChecker::check_subscribe(&pat, &sub_mcp()).is_allowed());
        assert!(PermissionChecker::check_subscribe(&pat, &sub_wasm()).is_allowed());
    }

    #[test]
    fn subscribe_ui_allowed_for_extension() {
        let pat = TopicPattern::exact("ui:render");
        assert!(PermissionChecker::check_subscribe(&pat, &sub_extension()).is_allowed());
    }

    #[test]
    fn subscribe_ui_denied_for_agent() {
        let pat = TopicPattern::exact("ui:render");
        assert!(PermissionChecker::check_subscribe(&pat, &sub_agent()).is_denied());
    }

    #[test]
    fn subscribe_wildcard_denied_for_non_daemon() {
        let pat = TopicPattern::wildcard("task:*");
        assert!(PermissionChecker::check_subscribe(&pat, &sub_agent()).is_denied());
        assert!(PermissionChecker::check_subscribe(&pat, &sub_extension()).is_denied());
    }

    #[test]
    fn subscribe_double_wildcard_denied_for_non_daemon() {
        let pat = TopicPattern::double_wildcard("task");
        assert!(PermissionChecker::check_subscribe(&pat, &sub_agent()).is_denied());
        assert!(PermissionChecker::check_subscribe(&pat, &sub_extension()).is_denied());
    }

    // ── PermissionResult methods ────────────────────────────────────

    #[test]
    fn permission_result_is_allowed_and_is_denied() {
        let allowed = PermissionResult::Allowed;
        assert!(allowed.is_allowed());
        assert!(!allowed.is_denied());

        let denied = PermissionResult::Denied("reason".into());
        assert!(!denied.is_allowed());
        assert!(denied.is_denied());
    }
}
