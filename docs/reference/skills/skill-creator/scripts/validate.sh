#!/bin/bash
# Skill Validator - Validates a skill directory structure and SKILL.md frontmatter.
# No Python required. Uses standard Unix tools only.
#
# Usage:
#   bash validate.sh <path-to-skill-directory>
#
# Exit codes:
#   0 - Valid
#   1 - Invalid (error message printed to stderr)

set -euo pipefail

if [ $# -lt 1 ]; then
  echo "Usage: bash validate.sh <skill-directory>" >&2
  exit 1
fi

SKILL_DIR="$1"

# Check directory exists
if [ ! -d "$SKILL_DIR" ]; then
  echo "FAIL: Directory not found: $SKILL_DIR" >&2
  exit 1
fi

# Check SKILL.md exists
SKILL_MD="$SKILL_DIR/SKILL.md"
if [ ! -f "$SKILL_MD" ]; then
  echo "FAIL: SKILL.md not found in $SKILL_DIR" >&2
  exit 1
fi

# Read file content
CONTENT="$(cat "$SKILL_MD")"

# Check frontmatter starts with ---
FIRST_LINE="$(head -1 "$SKILL_MD")"
if [ "$FIRST_LINE" != "---" ]; then
  echo "FAIL: SKILL.md must start with --- (YAML frontmatter)" >&2
  exit 1
fi

# Extract frontmatter (between first and second ---)
FRONTMATTER="$(awk 'BEGIN{found=0} /^---$/{found++; if(found==2) exit; next} found==1{print}' "$SKILL_MD")"

if [ -z "$FRONTMATTER" ]; then
  echo "FAIL: No closing --- found for YAML frontmatter" >&2
  exit 1
fi

# Extract name field
NAME="$(echo "$FRONTMATTER" | grep -E '^name:' | head -1 | sed 's/^name:[[:space:]]*//' | sed 's/^"//' | sed 's/"$//' | sed "s/^'//" | sed "s/'$//")"

if [ -z "$NAME" ]; then
  echo "FAIL: Missing 'name' in frontmatter" >&2
  exit 1
fi

# Validate name is kebab-case
if ! echo "$NAME" | grep -qE '^[a-z0-9][a-z0-9-]*[a-z0-9]$' 2> /dev/null; then
  # Allow single-char names
  if ! echo "$NAME" | grep -qE '^[a-z0-9]$' 2> /dev/null; then
    echo "FAIL: Name '$NAME' must be kebab-case (lowercase letters, digits, hyphens; no leading/trailing hyphens)" >&2
    exit 1
  fi
fi

# Check for consecutive hyphens
if echo "$NAME" | grep -q '\-\-'; then
  echo "FAIL: Name '$NAME' cannot contain consecutive hyphens" >&2
  exit 1
fi

# Check name length
NAME_LEN="${#NAME}"
if [ "$NAME_LEN" -gt 64 ]; then
  echo "FAIL: Name is too long ($NAME_LEN chars). Maximum is 64." >&2
  exit 1
fi

# Extract description field (handle multiline YAML)
# First try single-line description
DESC="$(echo "$FRONTMATTER" | grep -E '^description:' | head -1 | sed 's/^description:[[:space:]]*//' | sed 's/^"//' | sed 's/"$//' | sed "s/^'//" | sed "s/'$//")"

# If description starts with | or >, it's multiline - extract until next key
if echo "$DESC" | grep -qE '^\|' || echo "$DESC" | grep -qE '^>'; then
  DESC="$(echo "$FRONTMATTER" | awk '/^description:/{found=1; next} found && /^[a-z]/{exit} found{gsub(/^[[:space:]]+/, ""); printf "%s ", $0}' | sed 's/[[:space:]]*$//')"
fi

if [ -z "$DESC" ]; then
  echo "FAIL: Missing 'description' in frontmatter" >&2
  exit 1
fi

# Check description length
DESC_LEN="${#DESC}"
if [ "$DESC_LEN" -gt 1024 ]; then
  echo "FAIL: Description is too long ($DESC_LEN chars). Maximum is 1024." >&2
  exit 1
fi

# Check for angle brackets in description
if echo "$DESC" | grep -q '[<>]'; then
  echo "FAIL: Description cannot contain angle brackets (< or >)" >&2
  exit 1
fi

# Check for unexpected frontmatter keys
# NevoFlux supports: name, description, version, author, tags, enabled,
# triggers, allowed_tools (underscore), allowed-tools (hyphen), dependencies,
# extra, license, metadata, compatibility
ALLOWED_KEYS="name description version author tags enabled triggers allowed_tools allowed-tools dependencies extra license metadata compatibility"
FOUND_KEYS="$(echo "$FRONTMATTER" | grep -oE '^[a-z][a-z-]*:' | sed 's/:$//' | sort -u)"

for key in $FOUND_KEYS; do
  MATCH=0
  for allowed in $ALLOWED_KEYS; do
    if [ "$key" = "$allowed" ]; then
      MATCH=1
      break
    fi
  done
  if [ "$MATCH" -eq 0 ]; then
    echo "FAIL: Unexpected key '$key' in frontmatter. Allowed: $ALLOWED_KEYS" >&2
    exit 1
  fi
done

echo "OK: Skill '$NAME' is valid."
exit 0
