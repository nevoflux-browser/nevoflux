#!/bin/bash
# Copy NevoFlux distribution files to the final build output
# This should be run after the build completes

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

SOURCE_DIST="$PROJECT_ROOT/build/AppDir/distribution"
source "$SCRIPT_DIR/lib/detect-objdir.sh"
TARGET_DIST="$DIST_DIR/distribution"

echo "Copying NevoFlux distribution files to build output..."

# Check if source exists
if [ ! -d "$SOURCE_DIST" ]; then
  echo "Error: Source distribution directory not found at $SOURCE_DIST"
  exit 1
fi

# Create target directory
mkdir -p "$TARGET_DIST"

# Copy distribution files
echo "Copying: $SOURCE_DIST -> $TARGET_DIST"
cp -r "$SOURCE_DIST/"* "$TARGET_DIST/"

echo "✓ Distribution files copied successfully"
echo ""
echo "Extension XPI location: $TARGET_DIST/extensions/agent@nevoflux.com.xpi"
echo "Policies file location: $TARGET_DIST/policies.json"
