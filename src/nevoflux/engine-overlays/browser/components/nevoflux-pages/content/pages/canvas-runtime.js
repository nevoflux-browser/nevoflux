/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

'use strict';

/**
 * CanvasRuntime -- orchestrates the full multi-file project lifecycle:
 * files -> VirtualFS -> Bundler -> HTML generation -> iframe rendering.
 *
 * Sits between the Bundler and canvas.js, managing entry point detection,
 * mount wrapping for pure component exports, HTML document generation with
 * importmaps, and sandboxed iframe rendering.
 *
 * Assumes `VirtualFS` and `Bundler` are available as globals.
 */
const _CanvasRuntime = {
  /** @type {string|null} The resolved entry point path. */
  _entry: null,

  /** @type {string|null} The wrapper entry path if mount wrapping was applied. */
  _wrappedEntry: null,

  /** @type {HTMLIFrameElement|null} The current render iframe. */
  _iframe: null,

  // ── Entry Point Detection ──────────────────────────────

  /**
   * Entry point candidate paths, searched in order.
   * @type {string[]}
   */
  _ENTRY_CANDIDATES: [
    '/src/index.tsx',
    '/src/index.ts',
    '/src/index.jsx',
    '/src/index.js',
    '/src/main.tsx',
    '/src/main.ts',
    '/src/main.jsx',
    '/src/main.js',
    '/src/App.tsx',
    '/src/App.ts',
    '/src/App.jsx',
    '/src/App.js',
    '/index.tsx',
    '/index.ts',
    '/index.jsx',
    '/index.js',
    '/App.tsx',
    '/App.ts',
    '/App.jsx',
    '/App.js',
    // Vue entry points
    '/src/main.vue',
    '/src/App.vue',
    '/App.vue',
    // Svelte entry points
    '/src/main.svelte',
    '/src/App.svelte',
    '/App.svelte',
    // HTML entry points (served directly, no bundling)
    '/index.html',
    '/src/index.html',
  ],

  /**
   * Detect the entry point for a multi-file project in VirtualFS.
   *
   * Resolution order:
   * 1. Explicit entry (normalized, with extension resolution)
   * 2. Well-known candidate paths (searched in order)
   * 3. Fallback: first .tsx/.jsx/.ts/.js file found in VFS
   *
   * @param {string} [explicitEntry] - An explicit entry point path.
   * @returns {string|null} The resolved entry path, or null if no entry found.
   */
  detectEntry(explicitEntry) {
    // 1. Explicit entry provided
    if (explicitEntry) {
      const normalized = VirtualFS.normalize(explicitEntry);
      const resolved = VirtualFS.resolveWithExtension(normalized);
      if (resolved) {
        return resolved;
      }
      // If the explicit path exists as-is in VFS, use it
      if (VirtualFS.read(normalized) !== null) {
        return normalized;
      }
    }

    // 2. Well-known candidate paths
    for (const candidate of this._ENTRY_CANDIDATES) {
      if (VirtualFS.read(candidate) !== null) {
        return candidate;
      }
    }

    // 3. Fallback: first .tsx/.jsx/.ts/.js file in VFS
    const allFiles = VirtualFS.list();
    const jsExtensions = ['.tsx', '.jsx', '.ts', '.js', '.vue', '.svelte', '.html'];
    for (const ext of jsExtensions) {
      const match = allFiles.find((f) => f.endsWith(ext));
      if (match) {
        return match;
      }
    }

    return null;
  },

  // ── Mount Wrapper Detection ────────────────────────────

  /**
   * Determine whether an entry file's content needs a mount wrapper.
   *
   * Returns `false` if the content already contains its own mounting logic
   * (createRoot, ReactDOM.render, createApp, new App, document.getElementById).
   *
   * Returns `true` if the content appears to be a pure component export
   * (export default function/class/const/let/var or export default [A-Z]).
   *
   * @param {string} content - The file content to analyze.
   * @returns {boolean} True if the content needs a mount wrapper.
   */
  _needsMountWrapper(content) {
    // Already has mounting logic -- no wrapper needed
    if (
      content.includes('createRoot') ||
      content.includes('ReactDOM.render') ||
      content.includes('createApp') ||
      content.includes('document.getElementById') ||
      content.includes('document.querySelector')
    ) {
      return false;
    }

    // Pure component export patterns that need wrapping
    if (
      /export\s+default\s+(?:function|class|const|let|var)\b/.test(content) ||
      /export\s+default\s+[A-Z]/.test(content)
    ) {
      return true;
    }

    return false;
  },

  // ── Mount Wrapping ─────────────────────────────────────

  /**
   * Detect the framework for a file based on its extension.
   *
   * @param {string} filePath - The file path to check.
   * @returns {"react"|"vue"|"svelte"} The detected framework.
   */
  _detectFramework(filePath) {
    if (filePath.endsWith('.vue')) {
      return 'vue';
    }
    if (filePath.endsWith('.svelte')) {
      return 'svelte';
    }
    return 'react';
  },

  /**
   * Create a wrapper entry file that mounts a pure component export.
   *
   * Generates framework-appropriate mount code:
   * - **React**: Uses `createRoot` from `react-dom/client`
   * - **Vue**: Uses `createApp` from `vue`
   * - **Svelte**: Instantiates component with `new App({ target })`
   *
   * Writes the wrapper file at `/__nevoflux_entry_wrapper.{ext}` in VirtualFS.
   * Sets `this._wrappedEntry` to the wrapper path so the bundler uses
   * it as the actual entry point.
   *
   * @param {string} entryPath - The original entry point path in VFS.
   */
  _wrapEntryWithMount(entryPath) {
    const framework = this._detectFramework(entryPath);

    let wrapperPath;
    let wrapperCode;

    switch (framework) {
      case 'vue': {
        wrapperPath = '/__nevoflux_entry_wrapper.js';
        wrapperCode = [
          'import { createApp } from "vue";',
          `import App from "${entryPath}";`,
          '',
          'createApp(App).mount("#root");',
          '',
        ].join('\n');
        break;
      }

      case 'svelte': {
        wrapperPath = '/__nevoflux_entry_wrapper.js';
        wrapperCode = [
          `import App from "${entryPath}";`,
          '',
          'new App({',
          '  target: document.getElementById("root"),',
          '});',
          '',
        ].join('\n');
        break;
      }

      default: {
        // React
        wrapperPath = '/__nevoflux_entry_wrapper.jsx';
        wrapperCode = [
          'import { createRoot } from "react-dom/client";',
          `import App from "${entryPath}";`,
          '',
          'const root = createRoot(document.getElementById("root"));',
          'root.render(<App />);',
          '',
        ].join('\n');
        break;
      }
    }

    VirtualFS.write(wrapperPath, wrapperCode);
    this._wrappedEntry = wrapperPath;
  },

  // ── HTML Generation ────────────────────────────────────

  /**
   * Generate a complete HTML document for the bundled output.
   *
   * Includes:
   * - Meta charset and viewport tags
   * - Default box-sizing and body/root styles
   * - Collected CSS in a `<style>` tag
   * - SDK script in `<head>` for artifact-agent communication
   * - Importmap from `Bundler.IMPORTMAP_PACKAGES` for bare specifier resolution
   * - Bundled JS in a `<script type="module">`
   * - A `<div id="root">` mount point
   *
   * @param {string} js - The bundled JavaScript output.
   * @param {string} css - The collected CSS string.
   * @param {string} sdkScript - The NevofluxSDK injection script (HTML string).
   * @returns {string} A complete HTML document string.
   */
  generateHTML(js, css, sdkScript) {
    // Build importmap from Bundler's package mapping
    const importmapObj = { imports: {} };
    if (Bundler.IMPORTMAP_PACKAGES) {
      for (const [pkg, url] of Object.entries(Bundler.IMPORTMAP_PACKAGES)) {
        importmapObj.imports[pkg] = url;
      }
    }
    const importmapJSON = JSON.stringify(importmapObj, null, 2);

    // Escape closing script tags inside JS to prevent premature HTML parsing
    const safeJS = js.replace(/<\/script>/gi, '<\\/script>');

    const parts = [
      '<!DOCTYPE html>',
      '<html>',
      '<head>',
      '<meta charset="utf-8">',
      '<meta name="viewport" content="width=device-width, initial-scale=1">',
      '<style>',
      '*, *::before, *::after { box-sizing: border-box; }',
      'body { margin: 0; }',
      '#root { min-height: 100vh; }',
      '</style>',
    ];

    // Inject collected CSS
    if (css) {
      parts.push('<style>', css, '</style>');
    }

    // Inject SDK script
    if (sdkScript) {
      parts.push(sdkScript);
    }

    // Inject importmap
    parts.push('<script type="importmap">', importmapJSON, '</script>');

    parts.push('</head>');
    parts.push('<body>');
    parts.push('<div id="root"></div>');

    // Inject bundled JS as ES module
    parts.push('<script type="module">', safeJS, '</script>');

    parts.push('</body>');
    parts.push('</html>');

    return parts.join('\n');
  },

  // ── Main Render ────────────────────────────────────────

  /**
   * Render a multi-file project into a viewport element.
   *
   * Performs the full lifecycle:
   * 1. Clears VirtualFS and loads project files
   * 2. Detects entry point
   * 3. Wraps entry with mount code if needed
   * 4. Bundles via esbuild-wasm
   * 5. Generates HTML document
   * 6. Creates sandboxed iframe with srcdoc
   *
   * @param {HTMLElement} viewport - The container element to render into.
   * @param {object} project - The project descriptor.
   * @param {Record<string, string>} project.files - Map of file paths to contents.
   * @param {string} [project.entry] - Optional explicit entry point.
   * @param {Record<string, string>} [project.env] - Optional environment variables.
   * @param {string} sdkScript - The NevofluxSDK injection script (HTML string).
   * @returns {Promise<{ success: boolean, error?: string }>}
   */
  async render(viewport, project, sdkScript) {
    try {
      // 1. Clear VFS and load project files
      VirtualFS.clear();
      VirtualFS.loadBatch(project.files);

      // 2. Detect entry point
      this._wrappedEntry = null;
      const entry = this.detectEntry(project.entry);
      if (!entry) {
        return {
          success: false,
          error: 'No entry point found. Add an index.tsx, index.js, App.tsx, or App.js file.',
        };
      }
      this._entry = entry;

      // 3. HTML entry: serve directly with inlined VFS resources (no bundling)
      if (entry.endsWith('.html') || entry.endsWith('.htm')) {
        const html = this._assembleHTMLEntry(entry, sdkScript);
        this._createIframe(viewport, html);
        return { success: true };
      }

      // 4. Check if entry needs mount wrapper
      //    .vue and .svelte files are always component definitions and need wrapping.
      //    JS/TS files are checked for existing mount logic.
      const entryContent = VirtualFS.read(entry);
      let bundleEntry = entry;
      const isComponentFile = entry.endsWith('.vue') || entry.endsWith('.svelte');
      if (isComponentFile || (entryContent && this._needsMountWrapper(entryContent))) {
        this._wrapEntryWithMount(entry);
        bundleEntry = this._wrappedEntry;
      }

      // 5. Bundle
      const result = await Bundler.bundle({
        entry: bundleEntry,
        env: project.env || {},
      });

      if (result.errors && result.errors.length > 0) {
        return {
          success: false,
          error: result.errors.join('\n'),
        };
      }

      // 6. Generate HTML
      const html = this.generateHTML(result.js, result.css, sdkScript);

      // 7. Create sandboxed iframe
      this._createIframe(viewport, html);

      return { success: true };
    } catch (e) {
      console.error('[CanvasRuntime] render failed:', e);
      return {
        success: false,
        error: e.message || String(e),
      };
    }
  },

  // ── HTML Entry Assembly ──────────────────────────────────

  /**
   * Assemble an HTML entry file by inlining local VFS resources.
   *
   * Replaces `<script src="path">` and `<link rel="stylesheet" href="path">`
   * references with inline content from VirtualFS, so the HTML is
   * self-contained for srcdoc rendering.
   *
   * @param {string} entryPath - The HTML entry path in VFS.
   * @param {string} sdkScript - The NevofluxSDK injection script.
   * @returns {string} Self-contained HTML string.
   */
  _assembleHTMLEntry(entryPath, sdkScript) {
    let html = VirtualFS.read(entryPath) || '';
    const _entryDir = entryPath.substring(0, entryPath.lastIndexOf('/')) || '/';

    // Inline <script src="local-path"></script> from VFS
    html = html.replace(
      /<script\s+([^>]*?)src\s*=\s*["']([^"']+)["']([^>]*)>\s*<\/script>/gi,
      (_match, pre, src, post) => {
        // Skip external URLs
        if (/^https?:\/\//.test(src)) return _match;
        const resolved = VirtualFS.resolve(entryPath, src);
        const withExt = VirtualFS.resolveWithExtension(resolved) || resolved;
        const content = VirtualFS.read(withExt);
        if (content !== null) {
          // Preserve type attribute if present
          const typeMatch = (pre + post).match(/type\s*=\s*["']([^"']+)["']/);
          const typeAttr = typeMatch ? ` type="${typeMatch[1]}"` : '';
          return `<script${typeAttr}>\n${content}\n</script>`;
        }
        return _match; // Keep original if file not found
      }
    );

    // Inline <link rel="stylesheet" href="local-path"> from VFS
    html = html.replace(
      /<link\s+[^>]*rel\s*=\s*["']stylesheet["'][^>]*href\s*=\s*["']([^"']+)["'][^>]*\/?>/gi,
      (_match, href) => {
        if (/^https?:\/\//.test(href)) return _match;
        const resolved = VirtualFS.resolve(entryPath, href);
        const withExt = VirtualFS.resolveWithExtension(resolved) || resolved;
        const content = VirtualFS.read(withExt);
        if (content !== null) {
          return `<style>\n${content}\n</style>`;
        }
        return _match;
      }
    );

    // Also match href before rel (some HTML has href first)
    html = html.replace(
      /<link\s+[^>]*href\s*=\s*["']([^"']+)["'][^>]*rel\s*=\s*["']stylesheet["'][^>]*\/?>/gi,
      (_match, href) => {
        if (/^https?:\/\//.test(href)) return _match;
        // Check if already inlined (this regex won't match <style> tags)
        const resolved = VirtualFS.resolve(entryPath, href);
        const withExt = VirtualFS.resolveWithExtension(resolved) || resolved;
        const content = VirtualFS.read(withExt);
        if (content !== null) {
          return `<style>\n${content}\n</style>`;
        }
        return _match;
      }
    );

    // Inject SDK script before </head> or at top
    if (sdkScript) {
      const headIdx = html.indexOf('</head>');
      if (headIdx !== -1) {
        html = html.slice(0, headIdx) + sdkScript + html.slice(headIdx);
      } else {
        const bodyIdx = html.indexOf('<body');
        if (bodyIdx !== -1) {
          html = html.slice(0, bodyIdx) + sdkScript + html.slice(bodyIdx);
        } else {
          html = sdkScript + html;
        }
      }
    }

    return html;
  },

  // ── Iframe Helper ───────────────────────────────────────

  /**
   * Create (or replace) a sandboxed iframe with the given srcdoc.
   *
   * @param {HTMLElement} viewport - The container element.
   * @param {string} html - The HTML content for srcdoc.
   */
  _createIframe(viewport, html) {
    if (this._iframe) {
      this._iframe.remove();
      this._iframe = null;
    }
    this._iframe = document.createElement('iframe');
    this._iframe.setAttribute('sandbox', 'allow-scripts allow-forms allow-same-origin');
    this._iframe.srcdoc = html;
    viewport.appendChild(this._iframe);
  },

  // ── Hot Update ─────────────────────────────────────────

  /**
   * Update a single file and re-render.
   *
   * Writes the updated file to VirtualFS, re-bundles using the existing
   * entry point, and updates the iframe srcdoc.
   *
   * @param {HTMLElement} viewport - The container element with the iframe.
   * @param {string} path - The file path to update.
   * @param {string} content - The new file content.
   * @param {string} sdkScript - The NevofluxSDK injection script (HTML string).
   * @returns {Promise<{ success: boolean, error?: string }>}
   */
  async updateFile(viewport, path, content, sdkScript) {
    try {
      // Write updated file to VFS
      VirtualFS.write(path, content);

      // Determine bundle entry: use wrapped entry if it exists, otherwise original
      const bundleEntry = this._wrappedEntry || this._entry;
      if (!bundleEntry) {
        return {
          success: false,
          error: 'No entry point set. Call render() first.',
        };
      }

      // If the updated file is the entry, re-check if mount wrapper is needed.
      // .vue and .svelte files always need wrapping (they are component definitions).
      const normalizedPath = VirtualFS.normalize(path);
      if (normalizedPath === this._entry) {
        const isComponentFile = this._entry.endsWith('.vue') || this._entry.endsWith('.svelte');
        const updatedContent = VirtualFS.read(normalizedPath);
        if (isComponentFile || (updatedContent && this._needsMountWrapper(updatedContent))) {
          if (!this._wrappedEntry) {
            this._wrapEntryWithMount(this._entry);
          }
        } else {
          // Entry now has its own mount logic; remove wrapper
          if (this._wrappedEntry) {
            VirtualFS.delete(this._wrappedEntry);
            this._wrappedEntry = null;
          }
        }
      }

      // Re-bundle
      const effectiveEntry = this._wrappedEntry || this._entry;
      const result = await Bundler.bundle({
        entry: effectiveEntry,
        env: {},
      });

      if (result.errors && result.errors.length > 0) {
        return {
          success: false,
          error: result.errors.join('\n'),
        };
      }

      // Generate updated HTML
      const html = this.generateHTML(result.js, result.css, sdkScript);

      // Update existing iframe or create new one
      if (this._iframe && this._iframe.parentNode) {
        this._iframe.srcdoc = html;
      } else {
        this._iframe = document.createElement('iframe');
        this._iframe.setAttribute('sandbox', 'allow-scripts allow-forms allow-same-origin');
        this._iframe.srcdoc = html;
        viewport.appendChild(this._iframe);
      }

      return { success: true };
    } catch (e) {
      console.error('[CanvasRuntime] updateFile failed:', e);
      return {
        success: false,
        error: e.message || String(e),
      };
    }
  },
};
