#!/usr/bin/env bash
# inject-agent.sh — Download NevoFlux Agent for a target arch and stage it
# into a per-arch distribution directory.
#
# Usage:
#   inject-agent.sh <arch>
#     <arch>   x86_64 | aarch64
#
# Env:
#   AGENT_REPO            override agent repo (default: derive from $GITHUB_REPOSITORY-agent)
#   GH_TOKEN | GITHUB_TOKEN  GitHub auth for `gh release download`
#   APPDIR_ROOT           override base output dir (default: build)
#
# Output:
#   $APPDIR_ROOT/AppDir-<arch>/distribution/bin/nevoflux-agent.exe
#   $APPDIR_ROOT/AppDir-<arch>/distribution/bin/models/ (if present in zip)
#   $APPDIR_ROOT/AppDir-<arch>/distribution/bin/defaults/soul/*.md
#   $APPDIR_ROOT/AppDir-<arch>/distribution/bin/defaults/skills/
#
# Exit codes:
#   0 — success (including "no agent release available; staged soul + skills only")
#   1 — invalid args
#   2 — gh CLI missing or auth failure

set -euo pipefail

ARCH="${1:-}"
case "$ARCH" in
  x86_64|aarch64) ;;
  *)
    echo "ERROR: arch must be x86_64 or aarch64 (got: '$ARCH')" >&2
    exit 1
    ;;
esac

AGENT_REPO="${AGENT_REPO:-${GITHUB_REPOSITORY:-dorisgyl/nevoflux}-agent}"
APPDIR_ROOT="${APPDIR_ROOT:-build}"
DIST_DIR="$APPDIR_ROOT/AppDir-$ARCH/distribution"
BIN_DIR="$DIST_DIR/bin"

mkdir -p "$BIN_DIR"

# --- Detect gh CLI ---
if ! command -v gh >/dev/null 2>&1; then
  echo "ERROR: gh CLI not found in PATH" >&2
  exit 2
fi

# --- Auth (GH_TOKEN preferred; GITHUB_TOKEN fallback) ---
if [ -z "${GH_TOKEN:-}" ] && [ -n "${GITHUB_TOKEN:-}" ]; then
  export GH_TOKEN="$GITHUB_TOKEN"
fi

# --- Download agent zip (best-effort: continue if no release) ---
ARCHIVE_NAME="nevoflux-agent-windows-${ARCH}.zip"
TMP_DIR=$(mktemp -d)
trap 'rm -rf "$TMP_DIR"' EXIT

LATEST_TAG=$(gh release view --repo "$AGENT_REPO" --json tagName --jq '.tagName' 2>/dev/null || true)
if [ -z "$LATEST_TAG" ]; then
  echo "WARN: no releases found on $AGENT_REPO; skipping agent binary injection" >&2
else
  ASSET_EXISTS=$(gh release view "$LATEST_TAG" --repo "$AGENT_REPO" --json assets --jq ".assets[].name" 2>/dev/null \
                  | grep -c "^${ARCHIVE_NAME}\$" || true)
  if [ "$ASSET_EXISTS" = "0" ]; then
    echo "WARN: $AGENT_REPO@$LATEST_TAG has no asset '$ARCHIVE_NAME'; skipping agent binary" >&2
  else
    echo "Downloading $ARCHIVE_NAME from $AGENT_REPO@$LATEST_TAG ..."
    for attempt in 1 2 3; do
      if gh release download "$LATEST_TAG" \
           --repo "$AGENT_REPO" \
           --pattern "$ARCHIVE_NAME" \
           --dir "$TMP_DIR" \
           --clobber; then
        break
      fi
      if [ "$attempt" -lt 3 ]; then
        sleep $((attempt * 10))
      else
        echo "ERROR: failed to download $ARCHIVE_NAME after 3 attempts" >&2
        exit 2
      fi
    done

    # Extract and place agent .exe
    (cd "$TMP_DIR" && unzip -o "$ARCHIVE_NAME" >/dev/null)
    if [ -f "$TMP_DIR/nevoflux-agent.exe" ]; then
      cp "$TMP_DIR/nevoflux-agent.exe" "$BIN_DIR/nevoflux-agent.exe"
      echo "Placed: $BIN_DIR/nevoflux-agent.exe"
    elif [ -f "$TMP_DIR/nevoflux-agent" ]; then
      cp "$TMP_DIR/nevoflux-agent" "$BIN_DIR/nevoflux-agent.exe"
      echo "Placed (renamed from no-ext): $BIN_DIR/nevoflux-agent.exe"
    else
      echo "WARN: zip contained no nevoflux-agent binary; skipping" >&2
    fi

    # Optional models/ directory
    if [ -d "$TMP_DIR/models" ]; then
      cp -r "$TMP_DIR/models" "$BIN_DIR/models"
      echo "Placed: $BIN_DIR/models/"
    fi
  fi
fi

# --- Soul templates (always, even if agent missing) ---
mkdir -p "$BIN_DIR/defaults/soul"
if [ -d "docs/reference/templates" ]; then
  shopt -s nullglob
  templates=(docs/reference/templates/*.md)
  shopt -u nullglob
  if [ "${#templates[@]}" -gt 0 ]; then
    cp "${templates[@]}" "$BIN_DIR/defaults/soul/"
    echo "Placed: ${#templates[@]} soul templates"
  fi
fi

# --- Built-in skills (always, even if agent missing) ---
if [ -d "docs/reference/skills" ]; then
  mkdir -p "$BIN_DIR/defaults/skills"
  cp -r docs/reference/skills/. "$BIN_DIR/defaults/skills/"
  echo "Placed: docs/reference/skills/* → $BIN_DIR/defaults/skills/"
fi

echo "inject-agent.sh: arch=$ARCH OK ($DIST_DIR)"
