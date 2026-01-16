/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Browser Control Abstraction
//!
//! Provides high-level interface for controlling the browser

pub mod commands;
pub mod protocol;

pub use commands::BrowserController;
