/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

use dioxus::prelude::*;
use crate::context::AppContext;

#[component]
pub fn EventBusListener() -> Element {
    let ctx = use_context::<AppContext>();
    let notifications = ctx.event_notifications.read();
    let visible: Vec<_> = notifications.iter().rev().take(3).collect();

    rsx! {
        if !visible.is_empty() {
            div { class: "nevo-event-toasts",
                for notif in visible {
                    div {
                        class: "nevo-event-toast",
                        key: "{notif.id}",
                        div { class: "nevo-event-toast-title", "{notif.title}" }
                        if !notif.body.is_empty() {
                            div { class: "nevo-event-toast-body", "{notif.body}" }
                        }
                    }
                }
            }
        }
    }
}
