//! Session fixtures for testing.

use nevoflux_storage::{Session, SessionMode};

/// Create a sample session for testing.
///
/// Returns a session with sensible defaults:
/// - ID: "test-session-001"
/// - Title: "Test Session"
/// - Mode: Chat
/// - Not pinned, not archived
pub fn sample_session() -> Session {
    Session::new()
        .with_id("test-session-001")
        .with_title("Test Session")
        .with_mode(SessionMode::Chat)
}

/// Create a sample agent session for testing.
pub fn sample_agent_session() -> Session {
    Session::new()
        .with_id("test-agent-001")
        .with_title("Agent Test Session")
        .with_mode(SessionMode::Agent)
}

/// Create multiple sample sessions for testing.
pub fn sample_sessions(count: usize) -> Vec<Session> {
    (0..count)
        .map(|i| {
            Session::new()
                .with_id(format!("test-session-{:03}", i))
                .with_title(format!("Test Session {}", i))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sample_session() {
        let session = sample_session();

        assert_eq!(session.id, "test-session-001");
        assert_eq!(session.title, Some("Test Session".to_string()));
        assert_eq!(session.mode, SessionMode::Chat);
        assert!(!session.pinned);
        assert!(!session.archived);
    }

    #[test]
    fn test_sample_agent_session() {
        let session = sample_agent_session();

        assert_eq!(session.id, "test-agent-001");
        assert_eq!(session.mode, SessionMode::Agent);
    }

    #[test]
    fn test_sample_sessions_count() {
        let sessions = sample_sessions(5);

        assert_eq!(sessions.len(), 5);
        for (i, session) in sessions.iter().enumerate() {
            assert_eq!(session.id, format!("test-session-{:03}", i));
        }
    }
}
