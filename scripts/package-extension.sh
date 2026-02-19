#!/bin/bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Package NevoFlux Agent extension as XPI for distribution
# Usage: ./scripts/package-extension.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

EXTENSION_DIR="$PROJECT_ROOT/src/nevoflux/extensions/nevoflux-agent"
BUILD_DIR="$PROJECT_ROOT/build/AppDir/distribution/extensions"
EXTENSION_ID="agent@nevoflux.com"
XPI_NAME="${EXTENSION_ID}.xpi"

echo "Packaging NevoFlux Agent extension..."

# Verify extension directory exists
if [ ! -d "$EXTENSION_DIR" ]; then
  echo "Error: Extension directory not found at $EXTENSION_DIR"
  exit 1
fi

# Copy WASM build output from dist/ to wasm/ (if dist exists)
DIOXUS_DIST="$EXTENSION_DIR/dioxus-ui/dist/chat-sidebar"
WASM_DIR="$EXTENSION_DIR/wasm/chat-sidebar"
FIX_CSP_SCRIPT="$EXTENSION_DIR/dioxus-ui/scripts/fix-csp.py"

if [ -d "$DIOXUS_DIST" ]; then
  # Run CSP fix on dist directory first
  if [ -f "$FIX_CSP_SCRIPT" ]; then
    echo "Fixing CSP issues in dist/..."
    python3 "$FIX_CSP_SCRIPT" "$DIOXUS_DIST"
  fi

  echo "Copying WASM build output from dist/ to wasm/..."
  cp -r "$DIOXUS_DIST/"* "$WASM_DIR/"
  echo "✓ WASM files copied"
fi

# Create output directory
mkdir -p "$BUILD_DIR"

# Create XPI (which is just a ZIP file)
cd "$EXTENSION_DIR"

# Remove old XPI if exists
rm -f "$BUILD_DIR/$XPI_NAME"

# Package extension (excluding development files and build artifacts)
zip -r "$BUILD_DIR/$XPI_NAME" . \
  -x "*.git*" \
  -x "*.DS_Store" \
  -x "*.md" \
  -x "*node_modules*" \
  -x "*.log" \
  -x "dioxus-ui/target/*" \
  -x "dioxus-ui/dist/*" \
  -x "dioxus-ui/Cargo.lock" \
  -x "dioxus-ui/chat-sidebar/target/*" \
  -x "dioxus-ui/content-sidebar/target/*" \
  -x "dioxus-ui/shared-protocol/target/*" \
  -x "dioxus-ui/.cargo/*" \
  -x "package-lock.json"

echo "✓ Extension packaged: $BUILD_DIR/$XPI_NAME"

# Copy to engine directory if it exists
source "$SCRIPT_DIR/lib/detect-objdir.sh"
ENGINE_EXT_DIR="$DIST_DIR/distribution/extensions"
if [ -d "$ENGINE_EXT_DIR" ]; then
  echo "Copying to engine directory..."
  cp "$BUILD_DIR/$XPI_NAME" "$ENGINE_EXT_DIR/$XPI_NAME"
  echo "✓ Extension copied to: $ENGINE_EXT_DIR/$XPI_NAME"
else
  echo "⚠ Warning: Engine directory not found. Run 'npm run build' first."
  echo "   Manual copy needed: cp $BUILD_DIR/$XPI_NAME $ENGINE_EXT_DIR/"
fi

# Verify the package
echo ""
echo "Package contents:"
unzip -l "$BUILD_DIR/$XPI_NAME" | head -20

echo ""
echo "Done! Extension is ready for distribution."
echo "To test: Restart browser or reload extension in about:debugging"
