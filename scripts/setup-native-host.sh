#!/bin/bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Setup NevoFlux Native Messaging Host
# This script locates/builds the native agent and registers it with Firefox/NevoFlux
#
# Binary resolution order:
#   1. Command line argument: ./setup-native-host.sh /path/to/binary
#   2. Environment variable: NEVOFLUX_AGENT_BIN=/path/to/binary
#   3. Monorepo development: native/nevoflux-agent/target/release/nevoflux-agent
#   4. Legacy sibling checkout: ../nevoflux-agent/target/release/nevoflux-agent
#   5. GitHub release download (for release builds)

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Configuration
AGENT_PROJECT="$PROJECT_ROOT/native/nevoflux-agent"
LEGACY_AGENT_PROJECT="$PROJECT_ROOT/../nevoflux-agent"
AGENT_BIN_NAME="nevoflux-agent"
EXTENSION_ID="agent@nevoflux.com"
MANIFEST_NAME="com.nevoflux.agent"

# GitHub release configuration (for future use)
GITHUB_REPO="dorisgyl/nevoflux-agent"
GITHUB_RELEASE_TAG="latest"

echo "=== NevoFlux Native Messaging Host Setup ==="
echo ""

# Function to download from GitHub releases
download_from_github() {
  local target_path="$1"
  local os_type=""
  local arch=""

  # Detect OS
  case "$(uname -s)" in
    Linux*) os_type="linux" ;;
    Darwin*) os_type="macos" ;;
    *)
      echo "Error: Unsupported OS"
      exit 1
      ;;
  esac

  # Detect architecture
  case "$(uname -m)" in
    x86_64) arch="x86_64" ;;
    aarch64) arch="aarch64" ;;
    arm64) arch="aarch64" ;;
    *)
      echo "Error: Unsupported architecture"
      exit 1
      ;;
  esac

  local asset_name="${AGENT_BIN_NAME}-${os_type}-${arch}"
  local download_url="https://github.com/${GITHUB_REPO}/releases/${GITHUB_RELEASE_TAG}/download/${asset_name}"

  echo "Downloading from: $download_url"

  mkdir -p "$(dirname "$target_path")"

  if command -v curl &> /dev/null; then
    curl -fsSL -o "$target_path" "$download_url"
  elif command -v wget &> /dev/null; then
    wget -q -O "$target_path" "$download_url"
  else
    echo "Error: Neither curl nor wget is available"
    exit 1
  fi

  chmod +x "$target_path"
  echo "Downloaded to: $target_path"
}

# Resolve binary path
resolve_binary() {
  # Priority 1: Command line argument
  if [ -n "$1" ] && [ -f "$1" ]; then
    echo "$1"
    return 0
  fi

  # Priority 2: Environment variable
  if [ -n "$NEVOFLUX_AGENT_BIN" ] && [ -f "$NEVOFLUX_AGENT_BIN" ]; then
    echo "$NEVOFLUX_AGENT_BIN"
    return 0
  fi

  # Priority 3: Monorepo development build
  local dev_binary="$AGENT_PROJECT/target/release/$AGENT_BIN_NAME"
  if [ -f "$dev_binary" ]; then
    echo "$dev_binary"
    return 0
  fi

  # Priority 4: Legacy sibling checkout
  local legacy_dev_binary="$LEGACY_AGENT_PROJECT/target/release/$AGENT_BIN_NAME"
  if [ -f "$legacy_dev_binary" ]; then
    echo "$legacy_dev_binary"
    return 0
  fi

  # Priority 5: Download from GitHub (for release builds)
  # Uncomment when GitHub releases are available
  # local download_path="$PROJECT_ROOT/build/nevoflux-agent/$AGENT_BIN_NAME"
  # echo "Binary not found locally. Downloading from GitHub releases..."
  # download_from_github "$download_path"
  # echo "$download_path"
  # return 0

  return 1
}

# Get binary path
AGENT_BIN=$(resolve_binary "$1")

if [ -z "$AGENT_BIN" ] || [ ! -f "$AGENT_BIN" ]; then
  echo "Error: Native agent binary not found"
  echo ""
  echo "Expected location: $AGENT_PROJECT/target/release/$AGENT_BIN_NAME"
  echo ""
  echo "Options:"
  echo "  1. Build agent first:  cargo build --release --manifest-path $AGENT_PROJECT/Cargo.toml --bin $AGENT_BIN_NAME"
  echo "  2. Specify path:       ./setup-native-host.sh /path/to/nevoflux-agent"
  echo "  3. Set env var:        NEVOFLUX_AGENT_BIN=/path/to/binary ./setup-native-host.sh"
  exit 1
fi

# Convert to absolute path
if [[ "$AGENT_BIN" != /* ]]; then
  AGENT_BIN="$(cd "$(dirname "$AGENT_BIN")" && pwd)/$(basename "$AGENT_BIN")"
fi

echo "Using binary: $AGENT_BIN"
echo ""

# Register native messaging host (platform-specific)
echo "[2/3] Registering native messaging host..."

PLATFORM="$(uname -s)"
case "$PLATFORM" in
  Linux*)
    MANIFEST_DIR="$HOME/.mozilla/native-messaging-hosts"
    ;;
  Darwin*)
    MANIFEST_DIR="$HOME/Library/Application Support/Mozilla/NativeMessagingHosts"
    ;;
  MINGW* | MSYS* | CYGWIN*)
    # Windows: manifest goes to a local directory, registered via Registry
    MANIFEST_DIR="$APPDATA/Mozilla/NativeMessagingHosts"
    ;;
  *)
    echo "Error: Unsupported platform: $PLATFORM"
    exit 1
    ;;
esac

mkdir -p "$MANIFEST_DIR"

# Write manifest JSON (same format on all platforms)
MANIFEST_FILE="$MANIFEST_DIR/${MANIFEST_NAME}.json"
cat > "$MANIFEST_FILE" << EOF
{
  "name": "$MANIFEST_NAME",
  "description": "NevoFlux AI Agent Native Messaging Host",
  "path": "$AGENT_BIN",
  "type": "stdio",
  "allowed_extensions": ["$EXTENSION_ID"]
}
EOF

echo "Manifest created: $MANIFEST_FILE"

# Windows: also register in the Registry
case "$PLATFORM" in
  MINGW* | MSYS* | CYGWIN*)
    # Convert to Windows path for registry
    WIN_MANIFEST="$(cygpath -w "$MANIFEST_FILE" 2> /dev/null || echo "$MANIFEST_FILE")"
    REG_KEY="HKCU\\Software\\Mozilla\\NativeMessagingHosts\\${MANIFEST_NAME}"
    reg add "$REG_KEY" /ve /t REG_SZ /d "$WIN_MANIFEST" /f > /dev/null 2>&1 \
      && echo "Registry key created: $REG_KEY" \
      || echo "Warning: Failed to create registry key. You may need to run as administrator."
    ;;
esac

echo ""

# Verify installation
echo "[3/3] Verifying installation..."
echo "  Binary:   $AGENT_BIN"
echo "  Manifest: $MANIFEST_FILE"
echo ""

# Test agent executable
if "$AGENT_BIN" --help > /dev/null 2>&1; then
  echo "Agent is executable"
else
  echo "Warning: Could not execute agent. You may need to set execute permissions:"
  echo "  chmod +x $AGENT_BIN"
fi

echo ""
echo "=== Setup Complete ==="
echo ""
echo "Next steps:"
echo "  1. Run: npm run start:full"
echo "  2. Open browser and press Ctrl+Shift+A to open the NevoFlux sidebar"
echo "  3. Check the browser console (F12) for extension messages"
echo ""
echo "Note: AI functionality requires API keys. Configure them in:"
case "$PLATFORM" in
  MINGW* | MSYS* | CYGWIN*)
    echo "  %APPDATA%\\nevoflux\\config.toml"
    ;;
  *)
    echo "  ~/.config/nevoflux/config.toml"
    ;;
esac
echo ""
