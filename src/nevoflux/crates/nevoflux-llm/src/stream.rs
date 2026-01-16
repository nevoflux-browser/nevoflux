/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Streaming utilities for LLM responses

use anyhow::Result;
use futures::stream::{Stream, StreamExt};
use std::pin::Pin;

/// LLM response stream type
pub type ResponseStream = Pin<Box<dyn Stream<Item = Result<String>> + Send>>;

/// Parse Server-Sent Events (SSE) stream
pub async fn parse_sse_stream(response: reqwest::Response) -> Result<ResponseStream> {
    let stream = response.bytes_stream().map(|result| {
        result
            .map_err(|e| anyhow::anyhow!("Stream error: {}", e))
            .and_then(|bytes| {
                String::from_utf8(bytes.to_vec()).map_err(|e| anyhow::anyhow!("UTF-8 error: {}", e))
            })
    });

    Ok(Box::pin(stream))
}
