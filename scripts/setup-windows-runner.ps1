#!/usr/bin/env pwsh
# setup-windows-runner.ps1 — One-time provisioning checklist for the
# nevoflux-windows self-hosted GitHub Actions runner.
#
# This script is INFORMATIONAL — it prints commands the operator should run
# (or has already run) to set up the machine. It does NOT install anything
# silently because most installers require user consent for licensing.
#
# Usage:
#   pwsh scripts/setup-windows-runner.ps1
#
# Verification after install:
#   pwsh scripts/gha/check-windows-runner.ps1

Write-Host @"
=== nevoflux-windows runner setup checklist ===

This is a CHECKLIST, not an installer. Run each section's commands manually
(many installers are interactive). After completing all sections, verify with:

    pwsh scripts/gha/check-windows-runner.ps1

The check script returns 0 only when every prerequisite is satisfied.

------------------------------------------------------------------------------
[1] MozillaBuild (provides MSYS2 bash, used for mach python build/pgo/profileserver.py)
------------------------------------------------------------------------------
Download:  https://ftp.mozilla.org/pub/mozilla/libraries/win32/MozillaBuildSetup-Latest.exe
Install:   Run installer with defaults (installs to C:\mozilla-build).
Verify:    Test-Path 'C:\mozilla-build\start-shell.bat'

------------------------------------------------------------------------------
[2] Git for Windows (Git Bash, used by npm / surfer / mach)
------------------------------------------------------------------------------
Download:  https://gitforwindows.org/
Install:   Defaults are fine. ENSURE "Git Bash Here" is enabled and
           git.exe is added to PATH.
Verify:    & 'C:\Program Files\Git\bin\bash.exe' --version

------------------------------------------------------------------------------
[3] Node.js v20 (matches .nvmrc)
------------------------------------------------------------------------------
Recommended: nvm-windows from https://github.com/coreybutler/nvm-windows
  nvm install 20
  nvm use 20
Or:  Direct LTS installer from https://nodejs.org/
Verify:  node --version  # expect v20.x.x

------------------------------------------------------------------------------
[4] Python 3.11+ (mach Python runtime)
------------------------------------------------------------------------------
Download:  https://www.python.org/downloads/windows/  (3.11 or newer)
Install:   IMPORTANT - check "Add python.exe to PATH" on first installer screen
Verify:    python --version  # expect 3.11+

------------------------------------------------------------------------------
[5] Rust toolchain (rustup-installed, host = x86_64-pc-windows-msvc)
------------------------------------------------------------------------------
Download:  https://rustup.rs/   (rustup-init.exe)
Install:   Choose option 1 (default install). Host triple defaults to
           x86_64-pc-windows-msvc, which is what we want.
Add targets needed for cross-target arm64 builds:
  rustup target add x86_64-pc-windows-msvc
  rustup target add aarch64-pc-windows-msvc
Verify:   rustup target list --installed
          (expect both targets above)

------------------------------------------------------------------------------
[6] Visual Studio 2022 Build Tools (MSVC compiler + SDK)
------------------------------------------------------------------------------
Download:  https://visualstudio.microsoft.com/downloads/  (Build Tools for VS2022)
Install:   Check the following components AT MINIMUM:
             * Desktop development with C++ workload
             * MSVC v143 - VS 2022 C++ x64/x86 build tools
             * MSVC v143 - VS 2022 C++ ARM64 build tools  <-- CRITICAL for arm64 cross-target
             * Windows 11 SDK (10.0.22621 or newer)
             * C++ ATL for v143 build tools (x86 & x64)
             * C++ ATL for v143 build tools (ARM64)       <-- CRITICAL for arm64 cross-target
Verify:    pwsh scripts/gha/check-windows-runner.ps1
           (it tests for link.exe at the ARM64 host path)

------------------------------------------------------------------------------
[7] GitHub CLI (gh)
------------------------------------------------------------------------------
Download:  https://cli.github.com/
Install:   MSI installer; add to PATH.
Verify:    gh --version
Note:      No need to ``gh auth login`` interactively - CI passes GH_TOKEN.

------------------------------------------------------------------------------
[8] GitHub Actions self-hosted runner agent
------------------------------------------------------------------------------
Register the machine with the GitHub repo:
  Settings -> Actions -> Runners -> New self-hosted runner -> Windows
Apply the labels:
  self-hosted
  Windows
  nevoflux-windows                                  <-- workflow's runs-on key
  x64
Install as a service (recommended):
  .\config.cmd --name nevoflux-windows --labels nevoflux-windows ...
  .\svc.cmd install
  .\svc.cmd start

------------------------------------------------------------------------------
[9] Disk + reboot
------------------------------------------------------------------------------
Reserve >= 200 GB free on C:\. Initial source download + two obj-* trees +
caches consume ~150 GB at peak.

Reboot the machine after all installs to ensure PATH and registered services
take effect.

------------------------------------------------------------------------------
Final verification
------------------------------------------------------------------------------
After all steps:

    pwsh scripts/gha/check-windows-runner.ps1

Exit code 0 = ready for CI. Non-zero = fix listed missing items.
"@
