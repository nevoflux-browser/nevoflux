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

# Inject a unique version into manifest.json so Firefox always picks up updates.
# Priority: NEVOFLUX_EXT_VERSION env > surfer.json display version > date-based version.
# Firefox compares extension versions and only re-extracts when the version changes,
# so every build MUST have a distinct version string.
EXT_VERSION="${NEVOFLUX_EXT_VERSION:-}"
if [ -z "$EXT_VERSION" ]; then
  SURFER_JSON="$PROJECT_ROOT/surfer.json"
  if [ -f "$SURFER_JSON" ]; then
    # CI sets displayVersion via: surfer ci --display-version <version>
    EXT_VERSION=$(python3 - "$SURFER_JSON" <<'PYEOF'
import json, sys
s = json.load(open(sys.argv[1]))
brands = s.get('brands', {})
for b in brands.values():
    dv = b.get('release', {}).get('displayVersion', '')
    if dv and dv != '0.0.1' and dv != '0.0.1-dev':
        print(dv)
        break
PYEOF
    )
  fi
fi
if [ -z "$EXT_VERSION" ]; then
  # Fallback: date-based version (e.g. 0.2026.32706 from YYYY + DDDHH as minor.patch)
  EXT_VERSION="0.$(date -u +%Y).$(date -u +%j%H%M)"
fi
MANIFEST="$EXTENSION_DIR/manifest.json"
if [ -f "$MANIFEST" ]; then
  cp "$MANIFEST" "$MANIFEST.bak"
  echo "Injecting extension version: $EXT_VERSION"
  python3 - "$MANIFEST" "$EXT_VERSION" <<'PYEOF'
import json, sys
p, ver = sys.argv[1], sys.argv[2]
m = json.load(open(p))
m['version'] = ver
json.dump(m, open(p, 'w'), indent=2)
open(p, 'a').write('\n')
print('manifest.json version set to ' + ver)
PYEOF
else
  echo "⚠ Warning: manifest.json not found at $MANIFEST, skipping version injection"
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
  -x "package-lock.json" \
  -x "manifest.json.bak"

echo "✓ Extension packaged: $BUILD_DIR/$XPI_NAME"

# Restore original manifest.json to avoid dirtying git
if [ -f "$MANIFEST.bak" ]; then
  mv "$MANIFEST.bak" "$MANIFEST"
fi

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
