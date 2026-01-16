/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

//! Page Context Extraction for Content Sidebar
//!
//! Extracts interactive elements and page information for the
//! AutoPageContext structure used in Computer Use.

use shared_protocol::{AutoPageContext, BoundingBox, InteractiveElement, Viewport};
use wasm_bindgen::JsCast;
use web_sys::{Document, Element, HtmlElement, Window};

/// Interactive element selectors to search for
const INTERACTIVE_SELECTORS: &str =
    "a, button, input, select, textarea, [role='button'], [onclick], [href], \
     [type='submit'], [type='button'], [role='link'], [role='menuitem'], \
     [role='tab'], [role='checkbox'], [role='radio']";

/// Maximum number of interactive elements to extract
const MAX_INTERACTIVE_ELEMENTS: usize = 50;

/// Maximum length of text content to include
const MAX_TEXT_CONTENT: usize = 2000;

/// Extract page context for Computer Use
pub fn extract_page_context() -> AutoPageContext {
    let (window, document) = match get_window_document() {
        Some(wd) => wd,
        None => return AutoPageContext::default(),
    };

    let url = window.location().href().unwrap_or_default();
    let title = document.title();
    let viewport = extract_viewport(&window, &document);
    let interactive_elements = extract_interactive_elements(&document, &viewport);
    let text_content = extract_text_content(&document);

    AutoPageContext {
        url,
        title,
        viewport,
        interactive_elements,
        text_content,
    }
}

/// Get window and document
fn get_window_document() -> Option<(Window, Document)> {
    let window = web_sys::window()?;
    let document = window.document()?;
    Some((window, document))
}

/// Extract viewport information
fn extract_viewport(window: &Window, document: &Document) -> Viewport {
    let width = window.inner_width().ok().and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
    let height = window.inner_height().ok().and_then(|v| v.as_f64()).unwrap_or(0.0) as u32;
    let scroll_x = window.scroll_x().unwrap_or(0.0) as i32;
    let scroll_y = window.scroll_y().unwrap_or(0.0) as i32;

    let scroll_height = document
        .document_element()
        .map(|el| el.scroll_height() as u32)
        .unwrap_or(0);

    Viewport {
        width,
        height,
        scroll_x,
        scroll_y,
        scroll_height,
    }
}

/// Extract interactive elements from the page
fn extract_interactive_elements(document: &Document, viewport: &Viewport) -> Vec<InteractiveElement> {
    let mut elements = Vec::new();

    let node_list = match document.query_selector_all(INTERACTIVE_SELECTORS) {
        Ok(nl) => nl,
        Err(_) => return elements,
    };

    for i in 0..node_list.length() {
        if elements.len() >= MAX_INTERACTIVE_ELEMENTS {
            break;
        }

        let node = match node_list.get(i) {
            Some(n) => n,
            None => continue,
        };

        let element = match node.dyn_ref::<Element>() {
            Some(e) => e,
            None => continue,
        };

        // Check if element is visible and in viewport
        let html_element = element.dyn_ref::<HtmlElement>();
        let bounding_box = get_bounding_box(element);

        // Skip if not visible or outside viewport
        if !is_visible(element, html_element) {
            continue;
        }

        if !is_in_viewport(&bounding_box, viewport) {
            continue;
        }

        // Generate unique selector
        let selector = generate_unique_selector(element, document);
        if selector.is_empty() {
            continue;
        }

        // Extract element information
        let tag = element.tag_name().to_lowercase();
        let element_type = get_element_type(element);
        let text = get_element_text(element);
        let placeholder = get_placeholder(element);
        let role = element.get_attribute("role");
        let name = element.get_attribute("name");
        let id = if element.id().is_empty() { None } else { Some(element.id()) };
        let class_list = get_class_list(element);
        let is_enabled = is_element_enabled(element);

        elements.push(InteractiveElement {
            selector,
            tag,
            element_type,
            text,
            placeholder,
            role,
            name,
            id,
            class_list,
            bounding_box,
            is_visible: true,
            is_enabled,
        });
    }

    elements
}

/// Get bounding box of element
fn get_bounding_box(element: &Element) -> BoundingBox {
    let rect = element.get_bounding_client_rect();
    BoundingBox {
        x: rect.x(),
        y: rect.y(),
        width: rect.width(),
        height: rect.height(),
    }
}

/// Check if element is visible
fn is_visible(element: &Element, html_element: Option<&HtmlElement>) -> bool {
    // Check basic visibility
    let rect = element.get_bounding_client_rect();
    if rect.width() == 0.0 || rect.height() == 0.0 {
        return false;
    }

    // Check computed style if possible
    if let Some(html_el) = html_element {
        if let Some(window) = web_sys::window() {
            if let Ok(Some(style)) = window.get_computed_style(html_el) {
                if let Ok(display) = style.get_property_value("display") {
                    if display == "none" {
                        return false;
                    }
                }
                if let Ok(visibility) = style.get_property_value("visibility") {
                    if visibility == "hidden" {
                        return false;
                    }
                }
                if let Ok(opacity) = style.get_property_value("opacity") {
                    if opacity.parse::<f32>().unwrap_or(1.0) < 0.1 {
                        return false;
                    }
                }
            }
        }
    }

    true
}

/// Check if bounding box is in viewport
fn is_in_viewport(bbox: &BoundingBox, viewport: &Viewport) -> bool {
    let vw = viewport.width as f64;
    let vh = viewport.height as f64;

    // Check if any part of the element is in viewport
    bbox.x < vw && bbox.y < vh && bbox.x + bbox.width > 0.0 && bbox.y + bbox.height > 0.0
}

/// Generate unique CSS selector for element
fn generate_unique_selector(element: &Element, document: &Document) -> String {
    // Priority 1: ID
    let id = element.id();
    if !id.is_empty() {
        let selector = format!("#{}", css_escape(&id));
        if is_unique_selector(&selector, element, document) {
            return selector;
        }
    }

    // Priority 2: data-testid or data-test
    for attr in &["data-testid", "data-test", "data-cy"] {
        if let Some(value) = element.get_attribute(attr) {
            let selector = format!("[{}='{}']", attr, css_escape(&value));
            if is_unique_selector(&selector, element, document) {
                return selector;
            }
        }
    }

    // Priority 3: name attribute (for form elements)
    if let Some(name) = element.get_attribute("name") {
        let tag = element.tag_name().to_lowercase();
        let selector = format!("{}[name='{}']", tag, css_escape(&name));
        if is_unique_selector(&selector, element, document) {
            return selector;
        }
    }

    // Priority 4: Class combination
    let class_name = element.class_name();
    if !class_name.is_empty() {
        let tag = element.tag_name().to_lowercase();
        let classes: Vec<&str> = class_name.split_whitespace().collect();
        if let Some(class) = classes.first() {
            let selector = format!("{}.{}", tag, css_escape(class));
            if is_unique_selector(&selector, element, document) {
                return selector;
            }
        }
    }

    // Priority 5: nth-child path (fallback)
    generate_nth_child_selector(element, document)
}

/// CSS escape for selector values
fn css_escape(value: &str) -> String {
    // Simple escape: replace special chars
    value
        .chars()
        .map(|c| match c {
            '\\' | '"' | '\'' | '[' | ']' | '(' | ')' | '=' | ':' | '.' | '#' | '>' | '+' | '~' | ' ' => {
                format!("\\{}", c)
            }
            _ => c.to_string(),
        })
        .collect()
}

/// Check if selector uniquely identifies element
fn is_unique_selector(selector: &str, element: &Element, document: &Document) -> bool {
    match document.query_selector(selector) {
        Ok(Some(found)) => {
            // Check if it's the same element
            found.is_same_node(Some(element))
        }
        _ => false,
    }
}

/// Generate nth-child based selector
fn generate_nth_child_selector(element: &Element, document: &Document) -> String {
    let mut parts = Vec::new();
    let mut current: Option<Element> = Some(element.clone());
    let mut depth = 0;

    while let Some(el) = current {
        if depth > 5 {
            break; // Limit depth
        }

        let tag = el.tag_name().to_lowercase();
        if tag == "html" || tag == "body" {
            parts.push(tag);
            break;
        }

        // Get nth-child position
        if let Some(parent) = el.parent_element() {
            let children = parent.children();
            let mut index = 1;
            for i in 0..children.length() {
                if let Some(child) = children.item(i) {
                    // Cast to Node for comparison
                    let child_node: &web_sys::Node = child.as_ref();
                    let el_node: &web_sys::Node = el.as_ref();
                    if child_node.is_same_node(Some(el_node)) {
                        break;
                    }
                    if child.tag_name() == el.tag_name() {
                        index += 1;
                    }
                }
            }
            parts.push(format!("{}:nth-of-type({})", tag, index));
        } else {
            parts.push(tag);
        }

        current = el.parent_element();
        depth += 1;
    }

    parts.reverse();
    parts.join(" > ")
}

/// Get element type (for input elements)
fn get_element_type(element: &Element) -> Option<String> {
    element.get_attribute("type")
}

/// Get visible text of element
fn get_element_text(element: &Element) -> Option<String> {
    let text = element.text_content()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        // Try aria-label
        element.get_attribute("aria-label")
    } else {
        Some(trimmed.chars().take(100).collect())
    }
}

/// Get placeholder text
fn get_placeholder(element: &Element) -> Option<String> {
    element.get_attribute("placeholder")
}

/// Get class list
fn get_class_list(element: &Element) -> Vec<String> {
    element
        .class_name()
        .split_whitespace()
        .map(|s| s.to_string())
        .collect()
}

/// Check if element is enabled
fn is_element_enabled(element: &Element) -> bool {
    // Check disabled attribute
    if element.has_attribute("disabled") {
        return false;
    }

    // Check aria-disabled
    if element.get_attribute("aria-disabled").as_deref() == Some("true") {
        return false;
    }

    true
}

/// Extract main text content from page
fn extract_text_content(document: &Document) -> Option<String> {
    let body = document.body()?;

    // Get text content
    let mut text = body.text_content().unwrap_or_default();

    // Clean up: remove excessive whitespace
    text = text
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect::<Vec<_>>()
        .join("\n");

    // Truncate if too long
    if text.len() > MAX_TEXT_CONTENT {
        text = text.chars().take(MAX_TEXT_CONTENT).collect();
        text.push_str("...");
    }

    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}
