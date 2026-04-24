/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

'use strict';

/**
 * Bundler -- Babel-based in-browser bundler for Canvas multi-file projects.
 *
 * Uses Babel standalone (loaded via `<script>` tag, available as `globalThis.Babel`)
 * for JSX/TypeScript transformation, and a lightweight module system for bundling.
 *
 * Transforms:
 * - JSX/TSX via Babel preset-react (automatic runtime)
 * - TypeScript via Babel preset-typescript
 * - ES module imports → CommonJS require() via Babel plugin
 * - Vue SFC via vue-compiler-sfc (optional)
 * - Svelte via svelte compiler (optional)
 * - CSS collected and injected separately
 *
 * External packages (react, vue, svelte) are resolved via importmap at runtime.
 *
 * Assumes `globalThis.Babel` and `VirtualFS` are available as globals.
 */
// eslint-disable-next-line no-unused-vars -- consumed by canvas-runtime.js across <script> tags in canvas.html
const Bundler = {
  /** @type {boolean} Whether Babel has been verified available. */
  _initialized: false,

  /** @type {boolean} Whether initialization is currently in progress. */
  _initializing: false,

  /** @type {string[]} CSS chunks collected during a build. */
  _collectedCSS: [],

  /** @type {Map<string, {js: string, css: string, errors: string[]}>} */
  _bundleCache: new Map(),

  /** @type {object|null} Lazily-loaded Vue compiler-sfc module. */
  _vueCompiler: null,

  /** @type {boolean} Whether Svelte compiler global is confirmed available. */
  _svelteCompilerReady: false,

  /**
   * Framework packages mapped to esm.sh CDN URLs for runtime importmap.
   * @type {Record<string, string>}
   */
  IMPORTMAP_PACKAGES: {
    react: 'https://esm.sh/react@18?dev',
    'react-dom': 'https://esm.sh/react-dom@18?dev',
    'react-dom/client': 'https://esm.sh/react-dom@18/client?dev',
    'react/jsx-runtime': 'https://esm.sh/react@18/jsx-runtime?dev',
    'react/jsx-dev-runtime': 'https://esm.sh/react@18/jsx-dev-runtime?dev',
    vue: 'https://esm.sh/vue@3?dev',
    svelte: 'https://esm.sh/svelte@4?dev',
    'svelte/internal': 'https://esm.sh/svelte@4/internal?dev',
  },

  // ── Initialization ──────────────────────────────────────

  /**
   * Initialize the Babel-based bundler.
   *
   * Idempotent: subsequent calls are no-ops once initialization completes.
   * Concurrent calls during initialization will wait for the first to finish.
   *
   * @returns {Promise<void>}
   * @throws {Error} If Babel global is not available.
   */
  async init() {
    if (this._initialized) {
      return;
    }

    // Guard against concurrent initialization calls
    if (this._initializing) {
      while (this._initializing) {
        await new Promise((resolve) => setTimeout(resolve, 50));
      }
      return;
    }

    this._initializing = true;

    try {
      if (typeof Babel === 'undefined') {
        throw new Error(
          'Bundler: Babel global not found. Ensure babel.min.js is loaded via <script> tag.'
        );
      }

      this._initialized = true;
      console.info('[Bundler] Babel-based bundler initialized successfully.');
    } catch (e) {
      console.error('[Bundler] Failed to initialize:', e);
      throw e;
    } finally {
      this._initializing = false;
    }
  },

  // ── Caching ─────────────────────────────────────────────

  /**
   * Compute a cache key from VFS file contents and bundle options.
   *
   * @param {string} entry - The entry point path.
   * @param {Record<string, string>} env - Environment variables.
   * @returns {string} A hash string for the current file state.
   * @private
   */
  _computeCacheKey(entry, env) {
    const files = VirtualFS.list();
    let hash = 5381;

    const feedString = (str) => {
      for (let i = 0; i < str.length; i++) {
        hash = ((hash << 5) + hash + str.charCodeAt(i)) | 0;
      }
    };

    feedString('entry:');
    feedString(entry);

    const envKeys = Object.keys(env).sort();
    for (const key of envKeys) {
      feedString('env:');
      feedString(key);
      feedString('=');
      feedString(env[key]);
    }

    for (const path of files) {
      feedString('file:');
      feedString(path);

      const content = VirtualFS.read(path) || '';
      hash = ((hash << 5) + hash + content.length) | 0;

      const sampleLen = Math.min(256, content.length);
      for (let i = 0; i < sampleLen; i++) {
        hash = ((hash << 5) + hash + content.charCodeAt(i)) | 0;
      }

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
   */
  clearCache() {
    this._bundleCache.clear();
    console.info('[Bundler] Cache cleared.');
  },

  // ── Bundle ──────────────────────────────────────────────

  /**
   * Bundle a multi-file project from VirtualFS.
   *
   * @param {object} options - Bundle options.
   * @param {string} options.entry - Entry point path in VirtualFS.
   * @param {Record<string, string>} [options.env={}] - Environment variables.
   * @returns {Promise<{ js: string, css: string, errors: string[] }>}
   */
  async bundle({ entry, env = {} }) {
    await this.init();

    const cacheKey = this._computeCacheKey(entry, env);
    const cached = this._bundleCache.get(cacheKey);
    if (cached) {
      console.info('[Bundler] Cache hit, skipping rebuild.');
      return cached;
    }

    this._collectedCSS = [];

    const modules = new Map(); // path -> transformed CJS code
    const externals = new Set(); // external package names
    const errors = [];

    // 1. Resolve dependency graph and transform all modules
    this._resolveAndTransform(entry, modules, externals, errors);

    if (errors.length > 0) {
      return { js: '', css: this._collectedCSS.join('\n'), errors };
    }

    // 2. Generate bundled output
    const js = this._generateBundle(entry, modules, externals, env);
    const css = this._collectedCSS.join('\n');

    const bundleResult = { js, css, errors: [] };

    // Cache successful builds
    this._bundleCache.set(cacheKey, bundleResult);

    return bundleResult;
  },

  // ── Dependency Resolution & Transformation ──────────────

  /**
   * Recursively resolve and transform all modules starting from an entry path.
   *
   * @param {string} importPath - The path to resolve (may be relative or absolute).
   * @param {Map<string, string>} modules - Accumulated modules map.
   * @param {Set<string>} externals - Accumulated external package names.
   * @param {string[]} errors - Accumulated error messages.
   * @param {Set<string>} [visited] - Already-visited paths (cycle prevention).
   * @param {string} [importer] - The file importing this module (for relative resolution).
   * @private
   */
  _resolveAndTransform(
    importPath,
    modules,
    externals,
    errors,
    visited = new Set(),
    importer = null
  ) {
    // Resolve the absolute path
    let absPath;
    if (importer && (importPath.startsWith('./') || importPath.startsWith('../'))) {
      absPath = VirtualFS.resolve(importer, importPath);
    } else if (importPath.startsWith('@/')) {
      absPath = VirtualFS.resolve('/', importPath);
    } else {
      absPath = importPath.startsWith('/') ? importPath : '/' + importPath;
    }

    // Try with extension
    absPath = VirtualFS.resolveWithExtension(absPath) || absPath;

    if (visited.has(absPath)) return;
    visited.add(absPath);

    const source = VirtualFS.read(absPath);
    if (source === null) {
      errors.push(`File not found: ${importPath} (resolved to ${absPath})`);
      return;
    }

    // Handle CSS files — collect and register as empty module
    if (absPath.endsWith('.css')) {
      this._collectedCSS.push(source);
      modules.set(absPath, '/* css */');
      return;
    }

    // Preprocess Vue/Svelte to JS
    let jsSource = source;
    if (absPath.endsWith('.vue')) {
      jsSource = this._compileVue(absPath, source, errors);
      if (!jsSource) return;
    } else if (absPath.endsWith('.svelte')) {
      jsSource = this._compileSvelte(absPath, source, errors);
      if (!jsSource) return;
    }

    // Transform with Babel
    try {
      const result = Babel.transform(jsSource, {
        presets: [
          ['react', { runtime: 'automatic' }],
          ['typescript', { allExtensions: true, isTSX: true }],
        ],
        plugins: [['transform-modules-commonjs']],
        filename: absPath,
      });

      const transformed = result.code;

      // Parse require() calls to find dependencies
      const requireRegex = /require\(["']([^"']+)["']\)/g;
      let match;
      while ((match = requireRegex.exec(transformed)) !== null) {
        const dep = match[1];
        if (dep.startsWith('.') || dep.startsWith('/') || dep.startsWith('@/')) {
          // Local import — resolve and recurse
          this._resolveAndTransform(dep, modules, externals, errors, visited, absPath);
        } else {
          // External package
          externals.add(dep);
        }
      }

      modules.set(absPath, transformed);
    } catch (e) {
      errors.push(`Transform failed for ${absPath}: ${e.message}`);
    }
  },

  // ── Bundle Generation ───────────────────────────────────

  /**
   * Generate the final bundled JavaScript output.
   *
   * Output format:
   * - ESM imports for external packages (resolved by importmap)
   * - Lightweight CommonJS module registry
   * - Module factory definitions
   * - Entry point execution
   *
   * @param {string} entry - The entry point path.
   * @param {Map<string, string>} modules - Transformed module map.
   * @param {Set<string>} externals - External package names.
   * @param {Record<string, string>} env - Environment variables.
   * @returns {string} The bundled JavaScript code.
   * @private
   */
  _generateBundle(entry, modules, externals, env) {
    const lines = [];

    // 1. External imports via importmap
    const extEntries = [...externals];
    for (const ext of extEntries) {
      const safeName = '__ext_' + ext.replace(/[^a-zA-Z0-9]/g, '_');
      lines.push(`import * as ${safeName} from ${JSON.stringify(ext)};`);
    }

    // 2. process.env shim
    lines.push(
      `\nvar process = { env: { NODE_ENV: "production"${Object.entries(env)
        .map(([k, v]) => `, ${JSON.stringify(k)}: ${JSON.stringify(v)}`)
        .join('')} } };`
    );

    // 3. Module system
    lines.push(`
var __modules = new Map();
var __cache = new Map();`);

    // Pre-populate external modules
    for (const ext of extEntries) {
      const safeName = '__ext_' + ext.replace(/[^a-zA-Z0-9]/g, '_');
      lines.push(`__cache.set(${JSON.stringify(ext)}, ${safeName});`);
    }

    lines.push(`
function __require(id) {
  if (__cache.has(id)) return __cache.get(id);
  var fn = __modules.get(id);
  if (!fn) { console.error("[Bundle] Module not found:", id); return {}; }
  var module = { exports: {} };
  __cache.set(id, module.exports);
  fn(module.exports, module);
  if (module.exports !== __cache.get(id)) __cache.set(id, module.exports);
  return __cache.get(id);
}
`);

    // 4. Module definitions
    for (const [path, code] of modules) {
      if (code === '/* css */') {
        // CSS module — return empty
        lines.push(`__modules.set(${JSON.stringify(path)}, function(exports, module) {});`);
        continue;
      }

      // Rewrite require("./foo") calls to use absolute resolved paths
      const rewritten = code.replace(/require\(["']([^"']+)["']\)/g, (_match, dep) => {
        if (dep.startsWith('.') || dep.startsWith('/') || dep.startsWith('@/')) {
          const resolved = VirtualFS.resolve(path, dep);
          const withExt = VirtualFS.resolveWithExtension(resolved) || resolved;
          return `__require(${JSON.stringify(withExt)})`;
        }
        return `__require(${JSON.stringify(dep)})`;
      });

      lines.push(
        `__modules.set(${JSON.stringify(path)}, function(exports, module) {\n${rewritten}\n});`
      );
    }

    // 5. Execute entry point
    const entryAbs = VirtualFS.resolveWithExtension(entry) || entry;
    lines.push(`\n__require(${JSON.stringify(entryAbs)});`);

    return lines.join('\n');
  },

  // ── Vue SFC Compilation ─────────────────────────────────

  /**
   * Compile a Vue SFC to JavaScript.
   *
   * @param {string} filePath - The file path.
   * @param {string} source - The Vue SFC source code.
   * @param {string[]} errors - Error accumulator.
   * @returns {string|null} Compiled JavaScript or null on error.
   * @private
   */
  _compileVue(filePath, source, errors) {
    if (!this._vueCompiler) {
      // Try lazy load — if unavailable, return an error
      errors.push(`Vue compiler not loaded. Cannot compile ${filePath}.`);
      return null;
    }

    const { parse, compileScript, compileTemplate } = this._vueCompiler;

    try {
      const { descriptor, errors: parseErrors } = parse(source, { filename: filePath });

      if (parseErrors && parseErrors.length > 0) {
        errors.push(...parseErrors.map((e) => `Vue parse error in ${filePath}: ${e.message || e}`));
        return null;
      }

      const sfcId = filePath.replace(/[^a-zA-Z0-9]/g, '_');
      const parts = [];

      if (descriptor.script || descriptor.scriptSetup) {
        const scriptResult = compileScript(descriptor, { id: sfcId, inlineTemplate: true });
        parts.push(scriptResult.content);
      }

      if (descriptor.template && !descriptor.scriptSetup) {
        const templateResult = compileTemplate({
          source: descriptor.template.content,
          filename: filePath,
          id: sfcId,
        });
        if (templateResult.errors && templateResult.errors.length > 0) {
          errors.push(
            ...templateResult.errors.map(
              (e) => `Vue template error in ${filePath}: ${e.message || e}`
            )
          );
          return null;
        }
        parts.push(templateResult.code);
      }

      for (const style of descriptor.styles || []) {
        if (style.content) {
          this._collectedCSS.push(style.content);
        }
      }

      return parts.join('\n');
    } catch (e) {
      errors.push(`Vue SFC compilation failed for ${filePath}: ${e.message || e}`);
      return null;
    }
  },

  // ── Svelte Compilation ──────────────────────────────────

  /**
   * Compile a Svelte component to JavaScript.
   *
   * @param {string} filePath - The file path.
   * @param {string} source - The Svelte source code.
   * @param {string[]} errors - Error accumulator.
   * @returns {string|null} Compiled JavaScript or null on error.
   * @private
   */
  _compileSvelte(filePath, source, errors) {
    if (typeof globalThis.svelte === 'undefined' || !globalThis.svelte.compile) {
      errors.push(`Svelte compiler not available. Cannot compile ${filePath}.`);
      return null;
    }

    try {
      const result = globalThis.svelte.compile(source, {
        filename: filePath,
        generate: 'dom',
        css: 'injected',
        hydratable: false,
        dev: false,
      });

      if (result.warnings && result.warnings.length > 0) {
        for (const w of result.warnings) {
          console.warn(`[Bundler] Svelte warning in ${filePath}: ${w.message}`);
        }
      }

      if (result.css && result.css.code) {
        this._collectedCSS.push(result.css.code);
      }

      return result.js.code;
    } catch (e) {
      errors.push(`Svelte compilation failed for ${filePath}: ${e.message || e}`);
      return null;
    }
  },
};
