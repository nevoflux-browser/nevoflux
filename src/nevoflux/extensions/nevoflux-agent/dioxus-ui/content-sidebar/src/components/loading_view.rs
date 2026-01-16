/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Loading view component

use dioxus::prelude::*;

/// Loading view - displays spinner while content is loading
#[component]
pub fn LoadingView() -> Element {
    rsx! {
        div {
            class: "loading-view",

            div {
                class: "spinner",
            }

            p {
                class: "loading-text",
                "Loading content..."
            }
        }
    }
}
