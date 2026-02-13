/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

"use strict";

/**
 * Bundler -- esbuild-wasm based in-browser bundler for Canvas multi-file projects.
 *
 * Uses esbuild-wasm (loaded via `<script>` tag, available as `globalThis.esbuild`)
 * with custom plugins to bundle multi-file projects stored in VirtualFS.
 *
 * Plugins:
 * - VFS Plugin: resolves relative/absolute imports from VirtualFS
 * - CDN Plugin: marks bare npm specifiers as external (resolved via importmap)
 * - CSS Plugin: collects CSS from .css imports into a combined string
 *
 * Assumes `globalThis.esbuild` and `VirtualFS` are available as globals.
 */
const Bundler = {
  /** @type {boolean} Whether esbuild has been initialized. */
  _initialized: false,

  /** @type {boolean} Whether initialization is currently in progress. */
  _initializing: false,

  /** @type {string[]} CSS chunks collected during a build. */
  _collectedCSS: [],

  /** @type {Map<string, {js: string, css: string, errors: string[]}>} */
  _bundleCache: new Map(),

  /**
   * Framework packages mapped to esm.sh CDN URLs for runtime importmap.
   * @type {Record<string, string>}
   */
  IMPORTMAP_PACKAGES: {
    "react": "https://esm.sh/react@18?dev",
    "react-dom": "https://esm.sh/react-dom@18?dev",
    "react-dom/client": "https://esm.sh/react-dom@18/client?dev",
    "react/jsx-runtime": "https://esm.sh/react@18/jsx-runtime?dev",
    "react/jsx-dev-runtime": "https://esm.sh/react@18/jsx-dev-runtime?dev",
  },

  // ── Initialization ──────────────────────────────────────

  /**
   * Initialize esbuild-wasm with the WASM binary.
   *
   * Idempotent: subsequent calls are no-ops once initialization completes.
   * Concurrent calls during initialization will wait for the first to finish.
   *
   * @returns {Promise<void>}
   * @throws {Error} If esbuild global is not available or WASM init fails.
   */
  async init() {
    if (this._initialized) {
      return;
    }

    // Guard against concurrent initialization calls
    if (this._initializing) {
      // Wait for the in-flight initialization to complete
      while (this._initializing) {
        await new Promise((resolve) => setTimeout(resolve, 50));
      }
      return;
    }

    this._initializing = true;

    try {
      if (typeof esbuild === "undefined") {
        throw new Error("Bundler: esbuild global not found. Ensure esbuild-wasm.min.js is loaded via <script> tag.");
      }

      await esbuild.initialize({
        wasmURL: "chrome://nevoflux/content/nevoflux/vendor/esbuild.wasm",
      });

      this._initialized = true;
      console.info("[Bundler] esbuild-wasm initialized successfully.");
    } catch (e) {
      console.error("[Bundler] Failed to initialize esbuild-wasm:", e);
      throw e;
    } finally {
      this._initializing = false;
    }
  },

  // ── Plugins ─────────────────────────────────────────────

  /**
   * Create the VFS plugin for esbuild.
   *
   * Resolves relative (`./`, `../`), absolute (`/`), and alias (`@/`) imports
   * from VirtualFS. Uses VirtualFS.resolve() for path resolution and
   * VirtualFS.resolveWithExtension() for extension auto-completion.
   *
   * @returns {object} An esbuild plugin object.
   */
  _createVFSPlugin() {
    return {
      name: "vfs",
      setup(build) {
        // Resolve relative, absolute, and @/ imports
        build.onResolve({ filter: /^[\.\/]|^@\// }, (args) => {
          const importer = args.importer || "/";
          const resolved = VirtualFS.resolve(importer, args.path);
          const withExt = VirtualFS.resolveWithExtension(resolved);

          if (withExt) {
            return { path: withExt, namespace: "vfs" };
          }

          return {
            errors: [{
              text: `File not found: ${args.path} (resolved to ${resolved})`,
            }],
          };
        });

        // Load files from VirtualFS
        build.onLoad({ filter: /.*/, namespace: "vfs" }, (args) => {
          const contents = VirtualFS.read(args.path);

          if (contents === null) {
            return {
              errors: [{
                text: `File not found in VirtualFS: ${args.path}`,
              }],
            };
          }

          const loader = Bundler._getLoader(args.path);
          return { contents, loader };
        });
      },
    };
  },

  /**
   * Create the CDN plugin for esbuild.
   *
   * Handles bare npm specifiers (e.g., `react`, `@tanstack/query`).
   * Skips `@/` alias paths (handled by VFS plugin).
   * Marks all bare specifiers as external -- they will be resolved at runtime
   * via importmap or CDN URLs.
   *
   * @returns {object} An esbuild plugin object.
   */
  _createCDNPlugin() {
    return {
      name: "cdn",
      setup(build) {
        // Match bare specifiers: start with a letter or @ (scoped packages)
        build.onResolve({ filter: /^[a-zA-Z@]/ }, (args) => {
          // Skip @/ alias -- it's handled by VFS plugin
          if (args.path.startsWith("@/")) {
            return null;
          }

          // Mark all bare specifiers as external
          return { path: args.path, external: true };
        });
      },
    };
  },

  /**
   * Create the CSS plugin for esbuild.
   *
   * Intercepts `.css` file loads from the VFS namespace, collects the CSS
   * content into `_collectedCSS`, and returns an empty JS module so
   * esbuild does not attempt to parse CSS as JavaScript.
   *
   * @returns {object} An esbuild plugin object.
   */
  _createCSSPlugin() {
    const bundler = this;

    return {
      name: "css",
      setup(build) {
        build.onLoad({ filter: /\.css$/, namespace: "vfs" }, (args) => {
          const contents = VirtualFS.read(args.path);

          if (contents !== null) {
            bundler._collectedCSS.push(contents);
          }

          // Return empty JS module so esbuild skips CSS parsing
          return { contents: "", loader: "js" };
        });
      },
    };
  },

  // ── Caching ─────────────────────────────────────────────

  /**
   * Compute a cache key from VFS file contents and bundle options.
   *
   * Uses a djb2-style hash over the entry point, env keys/values, and all
   * VFS file paths and contents. Samples first and last 256 characters of
   * each file for a balance between speed and collision resistance.
   *
   * @param {string} entry - The entry point path.
   * @param {Record<string, string>} env - Environment variables.
   * @returns {string} A hash string for the current file state.
   * @private
   */
  _computeCacheKey(entry, env) {
    const files = VirtualFS.list(); // Returns sorted array of paths
    let hash = 5381;

    /**
     * Feed a string into the running hash.
     * @param {string} str
     */
    const feedString = (str) => {
      for (let i = 0; i < str.length; i++) {
        hash = ((hash << 5) + hash + str.charCodeAt(i)) | 0;
      }
    };

    // Include entry point
    feedString("entry:");
    feedString(entry);

    // Include env variables (sorted for determinism)
    const envKeys = Object.keys(env).sort();
    for (const key of envKeys) {
      feedString("env:");
      feedString(key);
      feedString("=");
      feedString(env[key]);
    }

    // Include all VFS file paths and content
    for (const path of files) {
      feedString("file:");
      feedString(path);

      const content = VirtualFS.read(path) || "";

      // Hash content length
      hash = ((hash << 5) + hash + content.length) | 0;

      // Hash first 256 chars
      const sampleLen = Math.min(256, content.length);
      for (let i = 0; i < sampleLen; i++) {
        hash = ((hash << 5) + hash + content.charCodeAt(i)) | 0;
      }

      // Hash last 256 chars (if file is longer than 256)
      if (content.length > 256) {
        for (let i = content.length - 256; i < content.length; i++) {
          hash = ((hash << 5) + hash + content.charCodeAt(i)) | 0;
        }
      }
    }

    return String(hash);
  },

  /**
   * Clear the bundle result cache.
   *
   * Call this when VFS is reset or when cached results should be invalidated
   * (e.g., after switching projects).
   */
  clearCache() {
    this._bundleCache.clear();
    console.info("[Bundler] Cache cleared.");
  },

  // ── Bundle ──────────────────────────────────────────────

  /**
   * Bundle a multi-file project from VirtualFS.
   *
   * Results are cached by a hash of VFS contents and bundle options.
   * If the cache contains a matching entry, the cached result is returned
   * without invoking esbuild. Only successful builds (no errors) are cached.
   *
   * @param {object} options - Bundle options.
   * @param {string} options.entry - Entry point path in VirtualFS (e.g., "/src/index.tsx").
   * @param {Record<string, string>} [options.env={}] - Additional environment variables
   *   to define as `process.env.KEY` replacements.
   * @returns {Promise<{ js: string, css: string, errors: string[] }>}
   *   Bundled JavaScript, collected CSS, and any error messages.
   */
  async bundle({ entry, env = {} }) {
    // Ensure esbuild is initialized
    await this.init();

    // Check bundle cache
    const cacheKey = this._computeCacheKey(entry, env);
    const cached = this._bundleCache.get(cacheKey);
    if (cached) {
      console.info("[Bundler] Cache hit, skipping rebuild.");
      return cached;
    }

    // Reset collected CSS for this build
    this._collectedCSS = [];

    // Build define map for process.env replacements
    const define = {
      "process.env.NODE_ENV": JSON.stringify("production"),
    };
    for (const [key, value] of Object.entries(env)) {
      define[`process.env.${key}`] = JSON.stringify(value);
    }

    try {
      const result = await esbuild.build({
        entryPoints: [entry],
        bundle: true,
        format: "esm",
        jsx: "automatic",
        jsxImportSource: "react",
        write: false,
        define,
        plugins: [
          this._createVFSPlugin(),
          this._createCDNPlugin(),
          this._createCSSPlugin(),
        ],
      });

      const js = result.outputFiles && result.outputFiles.length > 0
        ? result.outputFiles[0].text
        : "";

      const css = this._collectedCSS.join("\n");

      const errors = (result.errors || []).map((e) => e.text || String(e));

      const bundleResult = { js, css, errors };

      // Cache successful builds only
      if (errors.length === 0) {
        this._bundleCache.set(cacheKey, bundleResult);
      }

      return bundleResult;
    } catch (e) {
      console.error("[Bundler] Build failed:", e);

      // esbuild throws with an errors array on build failure
      const errors = e.errors
        ? e.errors.map((err) => err.text || String(err))
        : [e.message || String(e)];

      return { js: "", css: "", errors };
    }
  },

  // ── Helpers ─────────────────────────────────────────────

  /**
   * Map a file extension to an esbuild loader.
   *
   * @param {string} filePath - The file path to determine the loader for.
   * @returns {string} The esbuild loader name.
   * @private
   */
  _getLoader(filePath) {
    const ext = filePath.substring(filePath.lastIndexOf(".")).toLowerCase();
    const loaderMap = {
      ".ts": "ts",
      ".tsx": "tsx",
      ".js": "jsx",
      ".jsx": "jsx",
      ".mjs": "js",
      ".json": "json",
      ".css": "css",
    };
    return loaderMap[ext] || "js";
  },
};
