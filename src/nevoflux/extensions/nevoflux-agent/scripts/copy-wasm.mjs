#!/usr/bin/env node

/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Copy WASM build outputs to the extension's wasm/ directory
 *
 * This script copies the Trunk build outputs from dioxus-ui/dist/
 * to the extension's wasm/ directory where they can be loaded.
 */

import { promises as fs } from 'fs';
import path from 'path';
import { fileURLToPath } from 'url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const ROOT = path.resolve(__dirname, '..');

const BUILDS = [
  {
    name: 'chat-sidebar',
    src: path.join(ROOT, 'dioxus-ui', 'dist', 'chat-sidebar'),
    dest: path.join(ROOT, 'wasm', 'chat-sidebar'),
  },
  {
    name: 'content-sidebar',
    src: path.join(ROOT, 'dioxus-ui', 'dist', 'content-sidebar'),
    dest: path.join(ROOT, 'wasm', 'content-sidebar'),
  },
];

async function copyDir(src, dest) {
  // Create destination directory
  await fs.mkdir(dest, { recursive: true });

  // Read source directory
  const entries = await fs.readdir(src, { withFileTypes: true });

  for (const entry of entries) {
    const srcPath = path.join(src, entry.name);
    const destPath = path.join(dest, entry.name);

    if (entry.isDirectory()) {
      await copyDir(srcPath, destPath);
    } else {
      await fs.copyFile(srcPath, destPath);
      console.log(`  Copied: ${entry.name}`);
    }
  }
}

async function main() {
  console.log('Copying WASM builds to extension...\n');

  for (const build of BUILDS) {
    console.log(`[${build.name}]`);

    try {
      // Check if source exists
      await fs.access(build.src);

      // Remove existing destination
      try {
        await fs.rm(build.dest, { recursive: true, force: true });
      } catch (e) {
        // Directory may not exist
      }

      // Copy files
      await copyDir(build.src, build.dest);
      console.log(`  ✓ Copied to ${path.relative(ROOT, build.dest)}\n`);
    } catch (error) {
      if (error.code === 'ENOENT') {
        console.log(`  ⚠ Source not found: ${path.relative(ROOT, build.src)}`);
        console.log(`  Run 'trunk build' in dioxus-ui/${build.name}/ first\n`);
      } else {
        console.error(`  ✗ Error: ${error.message}\n`);
      }
    }
  }

  console.log('Done!');
}

main().catch(console.error);
