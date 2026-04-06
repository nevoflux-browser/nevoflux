#!/bin/bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# Reload NevoFlux Agent extension with cache clearing
# Usage: ./scripts/reload-extension.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "🔄 Reloading NevoFlux Agent extension..."

# Step 1: Package the extension
echo ""
echo "📦 Packaging extension..."
"$SCRIPT_DIR/package-extension.sh"

# Step 2: Clear extension caches
source "$SCRIPT_DIR/lib/detect-objdir.sh"
PROFILE_DIR="$OBJ_DIR/tmp/profile-default"

if [ -d "$PROFILE_DIR" ]; then
  echo ""
  echo "🧹 Clearing extension caches..."

  # Clear startup cache (most important)
  if [ -d "$PROFILE_DIR/startupCache" ]; then
    rm -rf "$PROFILE_DIR/startupCache"
    echo "  ✓ Cleared startupCache/"
  fi

  # Clear addon startup cache (critical for extension updates)
  if [ -f "$PROFILE_DIR/addonStartup.json.lz4" ]; then
    rm -f "$PROFILE_DIR/addonStartup.json.lz4"
    echo "  ✓ Cleared addonStartup.json.lz4"
  fi

  # Clear extension database (forces re-scan)
  if [ -f "$PROFILE_DIR/extensions.json" ]; then
    rm -f "$PROFILE_DIR/extensions.json"
    echo "  ✓ Cleared extensions.json"
  fi

  # Clear any copied extension files in profile
  if [ -d "$PROFILE_DIR/extensions" ]; then
    rm -f "$PROFILE_DIR/extensions"/*.xpi
    echo "  ✓ Cleared profile extensions/*.xpi"
  fi

  # Clear distribution addon install markers so Firefox re-installs from distribution/
  if [ -f "$PROFILE_DIR/prefs.js" ]; then
    sed -i '/extensions\.installedDistroAddon\./d' "$PROFILE_DIR/prefs.js"
    echo "  ✓ Cleared distribution addon install markers from prefs.js"
  fi

  echo "✓ All caches cleared"
else
  echo "⚠ Warning: Profile directory not found. Browser hasn't run yet?"
fi

echo ""
echo "✅ Extension reloaded and caches cleared!"
echo ""
echo "Now run: npm run start"
echo ""
echo "The extension will be loaded with your latest changes."
