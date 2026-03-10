#!/bin/bash
# Skill Packager - Creates a distributable .skill file (zip format).
# No Python required. Uses standard Unix 'zip' command.
#
# Usage:
#   bash package.sh <path-to-skill-directory> [output-directory]
#
# Example:
#   bash package.sh ~/.config/nevoflux/skills/my-skill
#   bash package.sh ~/.config/nevoflux/skills/my-skill ./dist
#
# Exit codes:
#   0 - Success
#   1 - Error

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

if [ $# -lt 1 ]; then
    echo "Usage: bash package.sh <skill-directory> [output-directory]" >&2
    echo "" >&2
    echo "Example:" >&2
    echo "  bash package.sh ~/.config/nevoflux/skills/my-skill" >&2
    echo "  bash package.sh ~/.config/nevoflux/skills/my-skill ./dist" >&2
    exit 1
fi

SKILL_DIR="$(cd "$1" && pwd)"
SKILL_NAME="$(basename "$SKILL_DIR")"
OUTPUT_DIR="${2:-.}"

# Check skill directory exists
if [ ! -d "$SKILL_DIR" ]; then
    echo "Error: Skill directory not found: $SKILL_DIR" >&2
    exit 1
fi

# Check SKILL.md exists
if [ ! -f "$SKILL_DIR/SKILL.md" ]; then
    echo "Error: SKILL.md not found in $SKILL_DIR" >&2
    exit 1
fi

# Run validation first
echo "Validating skill..."
if ! bash "$SCRIPT_DIR/validate.sh" "$SKILL_DIR"; then
    echo "Validation failed. Please fix errors before packaging." >&2
    exit 1
fi
echo ""

# Check zip is available
if ! command -v zip &> /dev/null; then
    echo "Error: 'zip' command not found. Please install it:" >&2
    echo "  Ubuntu/Debian: sudo apt install zip" >&2
    echo "  macOS: brew install zip (or use built-in)" >&2
    echo "  Fedora: sudo dnf install zip" >&2
    exit 1
fi

# Create output directory if needed
mkdir -p "$OUTPUT_DIR"
OUTPUT_FILE="$(cd "$OUTPUT_DIR" && pwd)/$SKILL_NAME.skill"

# Remove existing .skill file if present
if [ -f "$OUTPUT_FILE" ]; then
    rm "$OUTPUT_FILE"
fi

echo "Packaging skill: $SKILL_NAME"

# Create zip from parent directory so the skill folder name is the root entry
PARENT_DIR="$(dirname "$SKILL_DIR")"

cd "$PARENT_DIR"

zip -r "$OUTPUT_FILE" "$SKILL_NAME" \
    -x "$SKILL_NAME/__pycache__/*" \
    -x "$SKILL_NAME/*/__pycache__/*" \
    -x "$SKILL_NAME/node_modules/*" \
    -x "$SKILL_NAME/*/node_modules/*" \
    -x "$SKILL_NAME/*.pyc" \
    -x "$SKILL_NAME/*/*.pyc" \
    -x "$SKILL_NAME/.DS_Store" \
    -x "$SKILL_NAME/*/.DS_Store" \
    -x "$SKILL_NAME/evals/*" \
    -x "$SKILL_NAME/target/*" \
    -x "$SKILL_NAME/.git/*" \
    | while IFS= read -r line; do
        echo "  $line"
    done

echo ""
echo "Successfully packaged: $OUTPUT_FILE"
echo "Size: $(du -h "$OUTPUT_FILE" | cut -f1)"
