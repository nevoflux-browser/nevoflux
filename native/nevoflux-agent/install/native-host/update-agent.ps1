# Update NevoFlux Agent binary in the browser's distribution directory.
#
# Usage:
#   .\update-agent.ps1                                    # auto-detect everything
#   .\update-agent.ps1 -BrowserDir "C:\path\to\nevoflux"
#   .\update-agent.ps1 -AgentPath "C:\path\to\nevoflux-agent.exe"
#   .\update-agent.ps1 -Download                          # force download from GitHub

param(
    [string]$AgentPath = "",
    [string]$BrowserDir = "",
    [switch]$Download
)

$ErrorActionPreference = "Stop"

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$ProjectRoot = (Resolve-Path "$ScriptDir\..\..").Path

$AgentBinName = "nevoflux-agent.exe"
$GitHubRepo = "dorisgyl/nevoflux-agent"

Write-Host "=== Update NevoFlux Agent ===" -ForegroundColor Cyan
Write-Host ""

# Detect architecture
function Get-Arch {
    $envArch = $env:PROCESSOR_ARCHITECTURE
    if ($envArch -eq "AMD64") { return "x86_64" }
    if ($envArch -eq "ARM64") { return "aarch64" }
    return "unknown"
}

# Download from GitHub
function Download-Agent {
    param([string]$TargetDir)

    $arch = Get-Arch
    if ($arch -eq "unknown") {
        Write-Host "Error: Unsupported architecture" -ForegroundColor Red
        return $false
    }

    $assetName = "nevoflux-agent-windows-${arch}.zip"

    try {
        $release = Invoke-RestMethod -Uri "https://api.github.com/repos/$GitHubRepo/releases/latest"
        $tag = $release.tag_name
    } catch {
        Write-Host "Error: Cannot determine latest release" -ForegroundColor Red
        return $false
    }

    $url = "https://github.com/$GitHubRepo/releases/download/$tag/$assetName"
    Write-Host "  Downloading $assetName ($tag)..."

    New-Item -ItemType Directory -Force -Path $TargetDir | Out-Null
    $archive = Join-Path $TargetDir $assetName

    try {
        Invoke-WebRequest -Uri $url -OutFile $archive
    } catch {
        Write-Host "Error: Download failed" -ForegroundColor Red
        return $false
    }

    Expand-Archive -Path $archive -DestinationPath $TargetDir -Force
    Remove-Item $archive -Force
    return (Test-Path (Join-Path $TargetDir $AgentBinName))
}

# Resolve agent binary directory
function Resolve-AgentDir {
    if ($Download) {
        $dlDir = Join-Path $env:TEMP "nevoflux-agent-download"
        if (Test-Path $dlDir) { Remove-Item $dlDir -Recurse -Force }
        if (Download-Agent -TargetDir $dlDir) { return $dlDir }
        return $null
    }

    # -AgentPath parameter
    if ($AgentPath -and (Test-Path $AgentPath)) {
        return (Split-Path -Parent (Resolve-Path $AgentPath).Path)
    }

    # Env var
    $envBin = $env:NEVOFLUX_AGENT_BIN
    if ($envBin -and (Test-Path $envBin)) {
        return (Split-Path -Parent (Resolve-Path $envBin).Path)
    }

    # Local build
    $devBin = Join-Path $ProjectRoot "target\release\$AgentBinName"
    if (Test-Path $devBin) {
        return (Join-Path $ProjectRoot "target\release")
    }

    # GitHub download
    Write-Host "  Binary not found locally, downloading..." -ForegroundColor Yellow
    $dlDir = Join-Path $env:TEMP "nevoflux-agent-download"
    if (Test-Path $dlDir) { Remove-Item $dlDir -Recurse -Force }
    if (Download-Agent -TargetDir $dlDir) { return $dlDir }

    return $null
}

Write-Host "[1/3] Locating agent binary..."
$AgentDir = Resolve-AgentDir
$AgentBin = if ($AgentDir) { Join-Path $AgentDir $AgentBinName } else { $null }

if (-not $AgentBin -or -not (Test-Path $AgentBin)) {
    Write-Host "Error: Agent binary not found" -ForegroundColor Red
    Write-Host ""
    Write-Host "Options:"
    Write-Host "  .\update-agent.ps1 -AgentPath C:\path\to\$AgentBinName"
    Write-Host "  .\update-agent.ps1 -Download"
    exit 1
}

Write-Host "  Agent: $AgentBin"
$modelsDir = Join-Path $AgentDir "models"
if (Test-Path $modelsDir) {
    Write-Host "  Models: $modelsDir"
}
Write-Host ""

# Resolve browser directory
function Find-BrowserDir {
    # -BrowserDir parameter
    if ($BrowserDir -and (Test-Path $BrowserDir)) {
        return $BrowserDir
    }

    # Env var
    if ($env:NEVOFLUX_BROWSER_DIR -and (Test-Path $env:NEVOFLUX_BROWSER_DIR)) {
        return $env:NEVOFLUX_BROWSER_DIR
    }

    # Dev build (adjacent nevoflux project)
    $devDirs = Get-ChildItem -Path "$ProjectRoot\..\nevoflux\engine" -Filter "obj-*-windows-msvc" -Directory -ErrorAction SilentlyContinue
    foreach ($d in $devDirs) {
        $distBin = Join-Path $d.FullName "dist\bin"
        if (Test-Path $distBin) { return $distBin }
    }

    # Common install locations
    $candidates = @(
        "$env:ProgramFiles\NevoFlux",
        "${env:ProgramFiles(x86)}\NevoFlux",
        "$env:LOCALAPPDATA\NevoFlux"
    )
    foreach ($dir in $candidates) {
        if (Test-Path $dir) { return $dir }
    }

    return $null
}

Write-Host "[2/3] Locating browser directory..."
$Browser = Find-BrowserDir

if (-not $Browser -or -not (Test-Path $Browser)) {
    Write-Host "Error: NevoFlux browser directory not found" -ForegroundColor Red
    Write-Host ""
    Write-Host "Specify with: .\update-agent.ps1 -BrowserDir C:\path\to\nevoflux"
    exit 1
}

$TargetDir = Join-Path $Browser "distribution\bin"
Write-Host "  Browser: $Browser"
Write-Host "  Target:  $TargetDir"
Write-Host ""

# Copy files
Write-Host "[3/3] Updating agent..."
New-Item -ItemType Directory -Force -Path $TargetDir | Out-Null

Copy-Item $AgentBin -Destination (Join-Path $TargetDir $AgentBinName) -Force
Write-Host "  Copied: $AgentBinName"

if (Test-Path $modelsDir) {
    Copy-Item $modelsDir -Destination $TargetDir -Recurse -Force
    Write-Host "  Copied: models\"
}

Write-Host ""
Write-Host "=== Update Complete ===" -ForegroundColor Green
Write-Host "  Restart NevoFlux browser to use the new agent."
Write-Host ""
