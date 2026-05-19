#!/usr/bin/env node
if (process.platform !== 'win32') {
  console.log('[postinstall] Non-Windows platform, skipping patch-package.');
  process.exit(0);
}

const { spawnSync } = require('node:child_process');
const result = spawnSync('npx', ['--no-install', 'patch-package'], {
  stdio: 'inherit',
  shell: true,
});
process.exit(result.status ?? 1);
