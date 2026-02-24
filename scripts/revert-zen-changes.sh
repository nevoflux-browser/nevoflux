#!/bin/bash
# Revert all changes in src/zen/ to original state
# This script handles both:
#   1. Files copied from src/nevoflux/overlays/ (will be deleted)
#   2. Files modified by patches (will be reverted via git checkout)
#
# Usage: ./scripts/revert-zen-changes.sh [-y]
#   -y: Skip confirmation prompt

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
OVERLAYS_DIR="$PROJECT_ROOT/src/nevoflux/overlays"
ZEN_DIR="$PROJECT_ROOT/src/zen"

cd "$PROJECT_ROOT"

# Parse arguments
AUTO_CONFIRM=false
if [[ "$1" == "-y" ]]; then
  AUTO_CONFIRM=true
fi

echo "Checking src/zen/ for changes to revert..."
echo ""

# 1. Find overlay files that need to be removed from src/zen/
OVERLAY_FILES=()
if [ -d "$OVERLAYS_DIR" ]; then
  while IFS= read -r -d '' file; do
    # Get relative path from overlays dir
    relative_path="${file#$OVERLAYS_DIR/}"
    target_file="$ZEN_DIR/$relative_path"

    if [ -f "$target_file" ]; then
      OVERLAY_FILES+=("$relative_path")
    fi
  done < <(find "$OVERLAYS_DIR" -type f -print0)
fi

# 2. Find modified tracked files in src/zen/
MODIFIED_FILES=$(git diff --name-only src/zen/ 2> /dev/null || true)

# 3. Find staged files in src/zen/
STAGED_FILES=$(git diff --cached --name-only src/zen/ 2> /dev/null || true)

# Check if there's anything to do
if [ ${#OVERLAY_FILES[@]} -eq 0 ] && [ -z "$MODIFIED_FILES" ] && [ -z "$STAGED_FILES" ]; then
  echo "✓ No changes found in src/zen/ - already clean."
  exit 0
fi

# Display what will be done
if [ ${#OVERLAY_FILES[@]} -gt 0 ]; then
  echo "Files to DELETE (copied from overlays):"
  for f in "${OVERLAY_FILES[@]}"; do
    echo "  - src/zen/$f"
  done
  echo ""
fi

if [ -n "$MODIFIED_FILES" ]; then
  echo "Files to REVERT (modified by patches):"
  echo "$MODIFIED_FILES" | sed 's/^/  - /'
  echo ""
fi

if [ -n "$STAGED_FILES" ]; then
  echo "Files to UNSTAGE and REVERT (staged changes):"
  echo "$STAGED_FILES" | sed 's/^/  - /'
  echo ""
fi

# Confirm action
if [ "$AUTO_CONFIRM" = false ]; then
  read -p "Are you sure you want to revert these changes? [y/N] " -n 1 -r
  echo
  if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Aborted."
    exit 0
  fi
fi

# Execute cleanup
ERRORS=0

# 1. Delete overlay files from src/zen/
for f in "${OVERLAY_FILES[@]}"; do
  target_file="$ZEN_DIR/$f"
  if [ -f "$target_file" ]; then
    rm -f "$target_file"
    echo "Deleted: src/zen/$f"

    # Remove empty parent directories
    parent_dir=$(dirname "$target_file")
    while [ "$parent_dir" != "$ZEN_DIR" ] && [ -d "$parent_dir" ] && [ -z "$(ls -A "$parent_dir")" ]; do
      rmdir "$parent_dir"
      echo "Removed empty directory: ${parent_dir#$PROJECT_ROOT/}"
      parent_dir=$(dirname "$parent_dir")
    done
  fi
done

# 2. Unstage any staged files in src/zen/
if [ -n "$STAGED_FILES" ]; then
  git restore --staged src/zen/ 2> /dev/null || true
  echo "Unstaged files in src/zen/"
fi

# 3. Revert modified files
if [ -n "$MODIFIED_FILES" ] || [ -n "$STAGED_FILES" ]; then
  git checkout src/zen/ 2> /dev/null || {
    echo "WARN: Some files could not be reverted via git checkout"
    ERRORS=1
  }
  echo "Reverted modified files in src/zen/"
fi

echo ""
if [ $ERRORS -eq 0 ]; then
  echo "✓ All changes in src/zen/ have been reverted."
else
  echo "⚠ Completed with warnings. Please check src/zen/ status."
fi
