# Force NevoFlux Browser to use the built-in agent extension (already in
# omni.ja) by removing the distribution + profile XPI copies that shadow it
# and clearing the XPIProvider startup cache so the next launch re-scans.
#
# No reinstall needed: the installed browser's omni.ja already contains the
# built-in registration; we just have to remove the conflicting xpi sources.
#
# Usage (run from elevated PowerShell, needs admin to delete under
# Program Files):
#   .\scripts\activate-builtin-agent.ps1

$ErrorActionPreference = 'Stop'

# Make sure NevoFlux isn't running
$proc = Get-Process -Name 'nevoflux','firefox' -ErrorAction SilentlyContinue
if ($proc) {
    Write-Host "NevoFlux/firefox processes still running:"
    $proc | Format-Table -AutoSize
    $ans = Read-Host "Close them now? (y/N)"
    if ($ans -eq 'y') {
        $proc | Stop-Process -Force
        Start-Sleep -Seconds 2
    } else {
        Write-Error "Close NevoFlux completely first."
        exit 1
    }
}

# Need admin to delete under Program Files
$identity = [Security.Principal.WindowsIdentity]::GetCurrent()
$principal = New-Object Security.Principal.WindowsPrincipal($identity)
if (-not $principal.IsInRole([Security.Principal.WindowsBuiltInRole]::Administrator)) {
    Write-Error "Run from elevated PowerShell (right-click Run as administrator)."
    exit 1
}

# 1. Remove the installed-dir distribution xpi (the source of the shadowing).
$distXpi = 'C:\Program Files\NevoFlux Browser\distribution\extensions\agent@nevoflux.com.xpi'
if (Test-Path $distXpi) {
    Write-Host "Removing distribution xpi: $distXpi"
    Remove-Item -Force $distXpi
} else {
    Write-Host "Distribution xpi already absent: $distXpi"
}

# 2. Remove profile-installed copies (Firefox auto-copied from distribution on
#    first launch; these are what XPIProvider actually loaded last time).
$profilesRoot = "$env:APPDATA\Mozilla\NevoFlux\Profiles"
if (Test-Path $profilesRoot) {
    Get-ChildItem $profilesRoot -Directory | ForEach-Object {
        $profile = $_.FullName
        Write-Host ""
        Write-Host "Profile: $profile"
        # Profile xpi
        $profXpi = Join-Path $profile 'extensions\agent@nevoflux.com.xpi'
        if (Test-Path $profXpi) {
            Write-Host "  Removing profile xpi: $profXpi"
            Remove-Item -Force $profXpi
        }
        # Unpacked profile copy (if any)
        $profDir = Join-Path $profile 'extensions\agent@nevoflux.com'
        if (Test-Path $profDir) {
            Write-Host "  Removing profile unpacked dir: $profDir"
            Remove-Item -Recurse -Force $profDir
        }
        # XPIProvider's cached add-on startup state. Without removing this,
        # Firefox reuses cached add-on locations and may keep referencing
        # the now-deleted xpi.
        $addonStartup = Join-Path $profile 'addonStartup.json.lz4'
        if (Test-Path $addonStartup) {
            Write-Host "  Removing addon startup cache: addonStartup.json.lz4"
            Remove-Item -Force $addonStartup
        }
        # General startup cache (compiled chrome / scripts). Safer to clear
        # too so any cached references to the old xpi path are gone.
        $startupCache = Join-Path $profile 'startupCache'
        if (Test-Path $startupCache) {
            Write-Host "  Removing startupCache/"
            Remove-Item -Recurse -Force $startupCache
        }
        # extensions.json: XPIProvider's database. If it remembers the old
        # xpi by path, it may try to re-resolve. Safer to nuke.
        $extDb = Join-Path $profile 'extensions.json'
        if (Test-Path $extDb) {
            Write-Host "  Removing extensions.json (XPIProvider DB, will be rebuilt)"
            Remove-Item -Force $extDb
        }
    }
}

Write-Host ""
Write-Host "Done. Next launch of NevoFlux will:"
Write-Host "  - Rebuild addon database from scratch"
Write-Host "  - See no profile/distribution xpi for agent@nevoflux.com"
Write-Host "  - Pick up the built-in registration from"
Write-Host "    omni.ja:chrome/browser/content/built_in_addons.json"
Write-Host "  - Load NevoFlux Agent from resource://builtin-addons/nevoflux-agent/"
Write-Host ""
Write-Host "Verify in about:debugging#/runtime/this-firefox after launch:"
Write-Host "  Manifest URL should be resource://builtin-addons/nevoflux-agent/manifest.json"
