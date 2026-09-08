/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! WebExtension messaging layer
//!
//! Handles communication between Chat Sidebar (WASM) and background script.

mod bridge;
mod browser_tools;
pub mod device_login;
pub mod device_pair;
pub mod devices;
mod handler;
mod sender;

pub use browser_tools::execute_browser_tool;
pub use handler::init_message_listener;
pub use sender::*;
