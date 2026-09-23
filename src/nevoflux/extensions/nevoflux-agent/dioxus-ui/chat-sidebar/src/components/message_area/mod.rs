/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Message area components

mod welcome_screen;
mod message_list;
mod message_bubble;
mod error_card;
mod code_block;
mod activity_feed;
mod live_tool_feed;
mod usage_format;
mod usage_stats;

pub use welcome_screen::WelcomeScreen;
pub use message_list::MessageList;
pub use message_bubble::MessageBubble;
pub use error_card::ErrorCard;
pub use code_block::CodeBlock;
pub use activity_feed::ActivityFeed;
pub use activity_feed::DoneFeed;
pub use live_tool_feed::LiveToolFeed;
pub use usage_stats::UsageStats;

use dioxus::prelude::*;
use wasm_bindgen::prelude::*;

#[wasm_bindgen(inline_js = r#"
export function copyTextFallback(text) {
    // execCommand fallback — works in extension sidebar where
    // navigator.clipboard.writeText is blocked by security context
    try {
        const ta = document.createElement('textarea');
        ta.value = text;
        ta.style.cssText = 'position:fixed;left:-9999px';
        document.body.appendChild(ta);
        ta.select();
        const ok = document.execCommand('copy');
        document.body.removeChild(ta);
        if (ok) return true;
    } catch (_) {}
    return false;
}

export function initCodeCopyDelegation() {
    if (window.__codeCopyInit) return;
    window.__codeCopyInit = true;
    document.addEventListener('click', function(e) {
        const btn = e.target.closest('.markdown-content .code-copy-btn');
        if (!btn) return;
        const codeBlock = btn.closest('.code-block');
        if (!codeBlock) return;
        const pre = codeBlock.querySelector('pre');
        if (!pre) return;
        const text = pre.textContent;
        // execCommand fallback (works in extension sidebar)
        try {
            const ta = document.createElement('textarea');
            ta.value = text;
            ta.style.cssText = 'position:fixed;left:-9999px';
            document.body.appendChild(ta);
            ta.select();
            const ok = document.execCommand('copy');
            document.body.removeChild(ta);
            if (ok) {
                btn.textContent = 'Copied!';
                btn.classList.add('copied');
                setTimeout(function() {
                    btn.textContent = 'Copy';
                    btn.classList.remove('copied');
                }, 2000);
                return;
            }
        } catch (_) {}
        // Async Clipboard API fallback
        navigator.clipboard.writeText(text).then(function() {
            btn.textContent = 'Copied!';
            btn.classList.add('copied');
            setTimeout(function() {
                btn.textContent = 'Copy';
                btn.classList.remove('copied');
            }, 2000);
        });
    });
}
"#)]
extern "C" {
    #[wasm_bindgen(js_name = copyTextFallback)]
    pub fn copy_text_fallback(text: &str) -> bool;

    #[wasm_bindgen(js_name = initCodeCopyDelegation)]
    pub fn init_code_copy_delegation();
}
use crate::context::use_app_context;

/// Message area container - shows welcome screen or message list
#[component]
pub fn MessageArea() -> Element {
    let ctx = use_app_context();
    let messages = ctx.messages.read();
    let streaming = ctx.streaming.read();
    let is_empty = messages.is_empty() && streaming.is_none();
    // An empty transcript means "nothing has been said here yet" — unless
    // `/clear` is what emptied it, in which case it means "this conversation
    // was cleared and is still open". The welcome screen answers the first and
    // would be a lie about the second.
    let was_cleared = ctx
        .cleared_session
        .read()
        .as_deref()
        .is_some_and(|id| id == ctx.session.read().id);

    // Set up event delegation for code copy buttons in markdown content (once)
    use_effect(|| { init_code_copy_delegation(); });

    rsx! {
        div { class: "message-area",
            // Sticky stack of /loop cards (spec §2.6).
            // Renders nothing when no active loops exist for this session.
            crate::components::loop_ui::StickyLoopCards {}
            if is_empty && was_cleared {
                div { class: "session-cleared-note", "Session cleared" }
            } else if is_empty {
                WelcomeScreen {}
            } else {
                MessageList {}
            }
        }
    }
}
