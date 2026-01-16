/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

"use strict";

/**
 * NevoFlux Agent Content Script
 * Injected into web pages to extract content and perform actions
 */

/**
 * Extract page content for the agent
 * @returns {Object} Extracted page data
 */
function extractPageContent() {
  return {
    title: document.title,
    url: window.location.href,
    text: document.body.innerText,
    html: document.documentElement.outerHTML,
    links: Array.from(document.querySelectorAll("a")).map((a) => ({
      text: a.textContent.trim(),
      href: a.href
    })),
    images: Array.from(document.querySelectorAll("img")).map((img) => ({
      alt: img.alt,
      src: img.src
    })),
    forms: Array.from(document.querySelectorAll("form")).map((form) => ({
      action: form.action,
      method: form.method,
      fields: Array.from(form.querySelectorAll("input, textarea, select")).map((field) => ({
        name: field.name,
        type: field.type,
        id: field.id
      }))
    }))
  };
}

/**
 * Click an element by selector
 * @param {string} selector - CSS selector
 * @returns {Object} Result of click action
 */
function clickElement(selector) {
  try {
    const element = document.querySelector(selector);
    if (!element) {
      return { success: false, error: `Element not found: ${selector}` };
    }

    element.click();
    return { success: true, message: `Clicked element: ${selector}` };
  } catch (error) {
    return { success: false, error: error.message };
  }
}

/**
 * Fill form fields
 * @param {Array} fields - Array of {selector, value} objects
 * @returns {Object} Result of fill action
 */
function fillForm(fields) {
  try {
    const results = [];

    for (const field of fields) {
      const element = document.querySelector(field.selector);
      if (!element) {
        results.push({
          selector: field.selector,
          success: false,
          error: "Element not found"
        });
        continue;
      }

      element.value = field.value;
      element.dispatchEvent(new Event("input", { bubbles: true }));
      element.dispatchEvent(new Event("change", { bubbles: true }));

      results.push({
        selector: field.selector,
        success: true
      });
    }

    return { success: true, results };
  } catch (error) {
    return { success: false, error: error.message };
  }
}

/**
 * Listen for messages from background script
 */
browser.runtime.onMessage.addListener((message, sender, sendResponse) => {
  console.log("Content script received message:", message);

  switch (message.type) {
    case "extract_content":
      const content = extractPageContent();
      sendResponse(content);
      break;

    case "click_element":
      const clickResult = clickElement(message.selector);
      sendResponse(clickResult);
      break;

    case "fill_form":
      const fillResult = fillForm(message.fields);
      sendResponse(fillResult);
      break;

    default:
      console.warn("Unknown message type:", message.type);
      sendResponse({ error: "Unknown message type" });
  }
});

console.log("NevoFlux Agent content script loaded");
