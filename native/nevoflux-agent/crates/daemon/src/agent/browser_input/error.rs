// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Error types for the browser input strategy engine.

use nevoflux_protocol::BrowserToolError;
use thiserror::Error;

/// Errors that can occur during strategy execution.
///
/// Variants map 1:1 to the error codes documented in the spec:
/// 1001 = ElementNotFound, 1002 = FocusFailed, ..., 1014 = VerifyMismatch.
/// `recoverable()` matches the spec's recoverability classification.
#[derive(Debug, Error)]
pub enum BrowserInputError {
    #[error("Element not found: {selector}")]
    ElementNotFound { selector: String },

    #[error("Could not focus target: {reason}")]
    FocusFailed { reason: String },

    #[error("Probe returned error: code {code}, {message}")]
    ProbeFailed { code: i32, message: String },

    #[error("Strategy aborted: {reason}")]
    Aborted { reason: String, recoverable: bool },

    #[error("Action {step}/{total} failed: {inner}")]
    ActionFailed {
        step: usize,
        total: usize,
        inner: Box<BrowserInputError>,
    },

    #[error("Verify mismatch: expected {expected:?}, got {actual:?}")]
    VerifyMismatch { expected: String, actual: String },

    #[error("Element is disabled or readonly")]
    ElementDisabled,

    #[error("Invalid selector: {0}")]
    InvalidSelector(String),

    #[error("Timeout after {ms}ms")]
    Timeout { ms: u64 },

    #[error("Bridge channel closed")]
    ChannelClosed,

    #[error("Bridge error: {0}")]
    Bridge(String),
}

impl BrowserInputError {
    /// Returns true if the caller may meaningfully retry this operation.
    pub fn recoverable(&self) -> bool {
        match self {
            Self::ElementNotFound { .. } => true,
            Self::FocusFailed { .. } => true,
            Self::ProbeFailed { .. } => true,
            Self::Aborted { recoverable, .. } => *recoverable,
            Self::ActionFailed { inner, .. } => inner.recoverable(),
            Self::VerifyMismatch { .. } => true,
            Self::ElementDisabled => false,
            Self::InvalidSelector(_) => false,
            Self::Timeout { .. } => true,
            Self::ChannelClosed => false,
            Self::Bridge(_) => false,
        }
    }

    /// Map a protocol-level `BrowserToolError` to our internal error type
    /// based on the numeric error code returned from the Actor.
    pub fn from_browser_error(err: BrowserToolError) -> Self {
        match err.code {
            1001 => Self::ElementNotFound {
                selector: err.message.clone(),
            },
            1002 => Self::FocusFailed {
                reason: err.message.clone(),
            },
            1007 => Self::InvalidSelector(err.message.clone()),
            1008 => Self::ElementDisabled,
            _ => Self::Bridge(format!("code={} msg={}", err.code, err.message)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn element_not_found_is_recoverable() {
        let e = BrowserInputError::ElementNotFound {
            selector: "#x".into(),
        };
        assert!(e.recoverable());
    }

    #[test]
    fn element_disabled_is_not_recoverable() {
        let e = BrowserInputError::ElementDisabled;
        assert!(!e.recoverable());
    }

    #[test]
    fn invalid_selector_is_not_recoverable() {
        let e = BrowserInputError::InvalidSelector("div>".into());
        assert!(!e.recoverable());
    }

    #[test]
    fn action_failed_defers_to_inner() {
        let inner = Box::new(BrowserInputError::ElementNotFound {
            selector: "#x".into(),
        });
        let outer = BrowserInputError::ActionFailed {
            step: 2,
            total: 5,
            inner,
        };
        assert!(outer.recoverable());

        let inner2 = Box::new(BrowserInputError::ElementDisabled);
        let outer2 = BrowserInputError::ActionFailed {
            step: 2,
            total: 5,
            inner: inner2,
        };
        assert!(!outer2.recoverable());
    }

    #[test]
    fn aborted_uses_its_recoverable_flag() {
        let e = BrowserInputError::Aborted {
            reason: "not visible".into(),
            recoverable: true,
        };
        assert!(e.recoverable());

        let e = BrowserInputError::Aborted {
            reason: "bad tag".into(),
            recoverable: false,
        };
        assert!(!e.recoverable());
    }

    #[test]
    fn from_browser_error_maps_1001_to_element_not_found() {
        let err = BrowserToolError {
            code: 1001,
            message: "#x".into(),
            recoverable: true,
        };
        let mapped = BrowserInputError::from_browser_error(err);
        assert!(matches!(mapped, BrowserInputError::ElementNotFound { .. }));
    }

    #[test]
    fn from_browser_error_maps_1008_to_element_disabled() {
        let err = BrowserToolError {
            code: 1008,
            message: "disabled".into(),
            recoverable: false,
        };
        let mapped = BrowserInputError::from_browser_error(err);
        assert!(matches!(mapped, BrowserInputError::ElementDisabled));
    }

    #[test]
    fn from_browser_error_unknown_code_becomes_bridge_error() {
        let err = BrowserToolError {
            code: 9999,
            message: "???".into(),
            recoverable: false,
        };
        let mapped = BrowserInputError::from_browser_error(err);
        assert!(matches!(mapped, BrowserInputError::Bridge(_)));
    }
}
