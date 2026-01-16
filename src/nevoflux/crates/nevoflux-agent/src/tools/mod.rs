/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Tools Module for Computer Use
//!
//! This module defines the browser control tools that can be invoked by
//! the LLM to interact with web pages via the Content Sidebar.

mod definitions;
mod executor;

pub use definitions::*;
pub use executor::*;
