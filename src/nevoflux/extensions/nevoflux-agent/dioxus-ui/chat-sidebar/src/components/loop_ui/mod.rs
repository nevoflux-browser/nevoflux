/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Sidebar UI for the /loop skill (spec §2.6).
//!
//! StickyLoopCards stack at the top of the chat area; IterationCard
//! renders inline in the message stream when iterations fire.

pub mod iteration_card;
pub mod sticky_loop_card;

pub use iteration_card::IterationCard;
pub use sticky_loop_card::StickyLoopCards;
