/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Header component

use crate::bindings::nevoflux_api;
use crate::context::{use_app_context, AppContext};
use dioxus::prelude::*;
use wasm_bindgen_futures::spawn_local;

/// Header component with History and Maximize buttons
#[component]
pub fn Header() -> Element {
    let ctx = use_app_context();

    // Read maximize state
    let is_maximized = ctx.maximize.read().is_maximized;

    // On-device latch, for the header badge.
    let local_on = *ctx.local_on.read();

    let toggle_history = {
        let mut ctx = ctx.clone();
        move |_| {
            let current = *ctx.show_history_panel.read();
            ctx.show_history_panel.set(!current);

            // Refresh history when opening
            if !current {
                ctx.history.write().set_loading();
                spawn_local(async move {
                    let _ = crate::messaging::send_session_list(50, 0).await;
                });
            }
        }
    };

    // Handle minimize: close sidebar + show floating avatar (replaces the rail).
    //
    // The real work is owned by the CSP-safe init.js capture-phase handler for
    // `.minimize-btn` (injected by scripts/fix-csp.py at build time), mirroring the
    // maximize handler. Within the click gesture that plain-JS handler dispatches
    // `{type:'bg:agent_minimize'}` (background shows the avatar + starts the
    // keepalive) and then calls `browser.sidebarAction.close()`. Because it runs in
    // the capture phase and calls stopPropagation, this Dioxus onclick never fires
    // in the built artifact — it only records intent. The previous sync send +
    // `try_close_sidebar_sync()` (js_sys::eval) close was CSP-dead: the extension
    // CSP (`script-src 'self' 'wasm-unsafe-eval'`) blocks eval(), so the close was a
    // silent no-op (split-brain: sidebar stayed open with the avatar + keepalive).
    let handle_minimize = {
        move |_| {
            tracing::info!("Minimize to floating avatar requested");
        }
    };

    // Handle maximize: open in new tab, close sidebar
    let handle_maximize = {
        let ctx = ctx.clone();
        move |_| {
            tracing::info!("Maximize requested");

            // Try to close sidebar IMMEDIATELY (sync) to preserve user gesture context
            nevoflux_api::try_close_sidebar_sync();

            let ctx = ctx.clone();
            spawn_local(async move {
                if let Err(e) = do_maximize(ctx, "").await {
                    tracing::error!("Failed to maximize: {}", e);
                }
            });
        }
    };

    // Handle restore: close tab, activate source tab, open sidebar
    let handle_restore = {
        let ctx = ctx.clone();
        move |_| {
            tracing::info!("Restore requested");
            let ctx = ctx.clone();
            spawn_local(async move {
                if let Err(e) = do_restore(ctx).await {
                    tracing::error!("Failed to restore: {}", e);
                }
            });
        }
    };

    // Scheduled-jobs calendar button. `has_jobs`/`jobs_failed` derive from BOTH
    // the per-schedule map and the aggregate snapshot: on a fresh sidebar the
    // map may be sparse (only sticky `state_changed` primed it), so the sticky
    // `snapshot` aggregate (`active`/`failed_recent`) backstops the dot state.
    let (has_jobs, jobs_failed) = {
        let schedule_jobs = ctx.schedule_jobs.read();
        let snapshot = ctx.schedule_snapshot.read();
        // Count only non-terminal (live) schedules. Terminal rows (`cancelled`/
        // `ran`) must never light the badge — belt-and-braces against any event
        // path that could leave a terminal entry in the map.
        let is_live = |j: &crate::state::ScheduleJobState| j.status != "cancelled" && j.status != "ran";
        let map_has = schedule_jobs.values().any(is_live);
        let snap_active = snapshot
            .as_ref()
            .and_then(|s| s.get("active"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            > 0;
        let map_failed = schedule_jobs.values().any(|j| {
            is_live(j) && matches!(j.last_run_status.as_deref(), Some("error") | Some("missed"))
        });
        let snap_failed = snapshot
            .as_ref()
            .and_then(|s| s.get("failed_recent"))
            .and_then(|v| v.as_i64())
            .unwrap_or(0)
            > 0;
        (map_has || snap_active, map_failed || snap_failed)
    };

    let jobs_btn_class = if has_jobs {
        "header-btn jobs-btn has-jobs"
    } else {
        "header-btn jobs-btn"
    };

    // Calendar click (docked form only — the button is hidden when maximized,
    // where LeftMenu owns the Schedule Jobs entry). The whole flow — open the
    // maximized Jobs view (`&panel=jobs`) + close the sidebar — is owned by the
    // CSP-safe `.jobs-btn` capture-phase handler in sidebar-boot.js, which
    // stopPropagation's this Dioxus onclick. Mirrors handle_minimize: the Rust
    // try_close_sidebar_sync() close is CSP-dead (js_sys::eval is blocked by the
    // extension CSP), so plain JS must own the sidebar close within the gesture.
    // This closure only records intent and never fires in the built artifact.
    let toggle_jobs = move |_| {
        tracing::info!("Schedule Jobs clicked (handled by sidebar-boot.js)");
    };

    // The header avatar wears the face of whoever answers in this Space: the
    // Space's bound soul, or a temporary `@`-pick, falling back to the global
    // identity avatar when nothing is bound. Reading these signals here means the
    // face updates the moment the user switches Space (the container changes).
    let avatar: Option<String> = {
        let container = ctx.tab_context.read().cookie_store_id.clone();
        let picked = ctx
            .active_soul
            .read()
            .as_ref()
            .and_then(|a| a.is_override.then(|| a.slug.clone()));
        crate::state::soul::derive_active_soul(
            &ctx.souls.read(),
            &ctx.soul_bindings.read(),
            &container,
            picked.as_deref(),
        )
        .and_then(|a| a.avatar)
        .or_else(|| ctx.avatar_url.read().clone())
    };

    rsx! {
        header { class: "header",
            // Left side: Avatar (the current Space's soul, else global identity)
            div { class: "header-left",
                if let Some(ref url) = avatar {
                    div { class: "header-avatar",
                        img {
                            src: "{url}",
                            alt: "Avatar",
                            class: "header-avatar-img",
                        }
                    }
                }
                // Says where the conversation goes, and is careful not to
                // promise more than the latch delivers: tools still reach the
                // network, and claiming otherwise would be the worse error.
                if local_on {
                    span {
                        class: "header-local-badge",
                        title: "Conversation content isn't sent to any AI provider. Web pages, search and MCP tools still use the network.",
                        "On-device"
                    }
                }
            }

            // Right side: Action buttons
            div { class: "header-right",
                // Scheduled jobs (calendar) button — docked form only. In the
                // maximized form LeftMenu owns the Schedule Jobs entry, so this
                // header button would be a duplicate; hide it there.
                if !is_maximized {
                    button {
                        class: "{jobs_btn_class}",
                        aria_label: "Scheduled jobs",
                        title: "Scheduled jobs",
                        onclick: toggle_jobs,
                        svg {
                            xmlns: "http://www.w3.org/2000/svg",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            width: "16",
                            height: "16",
                            rect { x: "3", y: "4", width: "18", height: "18", rx: "2" }
                            line { x1: "16", y1: "2", x2: "16", y2: "6" }
                            line { x1: "8", y1: "2", x2: "8", y2: "6" }
                            line { x1: "3", y1: "10", x2: "21", y2: "10" }
                            circle { cx: "12", cy: "16", r: "2", fill: "currentColor", stroke: "none" }
                        }
                        if jobs_failed {
                            span { class: "jobs-btn-dot failed" }
                        } else if has_jobs {
                            span { class: "jobs-btn-dot" }
                        }
                    }
                }

                // History button
                button {
                    class: "header-btn history-btn",
                    aria_label: "History",
                    title: "Conversation history",
                    onclick: toggle_history,
                    // Clock/history icon
                    svg {
                        width: "16",
                        height: "16",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        circle { cx: "12", cy: "12", r: "10" }
                        path { d: "M12 6v6l4 2" }
                    }
                }

                // Maximize/Restore button
                if is_maximized {
                    // Restore button (in tab mode)
                    button {
                        class: "header-btn restore-btn",
                        aria_label: "Restore to sidebar",
                        title: "Restore to sidebar",
                        onclick: handle_restore,
                        // Panel-right glyph — "back to sidebar"
                        svg {
                            width: "16",
                            height: "16",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            rect { x: "3", y: "3", width: "18", height: "18", rx: "2" }
                            line { x1: "15", y1: "3", x2: "15", y2: "21" }
                            path { d: "M19 10l2-2-2-2" }
                        }
                    }
                } else {
                    // Maximize button (in sidebar mode)
                    button {
                        class: "header-btn maximize-btn",
                        aria_label: "Open in new tab",
                        title: "Open in new tab",
                        onclick: handle_maximize,
                        // Arrows pointing outward icon (maximize)
                        svg {
                            width: "16",
                            height: "16",
                            view_box: "0 0 24 24",
                            fill: "none",
                            stroke: "currentColor",
                            stroke_width: "2",
                            stroke_linecap: "round",
                            stroke_linejoin: "round",
                            path { d: "M15 3h6v6" }
                            path { d: "M9 21H3v-6" }
                            path { d: "M21 3l-7 7" }
                            path { d: "M3 21l7-7" }
                        }
                    }
                }

                // Minimize button (both modes) — always the last / rightmost child
                button {
                    class: "header-btn minimize-btn",
                    aria_label: "Minimize to floating avatar",
                    title: "Minimize to floating avatar",
                    onclick: handle_minimize,
                    // Window-minimize glyph (single bottom horizontal line)
                    svg {
                        width: "16",
                        height: "16",
                        view_box: "0 0 24 24",
                        fill: "none",
                        stroke: "currentColor",
                        stroke_width: "2",
                        stroke_linecap: "round",
                        stroke_linejoin: "round",
                        path { d: "M5 19h14" }
                    }
                }
            }
        }
    }
}

// ==================== Maximize/Restore Logic ====================

/// Maximize: open chat in new tab, close sidebar.
///
/// `extra_query` is appended verbatim to the maximized URL's query string
/// (must start with `&`, e.g. `"&panel=jobs"`), so callers can deep-link a
/// panel to open on boot. Pass `""` for a plain maximize.
async fn do_maximize(ctx: AppContext, extra_query: &str) -> Result<(), String> {
    // Get session_id from current session
    let session_id = ctx.session.read().id.clone();

    // Get target_tab_id from tab_context (the tab AI operates on)
    let target_tab_id = ctx.tab_context.read().tab_id;

    // Get source_tab_id (current active tab where sidebar is shown)
    let source_tab = nevoflux_api::get_active_tab().await?;
    let source_tab_id = source_tab.id as i32;

    // Build URL with parameters
    let base_url = web_sys::window()
        .and_then(|w| w.location().href().ok())
        .unwrap_or_else(|| "moz-extension://unknown/wasm/chat-sidebar/index.html".to_string());

    // Extract base path (remove any existing query params)
    let base_path = base_url.split('?').next().unwrap_or(&base_url);

    let url = format!(
        "{}?mode=maximized&session_id={}&target_tab_id={}&source_tab_id={}{}",
        base_path, session_id, target_tab_id, source_tab_id, extra_query
    );

    tracing::info!("Opening maximized view: {}", url);

    // Create new tab with the URL
    // Note: sidebar close is attempted synchronously in the click handler
    // to preserve user gesture context (Firefox security requirement)
    nevoflux_api::create_tab(&url, true).await?;

    Ok(())
}

/// Restore: close current tab, activate source tab, open sidebar
async fn do_restore(ctx: AppContext) -> Result<(), String> {
    let maximize_state = ctx.maximize.read();
    let source_tab_id = maximize_state.source_tab_id;
    drop(maximize_state);

    // Get current tab ID (we're in a tab, not sidebar)
    let current_tab = nevoflux_api::get_current_tab()
        .await?
        .ok_or_else(|| "Could not get current tab".to_string())?;
    let current_tab_id = current_tab.id as i32;

    // Activate source tab (if it still exists)
    if let Some(source_id) = source_tab_id {
        if let Err(e) = nevoflux_api::update_tab(source_id, true).await {
            tracing::warn!("Failed to activate source tab {}: {}", source_id, e);
            // Tab might have been closed - continue anyway
        }
    }

    // Open the sidebar
    if let Err(e) = nevoflux_api::open_sidebar().await {
        tracing::warn!("Failed to open sidebar: {}", e);
        // Continue anyway
    }

    // Close current tab
    nevoflux_api::remove_tab(current_tab_id).await?;

    Ok(())
}
