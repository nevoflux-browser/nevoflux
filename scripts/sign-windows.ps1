#!/usr/bin/env pwsh
<#
.SYNOPSIS
  Authenticode-sign the NevoFlux Windows release artifacts with Certum SimplySign
  (via a running, logged-in SimplySign Desktop).

.DESCRIPTION
  Two phases so you can log in to SimplySign between them:

    1. Download phase  - pull the unsigned Windows artifacts (installers +
       portable zips) from the draft GitHub Release into a work dir.
       Run with -DownloadOnly, then start + log in to SimplySign Desktop.

    2. Sign phase      - select the code-signing certificate that SimplySign
       Desktop loaded into Cert:\CurrentUser\My, sign the installers (and, by
       default, the PE files inside the portable zips), then re-upload the
       signed artifacts to the draft release (--clobber).

  SimplySign Desktop MUST be running and logged in before the sign phase: it
  exposes your certificate in the Windows cert store and performs the private
  key operation in the cloud. No .pfx is needed.

  Note: post-build signing signs the installer wrapper and the portable-zip
  binaries. It cannot sign the payload bundled *inside* the NSIS installer
  (that would require signing before packaging). The installer wrapper + the
  portable build are what users download and what SmartScreen evaluates.

.PARAMETER Version
  Release tag without a leading 'v' (e.g. 0.3.5). The draft release with this
  tag must already contain the unsigned Windows artifacts.

.PARAMETER ForceDownload
  Re-download even if the artifacts already exist in the work dir. Default: off
  — if every expected file is already present, the download is skipped.

.PARAMETER DownloadOnly
  Only download/prepare, then stop (use this first; then log in to SimplySign
  Desktop; then re-run WITHOUT this switch to sign + upload).

.PARAMETER Thumbprint
  SHA1 thumbprint of the code-signing cert to use. If omitted, the script
  auto-selects the single Code Signing cert in Cert:\CurrentUser\My (and errors
  if there are zero or several — then pass -Thumbprint).

.PARAMETER TimestampUrl
  RFC3161 timestamp server. Default: http://time.certum.pl

.PARAMETER Repo
  GitHub owner/repo of the release. Default: dorisgyl/nevoflux

.PARAMETER WorkDir
  Working directory. Default: $env:TEMP\nevoflux-winsign-<Version>

.PARAMETER InstallersOnly
  Sign only the *.installer*.exe files; skip extracting + signing the PE files
  inside the portable .zip archives. Faster, but the portable build stays
  unsigned. Default: off.

.PARAMETER NoUpload
  Sign locally but do not upload back to the draft release (inspect first).

.EXAMPLE
  # 1) prepare
  pwsh scripts/sign-windows.ps1 -Version 0.3.5 -DownloadOnly
  # 2) start SimplySign Desktop, log in, then:
  pwsh scripts/sign-windows.ps1 -Version 0.3.5
#>
[CmdletBinding()]
param(
  [Parameter(Mandatory = $true)] [string]$Version,
  [switch]$ForceDownload,
  [switch]$DownloadOnly,
  [string]$Thumbprint,
  [string]$TimestampUrl = 'http://time.certum.pl',
  [string]$Repo = 'dorisgyl/nevoflux',
  [string]$WorkDir,
  [switch]$InstallersOnly,
  [switch]$NoUpload
)

$ErrorActionPreference = 'Stop'

# Unsigned Windows assets expected in the draft release.
$Assets = @(
  'nevoflux.installer.exe',
  'nevoflux.installer-arm64.exe',
  'nevoflux.win-x86_64.zip',
  'nevoflux.win-arm64.zip'
)

if (-not $WorkDir) { $WorkDir = Join-Path $env:TEMP "nevoflux-winsign-$Version" }
New-Item -ItemType Directory -Force -Path $WorkDir | Out-Null
Write-Host "Work dir: $WorkDir"

# ----------------------------------------------------------------------------
# Phase 1: download (skip if already present unless -ForceDownload)
# ----------------------------------------------------------------------------
if (-not (Get-Command gh -ErrorAction SilentlyContinue)) {
  throw "gh CLI not found on PATH. Install GitHub CLI and run 'gh auth login'."
}

$missing = @($Assets | Where-Object { -not (Test-Path (Join-Path $WorkDir $_)) })
if ($ForceDownload -or $missing.Count -gt 0) {
  if ($ForceDownload) {
    Write-Host "ForceDownload set — downloading all artifacts."
  } else {
    Write-Host "Missing locally: $($missing -join ', ') — downloading."
  }
  foreach ($a in $Assets) {
    Write-Host "  downloading $a ..."
    & gh release download $Version --repo $Repo --pattern $a --dir $WorkDir --clobber
    if ($LASTEXITCODE -ne 0) { throw "gh release download failed for '$a' (version $Version)." }
  }
} else {
  Write-Host "All artifacts already in work dir — skipping download (use -ForceDownload to refresh)."
}

if ($DownloadOnly) {
  Write-Host ""
  Write-Host "==> Download complete."
  Write-Host "    1. Start SimplySign Desktop and log in (your code-signing cert"
  Write-Host "       then appears in Cert:\CurrentUser\My)."
  Write-Host "    2. Re-run WITHOUT -DownloadOnly to sign + upload:"
  Write-Host "         pwsh scripts/sign-windows.ps1 -Version $Version"
  return
}

# ----------------------------------------------------------------------------
# Phase 2: sign
# ----------------------------------------------------------------------------

# Locate signtool.exe (Windows SDK).
function Find-SignTool {
  $c = Get-Command signtool.exe -ErrorAction SilentlyContinue
  if ($c) { return $c.Source }
  $roots = @("${env:ProgramFiles(x86)}\Windows Kits\10\bin", "$env:ProgramFiles\Windows Kits\10\bin")
  foreach ($r in $roots) {
    if (Test-Path $r) {
      $st = Get-ChildItem -Path $r -Recurse -Filter signtool.exe -ErrorAction SilentlyContinue |
        Where-Object { $_.FullName -match '\\x64\\' } |
        Sort-Object FullName -Descending | Select-Object -First 1
      if ($st) { return $st.FullName }
    }
  }
  throw "signtool.exe not found. Install the Windows 10/11 SDK (Windows Kits)."
}
$SignTool = Find-SignTool
Write-Host "signtool: $SignTool"

# Select the code-signing certificate that SimplySign Desktop loaded.
$CODE_SIGNING_EKU = '1.3.6.1.5.5.7.3.3'
if ($Thumbprint) {
  $cert = Get-Item -Path ("Cert:\CurrentUser\My\" + $Thumbprint) -ErrorAction SilentlyContinue
  if (-not $cert) { throw "No cert with thumbprint $Thumbprint in Cert:\CurrentUser\My. Is SimplySign Desktop logged in?" }
} else {
  $codeCerts = @(Get-ChildItem Cert:\CurrentUser\My | Where-Object {
      $_.EnhancedKeyUsageList.ObjectId -contains $CODE_SIGNING_EKU
    })
  if ($codeCerts.Count -eq 0) {
    throw "No Code Signing certificate found in Cert:\CurrentUser\My. Start SimplySign Desktop and log in first."
  }
  if ($codeCerts.Count -gt 1) {
    Write-Host "Multiple code-signing certs found — re-run with -Thumbprint <one of>:"
    $codeCerts | ForEach-Object { Write-Host ("  {0}  {1}" -f $_.Thumbprint, $_.Subject) }
    throw "Ambiguous certificate — pass -Thumbprint."
  }
  $cert = $codeCerts[0]
}
$Thumbprint = $cert.Thumbprint
Write-Host "Signing cert: $($cert.Subject)  [$Thumbprint]"
Write-Host "Timestamp:    $TimestampUrl"

function Invoke-SignFiles {
  param([string[]]$Files)
  $Files = @($Files | Where-Object { $_ })
  if ($Files.Count -eq 0) { return }
  & $SignTool sign /fd SHA256 /tr $TimestampUrl /td SHA256 /sha1 $Thumbprint @Files
  if ($LASTEXITCODE -ne 0) { throw "signtool sign failed (exit $LASTEXITCODE). Is SimplySign Desktop logged in?" }
}

# Sign installers.
$installers = @(Get-ChildItem $WorkDir -Filter '*.installer*.exe' -ErrorAction SilentlyContinue | Select-Object -ExpandProperty FullName)
Write-Host ""
Write-Host "Signing $($installers.Count) installer(s)..."
Invoke-SignFiles -Files $installers

# Sign PE files inside the portable zips, then repack.
if (-not $InstallersOnly) {
  foreach ($zip in (Get-ChildItem $WorkDir -Filter '*.zip' -ErrorAction SilentlyContinue)) {
    Write-Host ""
    Write-Host "Processing $($zip.Name)..."
    $ext = Join-Path $WorkDir ("_unzip_" + $zip.BaseName)
    if (Test-Path $ext) { Remove-Item -Recurse -Force $ext }
    New-Item -ItemType Directory -Force -Path $ext | Out-Null
    Expand-Archive -Path $zip.FullName -DestinationPath $ext -Force
    $pe = @(Get-ChildItem $ext -Recurse -Include *.exe, *.dll -ErrorAction SilentlyContinue | Select-Object -ExpandProperty FullName)
    Write-Host "  signing $($pe.Count) PE file(s) inside $($zip.Name)..."
    Invoke-SignFiles -Files $pe
    Write-Host "  repacking $($zip.Name)..."
    Remove-Item -LiteralPath $zip.FullName -Force
    Compress-Archive -Path (Join-Path $ext '*') -DestinationPath $zip.FullName -Force
    Remove-Item -Recurse -Force $ext
  }
}

# Verify installer signatures.
Write-Host ""
Write-Host "Verifying installer signatures..."
foreach ($i in $installers) {
  & $SignTool verify /pa /q $i 2>$null
  $ok = ($LASTEXITCODE -eq 0)
  Write-Host ("  {0}  {1}" -f $(if ($ok) { 'OK  ' } else { 'FAIL' }), (Split-Path $i -Leaf))
}

# Upload signed artifacts back to the draft release.
if ($NoUpload) {
  Write-Host ""
  Write-Host "-NoUpload set — signed files left in $WorkDir (not uploaded)."
  return
}
Write-Host ""
Write-Host "Uploading signed artifacts to draft release $Version..."
foreach ($a in $Assets) {
  $p = Join-Path $WorkDir $a
  if (Test-Path $p) {
    & gh release upload $Version $p --repo $Repo --clobber
    if ($LASTEXITCODE -ne 0) { throw "gh release upload failed for '$a'." }
    Write-Host "  uploaded $a"
  }
}
Write-Host ""
Write-Host "Done. Signed Windows artifacts uploaded to draft release $Version."
