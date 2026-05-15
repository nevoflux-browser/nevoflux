#!/bin/bash
# Update NevoFlux Agent binary in the browser's distribution directory.
#
# Usage:
#   ./update-agent.sh                          # auto-detect everything
#   ./update-agent.sh --browser /path/to/nevoflux
#   ./update-agent.sh --agent /path/to/nevoflux-agent
#   ./update-agent.sh --download               # force download from GitHub
#
# Agent binary resolution:
#   1. --agent argument
#   2. NEVOFLUX_AGENT_BIN env var
#   3. Local build: ../../target/release/nevoflux-agent
#   4. GitHub release download (latest from dorisgyl/nevoflux-agent)
#
# Browser resolution:
#   1. --browser argument
#   2. NEVOFLUX_BROWSER_DIR env var
#   3. Auto-detect from common install locations

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

AGENT_BIN_NAME="nevoflux-agent"
GITHUB_REPO="dorisgyl/nevoflux-agent"

# Parse arguments
AGENT_PATH=""
BROWSER_DIR=""
FORCE_DOWNLOAD=false

while [[ $# -gt 0 ]]; do
  case $1 in
    --agent)   AGENT_PATH="$2"; shift 2 ;;
    --browser) BROWSER_DIR="$2"; shift 2 ;;
    --download) FORCE_DOWNLOAD=true; shift ;;
    -h|--help)
      echo "Usage: $0 [--agent PATH] [--browser PATH] [--download]"
      echo ""
      echo "  --agent PATH     Path to nevoflux-agent binary"
      echo "  --browser PATH   Path to NevoFlux browser install directory"
      echo "  --download       Force download latest from GitHub"
      exit 0
      ;;
    *) AGENT_PATH="$1"; shift ;;
  esac
done

# Detect platform
PLATFORM="$(uname -s)"
case "$PLATFORM" in
  Linux*)  OS="linux" ;;
  Darwin*) OS="macos" ;;
  MINGW* | MSYS* | CYGWIN*) OS="windows" ;;
  *) echo "Error: Unsupported platform: $PLATFORM"; exit 1 ;;
esac

ARCH="$(uname -m)"
case "$ARCH" in
  x86_64 | amd64)  ARCH="x86_64" ;;
  aarch64 | arm64)  ARCH="aarch64" ;;
  *) echo "Error: Unsupported architecture: $ARCH"; exit 1 ;;
esac

BIN_SUFFIX=""
[ "$OS" = "windows" ] && BIN_SUFFIX=".exe"

echo "=== Update NevoFlux Agent ==="
echo ""

# --- Resolve agent binary ---

download_agent() {
  local dir="$1"
  local asset="${AGENT_BIN_NAME}-${OS}-${ARCH}"
  local ext="tar.gz"
  [ "$OS" = "windows" ] && ext="zip"

  # Get latest tag
  local tag=""
  if command -v gh &> /dev/null; then
    tag=$(gh release view --repo "$GITHUB_REPO" --json tagName --jq '.tagName' 2>/dev/null || true)
  fi
  if [ -z "$tag" ]; then
    tag=$(curl -fsSL "https://api.github.com/repos/${GITHUB_REPO}/releases/latest" 2>/dev/null \
      | grep '"tag_name"' | head -1 | sed 's/.*"\(v[^"]*\)".*/\1/')
  fi
  [ -z "$tag" ] && { echo "Error: Cannot determine latest release"; return 1; }

  local url="https://github.com/${GITHUB_REPO}/releases/download/${tag}/${asset}.${ext}"
  echo "  Downloading ${asset}.${ext} (${tag})..."

  mkdir -p "$dir"
  local archive="$dir/${asset}.${ext}"

  curl -fsSL -o "$archive" "$url" || wget -q -O "$archive" "$url"

  if [ "$ext" = "tar.gz" ]; then
    tar -xzf "$archive" -C "$dir"
  else
    unzip -o "$archive" -d "$dir" 2>/dev/null || \
      powershell -Command "Expand-Archive -Path '$archive' -DestinationPath '$dir' -Force"
  fi
  rm -f "$archive"

  [ -f "$dir/${AGENT_BIN_NAME}${BIN_SUFFIX}" ] && chmod +x "$dir/${AGENT_BIN_NAME}${BIN_SUFFIX}"
}

resolve_agent() {
  if [ "$FORCE_DOWNLOAD" = true ]; then
    local dl_dir="/tmp/nevoflux-agent-download"
    rm -rf "$dl_dir"
    download_agent "$dl_dir"
    echo "$dl_dir"
    return
  fi

  # --agent argument
  if [ -n "$AGENT_PATH" ] && [ -f "$AGENT_PATH" ]; then
    echo "$(dirname "$(cd "$(dirname "$AGENT_PATH")" && pwd)/$(basename "$AGENT_PATH")")"
    return
  fi

  # Env var
  if [ -n "$NEVOFLUX_AGENT_BIN" ] && [ -f "$NEVOFLUX_AGENT_BIN" ]; then
    echo "$(dirname "$NEVOFLUX_AGENT_BIN")"
    return
  fi

  # Local build
  local dev="$PROJECT_ROOT/target/release/${AGENT_BIN_NAME}${BIN_SUFFIX}"
  if [ -f "$dev" ]; then
    echo "$PROJECT_ROOT/target/release"
    return
  fi

  # GitHub download
  local dl_dir="/tmp/nevoflux-agent-download"
  echo "  Binary not found locally, downloading..." >&2
  download_agent "$dl_dir"
  echo "$dl_dir"
}

echo "[1/3] Locating agent binary..."
AGENT_DIR=$(resolve_agent)
AGENT_BIN="$AGENT_DIR/${AGENT_BIN_NAME}${BIN_SUFFIX}"

if [ ! -f "$AGENT_BIN" ]; then
  echo "Error: Agent binary not found"
  exit 1
fi

echo "  Agent: $AGENT_BIN"
[ -d "$AGENT_DIR/models" ] && echo "  Models: $AGENT_DIR/models"
echo ""

# --- Resolve browser directory ---

find_browser_dir() {
  # --browser argument
  if [ -n "$BROWSER_DIR" ]; then
    echo "$BROWSER_DIR"
    return
  fi

  # Env var
  if [ -n "$NEVOFLUX_BROWSER_DIR" ]; then
    echo "$NEVOFLUX_BROWSER_DIR"
    return
  fi

  # Auto-detect
  case "$OS" in
    linux)
      # Dev build (adjacent nevoflux project)
      for obj in "$PROJECT_ROOT/../nevoflux/engine"/obj-*-linux-gnu/dist/bin; do
        [ -d "$obj" ] && { echo "$obj"; return; }
      done
      # Common install locations
      for dir in /opt/nevoflux /usr/lib/nevoflux /usr/lib64/nevoflux "$HOME/.local/share/nevoflux"; do
        [ -d "$dir" ] && { echo "$dir"; return; }
      done
      ;;
    macos)
      local app="/Applications/NevoFlux.app/Contents/Resources"
      [ -d "$app" ] && { echo "$app"; return; }
      # Dev build
      for obj in "$PROJECT_ROOT/../nevoflux/engine"/obj-*-apple-darwin/dist/bin; do
        [ -d "$obj" ] && { echo "$obj"; return; }
      done
      ;;
    windows)
      # Dev build
      for obj in "$PROJECT_ROOT/../nevoflux/engine"/obj-*-windows-msvc/dist/bin; do
        [ -d "$obj" ] && { echo "$obj"; return; }
      done
      # Common install locations
      for dir in "$PROGRAMFILES/NevoFlux" "$LOCALAPPDATA/NevoFlux"; do
        [ -d "$dir" ] && { echo "$dir"; return; }
      done
      ;;
  esac

  return 1
}

echo "[2/3] Locating browser directory..."
BROWSER=$(find_browser_dir)

if [ -z "$BROWSER" ] || [ ! -d "$BROWSER" ]; then
  echo "Error: NevoFlux browser directory not found"
  echo ""
  echo "Specify with: $0 --browser /path/to/nevoflux"
  exit 1
fi

TARGET_DIR="$BROWSER/distribution/bin"
echo "  Browser: $BROWSER"
echo "  Target:  $TARGET_DIR"
echo ""

# --- Copy files ---

echo "[3/3] Updating agent..."
mkdir -p "$TARGET_DIR"

cp "$AGENT_BIN" "$TARGET_DIR/${AGENT_BIN_NAME}${BIN_SUFFIX}"
chmod +x "$TARGET_DIR/${AGENT_BIN_NAME}${BIN_SUFFIX}"
echo "  Copied: ${AGENT_BIN_NAME}${BIN_SUFFIX}"

if [ -d "$AGENT_DIR/models" ]; then
  cp -r "$AGENT_DIR/models" "$TARGET_DIR/"
  echo "  Copied: models/"
fi

echo ""
echo "=== Update Complete ==="
echo "  Restart NevoFlux browser to use the new agent."
echo ""
