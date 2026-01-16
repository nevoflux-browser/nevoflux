#!/bin/bash
# Revert all changes in src/zen/ to original state
# Usage: ./scripts/revert-zen-changes.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$PROJECT_ROOT"

echo "Checking for modified files in src/zen/..."

# Find all modified files under src/zen/
MODIFIED_FILES=$(git diff --name-only src/zen/)

if [ -z "$MODIFIED_FILES" ]; then
  echo "No modified files found in src/zen/"
  exit 0
fi

echo "The following files will be reverted:"
echo "$MODIFIED_FILES"
echo ""

read -p "Are you sure you want to revert these changes? [y/N] " -n 1 -r
echo

if [[ $REPLY =~ ^[Yy]$ ]]; then
  git checkout src/zen/
  echo "✓ All changes in src/zen/ have been reverted."
else
  echo "Aborted."
fi
