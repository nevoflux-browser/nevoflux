/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Recognising "the person is not signed in" in a pairing failure.

/// Whether a pairing failure means the person has to sign in again.
///
/// `"log in first"` is the daemon's `OpenError::NotLoggedIn` text; `"no JWT in
/// set-auth-jwt"` is what older daemons said for an expired session.
pub fn needs_sign_in(reason: &str) -> bool {
    let reason = reason.to_lowercase();
    reason.contains("log in first") || reason.contains("no jwt in set-auth-jwt")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recognises_the_daemon_text() {
        assert!(needs_sign_in("log in first"));
    }

    #[test]
    fn recognises_the_older_daemon_text() {
        assert!(needs_sign_in(
            "Invalid request: no JWT in set-auth-jwt header or token body"
        ));
    }

    #[test]
    fn other_failures_are_not_sign_in() {
        assert!(!needs_sign_in("unknown command: remote.pair_agent"));
        assert!(!needs_sign_in("network down"));
    }
}
