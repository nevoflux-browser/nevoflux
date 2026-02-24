#!/bin/bash
# Apply existing nevoflux patches to src/zen/ for development
# Use this before making further modifications to already-patched files
# Usage: ./scripts/apply-nevoflux-patches-dev.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
PATCHES_DIR="$PROJECT_ROOT/src/nevoflux/patches"
ZEN_DIR="$PROJECT_ROOT/src/zen"

cd "$PROJECT_ROOT"

# Check if patches directory exists
if [ ! -d "$PATCHES_DIR" ]; then
  echo "No patches directory found at $PATCHES_DIR"
  exit 0
fi

# Check for existing uncommitted changes in src/zen/
EXISTING_CHANGES=$(git diff --name-only src/zen/)
if [ -n "$EXISTING_CHANGES" ]; then
  echo "WARNING: src/zen/ has uncommitted changes:"
  echo "$EXISTING_CHANGES"
  echo ""
  read -p "Continue anyway? Patches will be applied on top of existing changes. [y/N] " -n 1 -r
  echo
  if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Aborted."
    exit 1
  fi
fi

echo "Applying existing nevoflux patches to src/zen/ for development..."
echo ""

# Find and apply all patches
PATCH_COUNT=0
FAILED_COUNT=0

find "$PATCHES_DIR" -type f -name "*.patch" 2> /dev/null | while read -r patch_file; do
  echo "Applying: ${patch_file#$PROJECT_ROOT/}"

  if (cd "$ZEN_DIR" && git apply --ignore-whitespace "$patch_file" 2> /dev/null); then
    echo "  ✓ Success"
  else
    # Try with --3way for conflicts
    if (cd "$ZEN_DIR" && git apply --3way "$patch_file" 2> /dev/null); then
      echo "  ✓ Success (with 3-way merge)"
    else
      echo "  ✗ FAILED - patch may need manual update"
      ((FAILED_COUNT++)) || true
    fi
  fi
  ((PATCH_COUNT++)) || true
done

echo ""
echo "Done. You can now edit src/zen/ files."
echo ""
echo "After making changes:"
echo "  1. Test: npm run build:ui && npm run start"
echo "  2. Export: ./scripts/export-nevoflux-patches.sh"
echo "  3. Revert: ./scripts/revert-zen-changes.sh"
