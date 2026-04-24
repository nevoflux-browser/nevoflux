/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

'use strict';

/**
 * VirtualFS -- in-memory file system backed by Map<string, string>.
 *
 * Provides a virtual file system for Canvas multi-file projects with:
 * - Path normalization and resolution
 * - `@/` alias mapping to `/src/`
 * - Extension auto-completion for module imports
 * - Bare specifier detection for npm packages
 * - CRUD and batch operations
 * - IndexedDB persistence per project
 *
 * All paths are stored and returned as absolute (starting with `/`).
 */
// eslint-disable-next-line no-unused-vars -- consumed by canvas.js and canvas-runtime.js across <script> tags in canvas.html
const VirtualFS = {
  /** @type {Map<string, string>} */
  _files: new Map(),

  /** @type {string|null} */
  _projectId: null,

  /** @type {IDBDatabase|null} */
  _db: null,

  /**
   * Extensions tried in order when resolving module specifiers.
   * @type {string[]}
   */
  _EXTENSIONS: [
    '',
    '.ts',
    '.tsx',
    '.js',
    '.jsx',
    '.mjs',
    '.vue',
    '.svelte',
    '/index.ts',
    '/index.tsx',
    '/index.js',
    '/index.jsx',
    '/index.mjs',
    '/index.vue',
    '/index.svelte',
  ],

  // ── Path Normalization ──────────────────────────────────

  /**
   * Normalize a file path by resolving `.`, `..`, redundant slashes,
   * and expanding `@/` alias to `/src/`.
   *
   * @param {string} path - The path to normalize.
   * @returns {string} Normalized absolute path starting with `/`.
   *
   * @example
   * VirtualFS.normalize("@/components/App")  // "/src/components/App"
   * VirtualFS.normalize("./a/../b")          // "/b"
   * VirtualFS.normalize("")                  // "/"
   * VirtualFS.normalize("///a//b/")          // "/a/b"
   */
  normalize(path) {
    if (!path) {
      return '/';
    }

    // Expand @/ alias to /src/
    if (path.startsWith('@/')) {
      path = '/src/' + path.slice(2);
    }

    // Ensure leading slash
    if (!path.startsWith('/')) {
      path = '/' + path;
    }

    // Split on slashes, resolve . and ..
    const parts = path.split('/');
    const resolved = [];

    for (const part of parts) {
      if (part === '' || part === '.') {
        continue;
      }
      if (part === '..') {
        resolved.pop();
      } else {
        resolved.push(part);
      }
    }

    return '/' + resolved.join('/');
  },

  // ── Path Resolution ─────────────────────────────────────

  /**
   * Resolve an import specifier relative to an importer file path.
   *
   * Handles absolute paths, relative paths (`./`, `../`), and `@/` aliases.
   *
   * @param {string} from - The absolute path of the importing file.
   * @param {string} to - The import specifier to resolve.
   * @returns {string} Resolved absolute path.
   *
   * @example
   * VirtualFS.resolve("/src/App.tsx", "./utils/helpers")
   *   // "/src/utils/helpers"
   * VirtualFS.resolve("/src/deep/nested/file.ts", "../../lib/core")
   *   // "/src/lib/core"
   * VirtualFS.resolve("/src/App.tsx", "@/components/Button")
   *   // "/src/components/Button"
   * VirtualFS.resolve("/src/App.tsx", "/lib/shared")
   *   // "/lib/shared"
   */
  resolve(from, to) {
    // Absolute paths and @/ aliases resolve directly
    if (to.startsWith('/') || to.startsWith('@/')) {
      return this.normalize(to);
    }

    // Relative paths resolve from the directory of the importer
    if (to.startsWith('./') || to.startsWith('../')) {
      const dir = from.substring(0, from.lastIndexOf('/')) || '/';
      return this.normalize(dir + '/' + to);
    }

    // Bare specifiers pass through normalization (though callers should
    // typically check isBareSpecifier first)
    return this.normalize(to);
  },

  /**
   * Resolve a path with extension auto-completion.
   *
   * Tries the resolved path with each extension in order and returns the
   * first path that exists in the file system.
   *
   * @param {string} resolvedPath - An already-resolved absolute path.
   * @returns {string|null} The matched path with extension, or null if
   *   no matching file exists.
   *
   * @example
   * // Given files: { "/src/App.tsx": "..." }
   * VirtualFS.resolveWithExtension("/src/App")  // "/src/App.tsx"
   * VirtualFS.resolveWithExtension("/src/None")  // null
   */
  resolveWithExtension(resolvedPath) {
    for (const ext of this._EXTENSIONS) {
      const candidate = resolvedPath + ext;
      if (this._files.has(candidate)) {
        return candidate;
      }
    }
    return null;
  },

  // ── Bare Specifier Detection ────────────────────────────

  /**
   * Determine whether a specifier is a bare (npm package) specifier.
   *
   * Returns `false` for paths starting with `/`, `./`, `../`, or `@/`.
   * Returns `true` for everything else, including scoped packages
   * like `@scope/pkg`.
   *
   * @param {string} specifier - The import specifier to check.
   * @returns {boolean} True if the specifier is a bare package name.
   *
   * @example
   * VirtualFS.isBareSpecifier("react")           // true
   * VirtualFS.isBareSpecifier("@tanstack/query")  // true
   * VirtualFS.isBareSpecifier("./App")            // false
   * VirtualFS.isBareSpecifier("../utils")         // false
   * VirtualFS.isBareSpecifier("@/components/Btn") // false
   * VirtualFS.isBareSpecifier("/lib/shared")      // false
   */
  isBareSpecifier(specifier) {
    if (
      specifier.startsWith('/') ||
      specifier.startsWith('./') ||
      specifier.startsWith('../') ||
      specifier.startsWith('@/')
    ) {
      return false;
    }
    return true;
  },

  // ── CRUD Operations ─────────────────────────────────────

  /**
   * Read a file from the virtual file system.
   *
   * @param {string} path - The file path (will be normalized).
   * @returns {string|null} The file contents, or null if not found.
   */
  read(path) {
    const normalized = this.normalize(path);
    return this._files.get(normalized) ?? null;
  },

  /**
   * Write a file to the virtual file system.
   *
   * @param {string} path - The file path (will be normalized).
   * @param {string} content - The file contents.
   */
  write(path, content) {
    const normalized = this.normalize(path);
    this._files.set(normalized, content);
  },

  /**
   * Delete a file from the virtual file system.
   *
   * @param {string} path - The file path (will be normalized).
   * @returns {boolean} True if the file was deleted, false if not found.
   */
  delete(path) {
    const normalized = this.normalize(path);
    return this._files.delete(normalized);
  },

  /**
   * List all file paths in the virtual file system.
   *
   * @returns {string[]} Array of absolute file paths, sorted alphabetically.
   */
  list() {
    return Array.from(this._files.keys()).sort();
  },

  // ── Batch Operations ────────────────────────────────────

  /**
   * Load multiple files from a plain object.
   *
   * Existing files with the same paths will be overwritten.
   * Files not present in the input are retained.
   *
   * @param {Record<string, string>} files - Map of path to content.
   */
  loadBatch(files) {
    for (const [path, content] of Object.entries(files)) {
      this.write(path, content);
    }
  },

  /**
   * Export all files as a plain object.
   *
   * @returns {Record<string, string>} Map of absolute path to content.
   */
  export() {
    const result = {};
    for (const [path, content] of this._files) {
      result[path] = content;
    }
    return result;
  },

  // ── Clear ───────────────────────────────────────────────

  /**
   * Remove all files from the virtual file system.
   */
  clear() {
    this._files.clear();
  },

  // ── IndexedDB Persistence ───────────────────────────────

  /**
   * Initialize IndexedDB persistence for a project.
   *
   * Opens (or creates) an IndexedDB database scoped to the given project ID.
   * Must be called before `persist()` or `load()`.
   *
   * @param {string} projectId - Unique project identifier.
   * @returns {Promise<void>}
   */
  async initPersistence(projectId) {
    this._projectId = projectId;
    const dbName = `nevoflux-vfs-${projectId}`;

    return new Promise((resolve, reject) => {
      const request = indexedDB.open(dbName, 1);

      request.onupgradeneeded = (event) => {
        const db = event.target.result;
        if (!db.objectStoreNames.contains('files')) {
          db.createObjectStore('files', { keyPath: 'path' });
        }
      };

      request.onsuccess = (event) => {
        this._db = event.target.result;
        resolve();
      };

      request.onerror = (event) => {
        console.error('[VirtualFS] IndexedDB open error:', event.target.error);
        reject(event.target.error);
      };
    });
  },

  /**
   * Persist all in-memory files to IndexedDB.
   *
   * Replaces all stored files with the current in-memory state.
   * Requires `initPersistence()` to have been called first.
   *
   * @returns {Promise<void>}
   * @throws {Error} If persistence has not been initialized.
   */
  async persist() {
    if (!this._db) {
      throw new Error('VirtualFS: persistence not initialized. Call initPersistence() first.');
    }

    return new Promise((resolve, reject) => {
      const tx = this._db.transaction('files', 'readwrite');
      const store = tx.objectStore('files');

      // Clear existing entries and write current state
      store.clear();

      for (const [path, content] of this._files) {
        store.put({ path, content });
      }

      tx.oncomplete = () => resolve();
      tx.onerror = (event) => {
        console.error('[VirtualFS] Persist error:', event.target.error);
        reject(event.target.error);
      };
    });
  },

  /**
   * Load all files from IndexedDB into memory.
   *
   * Replaces all in-memory files with the stored state.
   * Requires `initPersistence()` to have been called first.
   *
   * @returns {Promise<void>}
   * @throws {Error} If persistence has not been initialized.
   */
  async load() {
    if (!this._db) {
      throw new Error('VirtualFS: persistence not initialized. Call initPersistence() first.');
    }

    return new Promise((resolve, reject) => {
      const tx = this._db.transaction('files', 'readonly');
      const store = tx.objectStore('files');
      const request = store.getAll();

      request.onsuccess = () => {
        this._files.clear();
        for (const entry of request.result) {
          this._files.set(entry.path, entry.content);
        }
        resolve();
      };

      request.onerror = (event) => {
        console.error('[VirtualFS] Load error:', event.target.error);
        reject(event.target.error);
      };
    });
  },
};
