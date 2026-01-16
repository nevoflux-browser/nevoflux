#!/bin/bash
# Test extension with minimal sidebar to diagnose loading issues

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

EXTENSION_DIR="$PROJECT_ROOT/src/nevoflux/extensions/nevoflux-agent"
MANIFEST="$EXTENSION_DIR/manifest.json"

echo "Switching to minimal sidebar for testing..."

# Backup current manifest
cp "$MANIFEST" "$MANIFEST.backup"

# Update manifest to use minimal sidebar
sed -i 's/"default_panel": "sidebar\/sidebar.html"/"default_panel": "sidebar\/sidebar-minimal.html"/' "$MANIFEST"

echo "✓ Manifest updated to use sidebar-minimal.html"

# Repackage
bash "$SCRIPT_DIR/package-extension.sh"

echo ""
echo "✓ Extension repackaged with minimal sidebar"
echo ""
echo "Next steps:"
echo "1. Clear cache: rm -rf engine/obj-x86_64-pc-linux-gnu/tmp/profile-default/{extensions,startupCache}"
echo "2. Restart browser: npm run start"
echo "3. Check if sidebar shows '✅ NevoFlux Agent Loaded'"
echo ""
echo "To restore: mv $MANIFEST.backup $MANIFEST && bash scripts/package-extension.sh"
