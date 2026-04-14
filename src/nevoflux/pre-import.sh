#!/bin/bash
# Pre-import setup for idempotent surfer imports.
#
# surfer import doesn't reset engine/ before applying patches. On re-import,
# previously applied patches cause failures. This script:
# 1. Skips Zen patches that conflict with NevoFlux engine-overlays
# 2. Reverses already-applied patches so surfer can re-apply them cleanly
# 3. Ensures branding directories exist for surfer's branding-patch.js

set -e

ENGINE_DIR="$(cd "$(dirname "$0")/../.." && pwd)/engine"
ROOT_DIR="$(cd "$(dirname "$0")/../.." && pwd)"
NEVOFLUX_DIR="$(cd "$(dirname "$0")" && pwd)"

# Cross-platform sed -i
sedi() {
  if sed --version > /dev/null 2>&1; then
    sed -i "$@"
  else
    sed -i '' "$@"
  fi
}

# ── 1. Restore any patches skipped by a previous (possibly failed) import ────
for skip_file in $(find "${ROOT_DIR}/src" -name "*.nevoflux-skip" 2>/dev/null); do
  original="${skip_file%.nevoflux-skip}"
  mv "$skip_file" "$original"
done

# ── 2. Skip Zen git patches whose target files are fully replaced by overlays ─
# Auto-detects conflicts by mapping engine-overlay paths to Zen patch names.
if [ -d "${NEVOFLUX_DIR}/engine-overlays" ]; then
  (cd "${NEVOFLUX_DIR}/engine-overlays" && find . -type f) | while read -r rel_file; do
    rel_file="${rel_file#./}"
    dir_part="$(dirname "$rel_file")"
    base_part="$(basename "$rel_file")"
    # Surfer replaces ALL dots with dashes: "firefox.browser.xml" → "firefox-browser-xml"
    patch_base="$(echo "$base_part" | tr '.' '-')"
    patch_path="${ROOT_DIR}/src/${dir_part}/${patch_base}.patch"
    if [ -f "$patch_path" ]; then
      echo "Skipping conflicting Zen patch (handled by engine-overlay): src/${dir_part}/${patch_base}.patch"
      mv "$patch_path" "$patch_path.nevoflux-skip"
    fi
  done
fi

# ── 3. Reverse NevoFlux-specific modifications from apply-patches.sh ──────────
# apply-patches.sh modifies some engine files with sed (e.g., step 14 changes
# vendor/profile names). These changes must be reversed before surfer can
# re-apply the Zen git patches that originally introduced those lines.

# Step 14: toolkit/moz.configure — vendor and profile names
MOZ_CONFIGURE="${ENGINE_DIR}/toolkit/moz.configure"
if [ -f "${MOZ_CONFIGURE}" ]; then
  if grep -q 'default="NevoFlux Team"' "${MOZ_CONFIGURE}"; then
    echo "Reversing NevoFlux vendor name in toolkit/moz.configure..."
    sedi 's/default="NevoFlux Team"/default="Zen Team"/' "${MOZ_CONFIGURE}"
  fi
  if grep -q 'default="nevoflux"' "${MOZ_CONFIGURE}"; then
    echo "Reversing NevoFlux profile name in toolkit/moz.configure..."
    sedi 's/default="nevoflux"/default="zen"/' "${MOZ_CONFIGURE}"
  fi
fi

# Step 4.1: branding.nsi — NevoFlux URLs back to Zen URLs
for BRANDING_NSI in "${ENGINE_DIR}"/browser/branding/*/branding.nsi; do
  if [ -f "${BRANDING_NSI}" ] && grep -q 'nevoflux.app' "${BRANDING_NSI}"; then
    echo "Reversing NevoFlux URLs in ${BRANDING_NSI#${ENGINE_DIR}/}..."
    sedi 's|https://nevoflux.app|https://zen-browser.app|g' "${BRANDING_NSI}"
    sedi 's|https://github.com/dorisgyl/nevoflux/issues|https://github.com/zen-browser/desktop/issues|g' "${BRANDING_NSI}"
  fi
done

# ── 4. Reverse already-applied git patches ────────────────────────────────────
# For patches not handled by engine-overlays or sed reversals above, detect if
# they've been applied and reverse them so surfer can re-apply cleanly.
echo "Checking for already-applied git patches to reverse..."
for patch_file in $(find "${ROOT_DIR}/src" -name "*.patch" -not -name "*.nevoflux-skip" 2>/dev/null); do
  if (cd "${ENGINE_DIR}" && patch -p1 --dry-run --reverse --force < "$patch_file") > /dev/null 2>&1; then
    echo "  Reversing already-applied patch: ${patch_file#${ROOT_DIR}/}"
    (cd "${ENGINE_DIR}" && patch -p1 --reverse --force < "$patch_file") > /dev/null 2>&1 || true
  fi
done

# ── 5. Ensure branding directories exist (surfer's branding-patch.js needs them)
mkdir -p "$ENGINE_DIR/browser/branding/release/content"
mkdir -p "$ENGINE_DIR/browser/branding/twilight/content"

# Ensure unofficial/branding.nsi exists (surfer uses it as template in copyMozFiles)
UNOFFICIAL_NSI="$ENGINE_DIR/browser/branding/unofficial/branding.nsi"
if [ ! -f "$UNOFFICIAL_NSI" ]; then
  echo "WARNING: $UNOFFICIAL_NSI not found, creating from configs/branding/release/branding.nsi"
  mkdir -p "$(dirname "$UNOFFICIAL_NSI")"
  CONFIGS_DIR="$(cd "$(dirname "$0")/../.." && pwd)/configs"
  cp "$CONFIGS_DIR/branding/release/branding.nsi" "$UNOFFICIAL_NSI"
fi
