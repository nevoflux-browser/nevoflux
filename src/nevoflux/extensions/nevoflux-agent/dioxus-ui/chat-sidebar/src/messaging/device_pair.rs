/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! `/pair-device`: pair a phone with this machine, durably (design §12).
//!
//! Distinct from `/remote-control`, which opens one channel onto one session
//! for one sitting and keeps nothing — a restart leaves the far end on a relay
//! channel this daemon will never dial again. A pairing is stored and dialled
//! at every startup, which is what lets a phone still work tomorrow.
//!
//! **The QR encodes the link and nothing else.** Putting the pairing code in it
//! would write key material into browser history and any referrer, because that
//! code is what derives the channel keys — the same reason a `?pair_token=` in
//! the URL was rejected. Keeping them apart is also free double-factor: the
//! scan proves you reached the machine, typing proves you were standing at it.
//! And the code's alphabet was chosen to be unambiguous when read off a screen,
//! which is only worth anything if a person reads it.
//!
//! **Generated here rather than in the daemon.** The headless head prints its
//! connect block to a pipe and cannot show a picture, so a QR would serve
//! exactly one of the two callers — and has no business in the code they share.

use crate::messaging::signin::needs_sign_in;
use crate::state::Message;
use base64::Engine;
use dioxus::prelude::*;

/// Where the paired device's app lives. One installed app per machine, scoped
/// to its own control channel — see the portal's `/d/[device]/`.
const PORTAL_BASE: &str = "https://portal.nevoflux.app";

/// The QR, as an `<img>`-able data URI.
///
/// SVG rather than a bitmap: it is a few hundred bytes, stays sharp at whatever
/// size the bubble gives it, and needs no image encoder.
fn qr_data_uri(link: &str) -> Option<String> {
    use qrcode::render::svg;
    use qrcode::{EcLevel, QrCode};

    // Medium correction. A screen is not a printed label — it will not be
    // creased or smudged — and lower correction keeps the modules large enough
    // to scan from a comfortable distance.
    let code = QrCode::with_error_correction_level(link, EcLevel::M).ok()?;
    let svg = code
        .render::<svg::Color>()
        .min_dimensions(220, 220)
        // Explicit black on white regardless of the sidebar's theme: a scanner
        // wants contrast in the direction it expects, and a QR rendered light-
        // on-dark is read by fewer of them than people assume.
        .dark_color(svg::Color("#000000"))
        .light_color(svg::Color("#ffffff"))
        .build();
    Some(format!(
        "data:image/svg+xml;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(svg)
    ))
}

/// What the sidebar says once a pairing exists.
///
/// Split out so the wording is testable without a daemon behind it — this text
/// carries the one instruction the whole flow depends on, and getting it wrong
/// costs somebody a pairing they have to do twice.
pub fn compose_pairing_message(link: &str, code: &str, qr: Option<&str>) -> String {
    let picture = match qr {
        Some(uri) => format!("\n\n![Scan to pair]({uri})\n"),
        // A QR that would not build is not worth an error: the link below it is
        // the thing that matters, and it is right there to be typed or sent.
        None => String::new(),
    };
    format!(
        "✅ This machine is ready to pair.{picture}\nScan the code above, or open:\n\n**{link}**\n\n\
         **Add it to your home screen first**, then open it from there and enter this pairing code:\n\n\
         ## `{code}`\n\n\
         The order matters: on iPhone the home-screen app and Safari keep separate storage, so \
         pairing in the browser first leaves the app knowing nothing.\n\n\
         This code is shown once. The pairing itself survives restarts — you will not have to do \
         this again."
    )
}

/// What the sidebar says when pairing did not happen.
pub fn compose_pairing_failure(reason: &str) -> String {
    if needs_sign_in(reason) {
        return "Could not pair this machine: it is not signed in to NevoFlux (the sign-in may have expired).\n\nRun `/remote-control` to sign in, then run `/pair-device` again.".to_string();
    }
    format!("Could not pair this machine: {reason}")
}

/// Run the pairing flow and report it into the transcript.
pub async fn run(mut messages: Signal<Vec<Message>>) {
    match crate::messaging::remote_pair().await {
        Ok((control_channel_id, code)) => {
            let link = format!("{PORTAL_BASE}/d/{control_channel_id}/pair");
            let qr = qr_data_uri(&link);
            messages
                .write()
                .push(Message::assistant_markdown(compose_pairing_message(
                    &link,
                    &code,
                    qr.as_deref(),
                )));
        }
        Err(e) => {
            messages
                .write()
                .push(Message::assistant_markdown(compose_pairing_failure(&e)));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_sign_in_says_to_sign_in_again() {
        for reason in [
            "log in first",
            "Invalid request: no JWT in set-auth-jwt header or token body",
        ] {
            let text = compose_pairing_failure(reason);
            assert!(text.contains("/remote-control"), "{text}");
            assert!(text.contains("/pair-device"), "{text}");
            assert!(text.contains("not signed in"), "{text}");
            for l in text.split('\n') {
                assert!(!l.starts_with("    "), "{l:?}");
            }
        }
    }

    #[test]
    fn any_other_failure_keeps_its_reason() {
        let text = compose_pairing_failure("network down");
        assert_eq!(text, "Could not pair this machine: network down");
    }

    #[test]
    fn the_qr_carries_the_link_and_not_the_code() {
        // The code derives the channel keys. In a URL it would land in browser
        // history and any referrer, which is the whole reason it is typed.
        let link = "https://portal.nevoflux.app/d/abc-123/pair";
        let text = compose_pairing_message(link, "A-BCDE-FGHJ-KMNP", Some("data:image/svg+xml;base64,ZmFrZQ=="));
        let qr_line = text
            .lines()
            .find(|l| l.starts_with("![Scan to pair]"))
            .expect("the picture is rendered");
        assert!(!qr_line.contains("A-BCDE"), "the code must not be encoded");
        assert!(text.contains(link));
        assert!(text.contains("A-BCDE-FGHJ-KMNP"), "but it is shown to type");
    }

    #[test]
    fn the_link_still_stands_without_a_picture() {
        // A QR that would not build is not worth an error; the link is the part
        // that matters and it can be typed or sent.
        let link = "https://portal.nevoflux.app/d/abc-123/pair";
        let text = compose_pairing_message(link, "A-BCDE-FGHJ-KMNP", None);
        assert!(!text.contains("!["));
        assert!(text.contains(link));
        assert!(text.contains("A-BCDE-FGHJ-KMNP"));
    }

    #[test]
    fn install_before_pairing_is_stated_as_an_order_not_a_hint() {
        // On iPhone the home-screen app and Safari keep separate storage, so
        // pairing in the browser first produces an app that knows nothing — and
        // reads, to the person holding it, as the pairing having failed.
        let text = compose_pairing_message("https://x/y", "CODE", None);
        let install = text.find("home screen").expect("says to install");
        let enter = text.find("enter this pairing code").expect("says to pair");
        assert!(install < enter, "installing has to come first in the copy");
    }

    #[test]
    fn a_link_encodes_to_a_picture_a_browser_can_show() {
        let uri = qr_data_uri("https://portal.nevoflux.app/d/abc-123/pair")
            .expect("a short https link always encodes");
        // The markdown renderer admits `data:image/`, and nothing else.
        assert!(uri.starts_with("data:image/svg+xml;base64,"));
        let payload = uri.trim_start_matches("data:image/svg+xml;base64,");
        let svg = base64::engine::general_purpose::STANDARD
            .decode(payload)
            .expect("valid base64");
        assert!(String::from_utf8_lossy(&svg).contains("<svg"));
    }
}
