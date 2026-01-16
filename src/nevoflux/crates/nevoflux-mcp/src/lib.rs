/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Model Context Protocol (MCP) Implementation
//!
//! Client library for communicating with MCP servers

pub mod client;
pub mod protocol;
pub mod transport;

pub use client::McpClient;
