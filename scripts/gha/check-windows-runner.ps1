#!/usr/bin/env pwsh
# check-windows-runner.ps1 — Fail-fast prerequisite check for nevoflux-windows runner.
# Invoked from the prepare job's first step. Exits non-zero with a clear list of
# missing tools if any prerequisite is absent.

$ErrorActionPreference = 'Stop'

$missing = @()

function Test-Cmd {
  # Check command presence via Get-Command (no execution).
  # We avoid invoking the command + `2>&1` because Windows PowerShell 5.1 wraps
  # native-exe stderr lines in NativeCommandError records under
  # $ErrorActionPreference='Stop', causing false MISSING reports for tools that
  # write info banners to stderr (e.g. `rustup --version`).
  param([string]$Name, [string]$Cmd, [string]$VersionArg = '--version')
  if (Get-Command $Cmd -ErrorAction SilentlyContinue) {
    Write-Host "OK  $Name"
  } else {
    Write-Host "MISSING  $Name (not on PATH)"
    $script:missing += $Name
  }
}

function Test-Path-Exists {
  param([string]$Name, [string]$Path)
  if (Test-Path $Path) {
    Write-Host "OK  $Name ($Path)"
  } else {
    Write-Host "MISSING  $Name (expected at $Path)"
    $script:missing += $Name
  }
}

Write-Host "=== nevoflux-windows runner prerequisite check ==="

# --- Tools on PATH ---
Test-Cmd 'pwsh (PowerShell 7+)' 'pwsh' '--version'
Test-Cmd 'node'          'node'   '--version'
Test-Cmd 'npm'           'npm'    '--version'
Test-Cmd 'git'           'git'    '--version'
Test-Cmd 'bash (Git Bash)' 'bash' '--version'
Test-Cmd 'python'        'python' '--version'
Test-Cmd 'gh CLI'        'gh'     '--version'
Test-Cmd 'rustc'         'rustc'  '--version'
Test-Cmd 'cargo'         'cargo'  '--version'
Test-Cmd 'rustup'        'rustup' '--version'
Test-Cmd 'tar'           'tar'    '--version'

# --- MozillaBuild ---
Test-Path-Exists 'MozillaBuild bash' 'C:\mozilla-build\msys2\usr\bin\bash.exe'
Test-Path-Exists 'MozillaBuild start-shell.bat' 'C:\mozilla-build\start-shell.bat'

# --- Rust targets ---
$installedTargets = & rustup target list --installed 2>$null
foreach ($t in @('x86_64-pc-windows-msvc','aarch64-pc-windows-msvc')) {
  if ($installedTargets -match $t) {
    Write-Host "OK  Rust target $t"
  } else {
    Write-Host "MISSING  Rust target $t (run: rustup target add $t)"
    $missing += "rust-target-$t"
  }
}

# --- VS2022 MSVC ARM64 build tools (check for link.exe with /MACHINE:ARM64 support) ---
$vsBase = 'C:\Program Files\Microsoft Visual Studio\2022'
$vsFound = $false
foreach ($edition in @('BuildTools','Community','Professional','Enterprise')) {
  $msvcDir = Join-Path $vsBase "$edition\VC\Tools\MSVC"
  if (Test-Path $msvcDir) {
    # Iterate ALL installed MSVC versions (newest first) - some versions may
    # ship without the ARM64 toolchain even when newer versions on the same
    # edition do. Pre-existing fix for a false-MISSING case noted in PR-3 review.
    foreach ($verDir in (Get-ChildItem $msvcDir -Directory | Sort-Object Name -Descending)) {
      $arm64Link = Join-Path $verDir.FullName 'bin\Hostx64\arm64\link.exe'
      if (Test-Path $arm64Link) {
        Write-Host "OK  VS2022 $edition ARM64 toolchain ($arm64Link)"
        $vsFound = $true
        break
      }
    }
    if ($vsFound) { break }
  }
}
if (-not $vsFound) {
  Write-Host "MISSING  VS2022 with 'MSVC v143 ARM64 build tools' component"
  $missing += 'vs2022-arm64-tools'
}

# --- Disk space ---
# Disk: 80 GB = CI startup gate; ~150 GB peak during full PGO+arm64 build;
# 200 GB recommended steady-state reserve (see scripts/setup-windows-runner.ps1 [10]).
$cFree = (Get-PSDrive C).Free / 1GB
Write-Host ("INFO C: free = {0:N1} GB  (gate: 80 GB, peak: ~150 GB, recommended: 200 GB)" -f $cFree)
if ($cFree -lt 80) {
  Write-Host "MISSING  >= 80 GB free on C: (have $([math]::Round($cFree,1)) GB)"
  $missing += 'disk-space-80gb'
}

# --- Summary ---
Write-Host ""
if ($missing.Count -gt 0) {
  Write-Host "=== FAIL: $($missing.Count) prerequisite(s) missing ==="
  $missing | ForEach-Object { Write-Host "  - $_" }
  Write-Host ""
  Write-Host "See scripts/setup-windows-runner.ps1 for install steps."
  exit 1
} else {
  Write-Host "=== PASS: all prerequisites satisfied ==="
  exit 0
}
