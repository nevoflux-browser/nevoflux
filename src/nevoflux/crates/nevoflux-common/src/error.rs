/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Error types for NevoFlux

use thiserror::Error;

/// NevoFlux error type
#[derive(Error, Debug)]
pub enum Error {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("LLM error: {0}")]
    Llm(String),

    #[error("MCP error: {0}")]
    Mcp(String),

    #[error("WASM error: {0}")]
    Wasm(String),

    #[error("Browser error: {0}")]
    Browser(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Other error: {0}")]
    Other(String),
}

/// Result type alias using NevoFlux error
pub type Result<T> = std::result::Result<T, Error>;
