/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Shadow DOM host creation and management
//!
//! This module handles the creation of a Shadow DOM container
//! for the Content Sidebar, providing style isolation from the
//! host page.

use wasm_bindgen::prelude::*;
use web_sys::{Element, HtmlElement, ShadowRoot, ShadowRootInit, ShadowRootMode};

/// ID for the Shadow DOM host element
const SHADOW_HOST_ID: &str = "nevoflux-content-sidebar-host";

/// Create Shadow DOM host element and return the shadow root
pub fn create_shadow_host() -> Result<ShadowRoot, JsValue> {
    let window = web_sys::window().ok_or("No window found")?;
    let document = window.document().ok_or("No document found")?;

    // Check if host already exists
    if let Some(existing) = document.get_element_by_id(SHADOW_HOST_ID) {
        // Return existing shadow root
        if let Some(shadow) = existing.shadow_root() {
            return Ok(shadow);
        }
        // Remove broken host and recreate
        if let Some(parent) = existing.parent_element() {
            parent.remove_child(&existing)?;
        }
    }

    // Create host element
    let host = document.create_element("div")?;
    host.set_id(SHADOW_HOST_ID);

    // Apply host styles - fixed position at right edge
    let host_html: &HtmlElement = host.dyn_ref().ok_or("Failed to cast to HtmlElement")?;
    let style = host_html.style();

    style.set_property("position", "fixed")?;
    style.set_property("top", "0")?;
    style.set_property("right", "0")?;
    style.set_property("width", "320px")?;
    style.set_property("height", "100vh")?;
    style.set_property("z-index", "2147483647")?; // Max z-index
    style.set_property("pointer-events", "auto")?;
    style.set_property("font-family", "system-ui, -apple-system, sans-serif")?;

    // Attach Shadow DOM with closed mode for better isolation
    let shadow_init = ShadowRootInit::new(ShadowRootMode::Closed);
    let shadow_root = host.attach_shadow(&shadow_init)?;

    // Insert host into document body
    let body = document.body().ok_or("No body found")?;
    body.append_child(&host)?;

    tracing::info!("Shadow DOM host created successfully");

    Ok(shadow_root)
}

/// Inject CSS styles into Shadow DOM
pub fn inject_styles(shadow_root: &ShadowRoot, css: &str) -> Result<(), JsValue> {
    let document = shadow_root.owner_document().ok_or("No owner document")?;

    let style_element = document.create_element("style")?;
    style_element.set_text_content(Some(css));

    shadow_root.append_child(&style_element)?;

    tracing::debug!("Styles injected into Shadow DOM");

    Ok(())
}

/// Create mount point for Dioxus within Shadow DOM
pub fn create_mount_point(shadow_root: &ShadowRoot) -> Result<Element, JsValue> {
    let document = shadow_root.owner_document().ok_or("No owner document")?;

    let mount_point = document.create_element("div")?;
    mount_point.set_id("dioxus-mount");
    mount_point.set_class_name("dioxus-root");

    shadow_root.append_child(&mount_point)?;

    tracing::debug!("Dioxus mount point created");

    Ok(mount_point)
}

/// Remove Shadow DOM host from document
pub fn remove_shadow_host() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or("No window found")?;
    let document = window.document().ok_or("No document found")?;

    if let Some(host) = document.get_element_by_id(SHADOW_HOST_ID) {
        if let Some(parent) = host.parent_element() {
            parent.remove_child(&host)?;
            tracing::info!("Shadow DOM host removed");
        }
    }

    Ok(())
}

/// Check if Shadow DOM host exists
pub fn shadow_host_exists() -> bool {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(SHADOW_HOST_ID))
        .is_some()
}

/// Get reference to existing Shadow DOM host
pub fn get_shadow_host() -> Option<Element> {
    web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(SHADOW_HOST_ID))
}

/// Toggle visibility of the Content Sidebar
pub fn toggle_visibility(visible: bool) -> Result<(), JsValue> {
    if let Some(host) = get_shadow_host() {
        let host_html: &HtmlElement = host.dyn_ref().ok_or("Failed to cast")?;
        let style = host_html.style();
        style.set_property("display", if visible { "block" } else { "none" })?;
    }
    Ok(())
}

/// Set sidebar width
pub fn set_width(width_px: u32) -> Result<(), JsValue> {
    if let Some(host) = get_shadow_host() {
        let host_html: &HtmlElement = host.dyn_ref().ok_or("Failed to cast")?;
        let style = host_html.style();
        style.set_property("width", &format!("{}px", width_px))?;
    }
    Ok(())
}

/// Set sidebar position (left or right edge)
pub fn set_position(on_left: bool) -> Result<(), JsValue> {
    if let Some(host) = get_shadow_host() {
        let host_html: &HtmlElement = host.dyn_ref().ok_or("Failed to cast")?;
        let style = host_html.style();

        if on_left {
            style.set_property("left", "0")?;
            style.set_property("right", "auto")?;
        } else {
            style.set_property("left", "auto")?;
            style.set_property("right", "0")?;
        }
    }
    Ok(())
}
