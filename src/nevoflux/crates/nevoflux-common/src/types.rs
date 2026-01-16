/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Common types used across NevoFlux

use serde::{Deserialize, Serialize};

/// Message role in conversation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
}

/// Conversation message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

/// Browser action command
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum BrowserAction {
    Navigate { url: String },
    Click { selector: String },
    FillForm { fields: Vec<FormField> },
    ExtractContent,
    Screenshot,
}

/// Form field to fill
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FormField {
    pub selector: String,
    pub value: String,
}

/// Page content extracted from browser
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageContent {
    pub title: String,
    pub url: String,
    pub text: String,
    pub html: Option<String>,
}
