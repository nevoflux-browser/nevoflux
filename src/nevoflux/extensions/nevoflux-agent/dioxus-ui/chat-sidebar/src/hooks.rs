/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Custom Dioxus hooks for the Chat Sidebar

use dioxus::prelude::*;

/// Hook for local storage persistence
pub fn use_local_storage<T: serde::Serialize + serde::de::DeserializeOwned + Default + Clone + 'static>(
    key: &'static str,
) -> Signal<T> {
    use_signal(|| {
        // Try to load from storage
        if let Some(window) = web_sys::window() {
            if let Ok(Some(storage)) = window.local_storage() {
                if let Ok(Some(json)) = storage.get_item(key) {
                    if let Ok(v) = serde_json::from_str(&json) {
                        return v;
                    }
                }
            }
        }
        T::default()
    })
}

/// Save value to local storage
pub fn save_to_local_storage<T: serde::Serialize>(key: &str, value: &T) {
    if let Some(window) = web_sys::window() {
        if let Ok(Some(storage)) = window.local_storage() {
            if let Ok(json) = serde_json::to_string(value) {
                let _ = storage.set_item(key, &json);
            }
        }
    }
}

/// Keyboard modifier configuration
#[derive(Clone, Copy, Default)]
pub struct KeyModifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

impl KeyModifiers {
    pub fn ctrl() -> Self {
        Self { ctrl: true, ..Default::default() }
    }

    pub fn shift() -> Self {
        Self { shift: true, ..Default::default() }
    }

    pub fn ctrl_shift() -> Self {
        Self { ctrl: true, shift: true, ..Default::default() }
    }
}
