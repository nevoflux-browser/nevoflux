/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! LLM Client Library
//!
//! Provides unified interface for different LLM providers (Anthropic, OpenAI, etc.)

pub mod anthropic;
pub mod client;
pub mod openai;
pub mod stream;

pub use client::LlmClient;
