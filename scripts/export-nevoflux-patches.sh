#!/bin/bash
# Export all modified src/zen/ files as patches to src/nevoflux/patches/
# Usage: ./scripts/export-nevoflux-patches.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PATCHES_DIR="$PROJECT_ROOT/src/nevoflux/patches"

cd "$PROJECT_ROOT"

echo "Scanning for modified files in src/zen/..."

# Find all modified files under src/zen/
MODIFIED_FILES=$(git diff --name-only src/zen/)

if [ -z "$MODIFIED_FILES" ]; then
  echo "No modified files found in src/zen/"
  exit 0
fi

echo "Found modified files:"
echo "$MODIFIED_FILES"
echo ""

# Generate patches for each modified file
for file in $MODIFIED_FILES; do
  # Calculate patch path: src/zen/common/modules/ZenStartup.mjs -> common/modules/ZenStartup-mjs.patch
  RELATIVE_PATH="${file#src/zen/}"
  PATCH_SUBDIR="$(dirname "$RELATIVE_PATH")"
  FILENAME="$(basename "$RELATIVE_PATH" | tr '.' '-').nfpatch"

  PATCH_DIR="$PATCHES_DIR/$PATCH_SUBDIR"
  PATCH_FILE="$PATCH_DIR/$FILENAME"

  # Create patch directory
  mkdir -p "$PATCH_DIR"

  # Generate patch
  git diff "$file" > "$PATCH_FILE"

  echo "✓ Created: $PATCH_FILE"
done

echo ""
echo "All patches exported to src/nevoflux/patches/"
echo "Run './scripts/revert-zen-changes.sh' to revert src/zen/ to original state."
