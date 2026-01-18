/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Channel handlers for 4-channel architecture

pub mod input;
pub mod output;
pub mod mcp;
pub mod page_llm;

pub use input::InputChannelHandler;
pub use output::OutputChannelHandler;
pub use mcp::McpChannelHandler;
pub use page_llm::PageLlmChannelHandler;
