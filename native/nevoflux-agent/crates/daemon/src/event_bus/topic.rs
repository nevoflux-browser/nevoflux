//! Topic validation and pattern matching for the EventBus.
//!
//! Topics use colon-separated segments (e.g. `task:status:updated`).
//! Each segment must contain only `[a-zA-Z0-9_-]` characters.
//! Patterns may additionally use `*` as a single-segment wildcard.

use std::fmt;

/// Maximum total length of a topic string.
pub const MAX_TOPIC_LEN: usize = 256;

/// Maximum number of colon-separated segments.
pub const MAX_SEGMENTS: usize = 8;

/// Maximum length of a single segment.
pub const MAX_SEGMENT_LEN: usize = 64;

/// Errors produced during topic or pattern validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TopicError {
    /// The topic string is empty.
    Empty,
    /// The topic exceeds [`MAX_TOPIC_LEN`] characters.
    TooLong { len: usize },
    /// The topic has more than [`MAX_SEGMENTS`] segments.
    TooManySegments { count: usize },
    /// A segment between colons is empty (e.g. `foo::bar`).
    EmptySegment { position: usize },
    /// A segment exceeds [`MAX_SEGMENT_LEN`] characters.
    SegmentTooLong { position: usize, len: usize },
    /// A segment contains a character outside `[a-zA-Z0-9_-]`.
    InvalidCharacter { position: usize, ch: char },
}

impl fmt::Display for TopicError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TopicError::Empty => write!(f, "topic must not be empty"),
            TopicError::TooLong { len } => {
                write!(f, "topic length {len} exceeds maximum {MAX_TOPIC_LEN}")
            }
            TopicError::TooManySegments { count } => {
                write!(f, "topic has {count} segments, maximum is {MAX_SEGMENTS}")
            }
            TopicError::EmptySegment { position } => {
                write!(f, "segment {position} is empty")
            }
            TopicError::SegmentTooLong { position, len } => {
                write!(
                    f,
                    "segment {position} length {len} exceeds maximum {MAX_SEGMENT_LEN}"
                )
            }
            TopicError::InvalidCharacter { position, ch } => {
                write!(f, "segment {position} contains invalid character '{ch}'")
            }
        }
    }
}

impl std::error::Error for TopicError {}

/// Returns true if `ch` is a valid topic segment character: `[a-zA-Z0-9_-]`.
fn is_valid_segment_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '-'
}

/// Validate a concrete topic string (no wildcards allowed).
///
/// Rules:
/// - Must not be empty
/// - Must not exceed [`MAX_TOPIC_LEN`] bytes
/// - At most [`MAX_SEGMENTS`] colon-separated segments
/// - Each segment must be non-empty, at most [`MAX_SEGMENT_LEN`] bytes
/// - Each segment must match `[a-zA-Z0-9_-]+`
pub fn validate_topic(topic: &str) -> Result<(), TopicError> {
    if topic.is_empty() {
        return Err(TopicError::Empty);
    }
    if topic.len() > MAX_TOPIC_LEN {
        return Err(TopicError::TooLong { len: topic.len() });
    }

    let segments: Vec<&str> = topic.split(':').collect();
    if segments.len() > MAX_SEGMENTS {
        return Err(TopicError::TooManySegments {
            count: segments.len(),
        });
    }

    for (i, seg) in segments.iter().enumerate() {
        if seg.is_empty() {
            return Err(TopicError::EmptySegment { position: i });
        }
        if seg.len() > MAX_SEGMENT_LEN {
            return Err(TopicError::SegmentTooLong {
                position: i,
                len: seg.len(),
            });
        }
        for ch in seg.chars() {
            if !is_valid_segment_char(ch) {
                return Err(TopicError::InvalidCharacter { position: i, ch });
            }
        }
    }

    Ok(())
}

/// Validate a topic pattern string.
///
/// Same rules as [`validate_topic`], but a segment may also be a single `*`
/// (single-segment wildcard). Double wildcards (`**`) are rejected.
pub fn validate_pattern(pattern: &str) -> Result<(), TopicError> {
    if pattern.is_empty() {
        return Err(TopicError::Empty);
    }
    if pattern.len() > MAX_TOPIC_LEN {
        return Err(TopicError::TooLong { len: pattern.len() });
    }

    let segments: Vec<&str> = pattern.split(':').collect();
    if segments.len() > MAX_SEGMENTS {
        return Err(TopicError::TooManySegments {
            count: segments.len(),
        });
    }

    for (i, seg) in segments.iter().enumerate() {
        if seg.is_empty() {
            return Err(TopicError::EmptySegment { position: i });
        }
        if seg.len() > MAX_SEGMENT_LEN {
            return Err(TopicError::SegmentTooLong {
                position: i,
                len: seg.len(),
            });
        }
        // Allow single `*` as a wildcard segment; reject `**`.
        if *seg == "*" {
            continue;
        }
        if *seg == "**" {
            return Err(TopicError::InvalidCharacter {
                position: i,
                ch: '*',
            });
        }
        for ch in seg.chars() {
            if !is_valid_segment_char(ch) {
                return Err(TopicError::InvalidCharacter { position: i, ch });
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event_bus::TopicPattern;

    // ── validate_topic ──────────────────────────────────────────────

    #[test]
    fn valid_simple_topic() {
        assert!(validate_topic("task").is_ok());
    }

    #[test]
    fn valid_hierarchical_topic() {
        assert!(validate_topic("task:status:updated").is_ok());
    }

    #[test]
    fn valid_topic_with_hyphens_underscores() {
        assert!(validate_topic("my-topic:sub_topic:v2").is_ok());
    }

    #[test]
    fn valid_max_segments() {
        let topic = (0..MAX_SEGMENTS)
            .map(|i| format!("s{i}"))
            .collect::<Vec<_>>()
            .join(":");
        assert!(validate_topic(&topic).is_ok());
    }

    #[test]
    fn invalid_empty_topic() {
        assert_eq!(validate_topic(""), Err(TopicError::Empty));
    }

    #[test]
    fn invalid_too_long_topic() {
        let topic = "a".repeat(MAX_TOPIC_LEN + 1);
        assert!(matches!(
            validate_topic(&topic),
            Err(TopicError::TooLong { .. })
        ));
    }

    #[test]
    fn invalid_too_many_segments() {
        let topic = (0..MAX_SEGMENTS + 1)
            .map(|i| format!("s{i}"))
            .collect::<Vec<_>>()
            .join(":");
        assert!(matches!(
            validate_topic(&topic),
            Err(TopicError::TooManySegments { .. })
        ));
    }

    #[test]
    fn invalid_empty_segment() {
        assert!(matches!(
            validate_topic("foo::bar"),
            Err(TopicError::EmptySegment { position: 1 })
        ));
    }

    #[test]
    fn invalid_segment_too_long() {
        let long_seg = "a".repeat(MAX_SEGMENT_LEN + 1);
        let topic = format!("ok:{long_seg}");
        assert!(matches!(
            validate_topic(&topic),
            Err(TopicError::SegmentTooLong { position: 1, .. })
        ));
    }

    #[test]
    fn invalid_bad_chars() {
        assert!(matches!(
            validate_topic("foo:bar baz"),
            Err(TopicError::InvalidCharacter {
                position: 1,
                ch: ' '
            })
        ));
        assert!(matches!(
            validate_topic("foo:b@r"),
            Err(TopicError::InvalidCharacter {
                position: 1,
                ch: '@'
            })
        ));
    }

    // ── validate_pattern ────────────────────────────────────────────

    #[test]
    fn valid_pattern_with_wildcard() {
        assert!(validate_pattern("task:*").is_ok());
        assert!(validate_pattern("*:status").is_ok());
        assert!(validate_pattern("task:*:updated").is_ok());
    }

    #[test]
    fn pattern_rejects_double_wildcard() {
        assert!(matches!(
            validate_pattern("task:**"),
            Err(TopicError::InvalidCharacter {
                position: 1,
                ch: '*'
            })
        ));
    }

    #[test]
    fn pattern_rejects_star_embedded_in_segment() {
        // `fo*` is not a bare `*`, so the `*` is an invalid char
        assert!(matches!(
            validate_pattern("fo*:bar"),
            Err(TopicError::InvalidCharacter {
                position: 0,
                ch: '*'
            })
        ));
    }

    // ── TopicPattern::matches() ─────────────────────────────────────

    #[test]
    fn exact_pattern_matches() {
        let pat = TopicPattern::exact("task:status");
        assert!(pat.matches("task:status"));
        assert!(!pat.matches("task:status:updated"));
        assert!(!pat.matches("task"));
    }

    #[test]
    fn wildcard_pattern_matches() {
        let pat = TopicPattern::wildcard("task:*");
        assert!(pat.matches("task:status"));
        assert!(pat.matches("task:progress"));
        assert!(!pat.matches("task:status:updated"));
        assert!(!pat.matches("other:status"));
    }

    #[test]
    fn double_wildcard_pattern_matches() {
        let pat = TopicPattern::double_wildcard("task");
        assert!(pat.matches("task"));
        assert!(pat.matches("task:status"));
        assert!(pat.matches("task:status:updated"));
        assert!(!pat.matches("other:status"));

        // Empty prefix matches everything
        let all = TopicPattern::double_wildcard("");
        assert!(all.matches("anything"));
        assert!(all.matches("any:thing:at:all"));
    }
}
