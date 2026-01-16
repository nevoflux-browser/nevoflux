#!/bin/bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Setup NevoFlux Native Messaging Host
# This script builds the Rust agent and registers it with Firefox/NevoFlux

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== NevoFlux Native Messaging Host Setup ==="
echo ""

# 1. Build Rust agent
echo "[1/3] Building Rust agent..."
cd "$PROJECT_ROOT/src/nevoflux/crates"
cargo build --release

# 2. Get absolute path
AGENT_BIN="$PROJECT_ROOT/src/nevoflux/crates/target/release/nevoflux-agent"
if [ ! -f "$AGENT_BIN" ]; then
    echo "Error: Agent binary not found at $AGENT_BIN"
    echo "Please ensure the build completed successfully"
    exit 1
fi

echo "✓ Agent built successfully"
echo ""

# 3. Create manifest directory
echo "[2/3] Creating native messaging manifest..."
MANIFEST_DIR="$HOME/.mozilla/native-messaging-hosts"
mkdir -p "$MANIFEST_DIR"

# 4. Write manifest
MANIFEST_FILE="$MANIFEST_DIR/com.nevoflux.agent.json"
cat > "$MANIFEST_FILE" <<EOF
{
  "name": "com.nevoflux.agent",
  "description": "NevoFlux AI Agent Native Messaging Host",
  "path": "$AGENT_BIN",
  "type": "stdio",
  "allowed_extensions": ["agent@nevoflux.com"]
}
EOF

echo "✓ Manifest created"
echo ""

# 5. Verify installation
echo "[3/3] Verifying installation..."
echo "  Agent binary: $AGENT_BIN"
echo "  Manifest: $MANIFEST_FILE"
echo ""

# Test agent executable
if "$AGENT_BIN" --help > /dev/null 2>&1; then
    echo "✓ Agent is executable"
else
    echo "Warning: Could not execute agent. You may need to set execute permissions:"
    echo "  chmod +x $AGENT_BIN"
fi

echo ""
echo "=== Setup Complete ==="
echo ""
echo "Next steps:"
echo "  1. Run: npm run start"
echo "  2. Open browser and press Ctrl+Shift+A to open the NevoFlux sidebar"
echo "  3. Check the browser console (F12) for extension messages"
echo ""
echo "Note: AI functionality requires API keys. Configure them in:"
echo "  ~/.config/nevoflux/config.toml"
echo ""
