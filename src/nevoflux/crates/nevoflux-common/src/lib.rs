/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! NevoFlux Common Types and Utilities
//!
//! Shared types, error definitions, and configuration used across all crates.

pub mod config;
pub mod error;
pub mod protocol;
pub mod types;

pub use error::{Error, Result};
