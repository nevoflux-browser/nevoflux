// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at http://mozilla.org/MPL/2.0/.

import js from '@eslint/js';
import globals from 'globals';
import { defineConfig, globalIgnores } from 'eslint/config';
import zenGlobals from './src/zen/zen.globals.js';

export default defineConfig([
  globalIgnores([
    '**/mochitests/**',
    '**/dioxus-ui/target/**',
    '**/dioxus-ui/dist/**',
    '**/wasm/chat-sidebar/**',
    '**/vendor/**',
  ]),
  {
    files: ['**/*.{js,mjs,cjs}'],
    plugins: { js },
    extends: ['js/recommended'],
    languageOptions: {
      globals: {
        ...globals.browser,
        ...zenGlobals.reduce((acc, global) => {
          acc[global] = 'readable';
          return acc;
        }, {}),
        // WebExtension API global
        browser: 'readable',
        // Firefox preference function
        pref: 'readable',
        // Firefox/Gecko internals
        Components: 'readable',
        Cr: 'readable',
        InspectorUtils: 'readable',
        WebExtensionPolicy: 'readable',
        // WebExtension toolkit globals
        ExtensionAPI: 'readable',
        ExtensionParent: 'readable',
        EventManager: 'readable',
        tabTracker: 'readable',
        // NevoFlux page globals
        NevofluxPage: 'readable',
        VirtualFS: 'readable',
        Bundler: 'readable',
        CanvasRuntime: 'readable',
        Babel: 'readable',
        // Zen sidebar globals
        gSidebarRevampEnabled: 'readable',
        gAllowTransparentBrowser: 'readable',
      },
    },
    rules: {
      'no-empty': ['error', { allowEmptyCatch: true }],
      'no-unused-vars': [
        'warn',
        {
          caughtErrors: 'none',
          argsIgnorePattern: '^_',
          varsIgnorePattern: '^_',
          destructuredArrayIgnorePattern: '^_',
        },
      ],
      'no-prototype-builtins': 'warn',
      'no-useless-escape': 'warn',
      'no-case-declarations': 'warn',
    },
    ignores: ['**/tests/**'],
  },
  {
    files: ['**/scripts/**/*.{js,mjs,cjs}', '**/api-schema/**/*.js'],
    languageOptions: {
      globals: {
        ...globals.node,
      },
    },
  },
]);
