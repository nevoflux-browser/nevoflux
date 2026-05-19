#!/usr/bin/env pwsh
# check-windows-runner.ps1 — Fail-fast prerequisite check for nevoflux-windows runner.
# Invoked from the prepare job's first step. Exits non-zero with a clear list of
# missing tools if any prerequisite is absent.

$ErrorActionPreference = 'Stop'

$missing = @()

function Test-Cmd {
  param([string]$Name, [string]$Cmd, [string]$VersionArg = '--version')
  try {
    $null = & $Cmd $VersionArg 2>&1
    if ($LASTEXITCODE -ne 0) { throw "exit $LASTEXITCODE" }
    Write-Host "OK  $Name"
  } catch {
    Write-Host "MISSING  $Name ($Cmd $VersionArg)"
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
    $verDir = Get-ChildItem $msvcDir -Directory | Select-Object -First 1
    if ($verDir) {
      $arm64Link = Join-Path $verDir.FullName 'bin\Hostx64\arm64\link.exe'
      if (Test-Path $arm64Link) {
        Write-Host "OK  VS2022 $edition ARM64 toolchain ($arm64Link)"
        $vsFound = $true
        break
      }
    }
  }
}
if (-not $vsFound) {
  Write-Host "MISSING  VS2022 with 'MSVC v143 ARM64 build tools' component"
  $missing += 'vs2022-arm64-tools'
}

# --- Disk space ---
$cFree = (Get-PSDrive C).Free / 1GB
Write-Host ("INFO C: free = {0:N1} GB" -f $cFree)
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
