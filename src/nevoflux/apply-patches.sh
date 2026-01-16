#!/bin/bash
# Apply nevoflux patches to the zen directory

set -e

NEVOFLUX_DIR="$(cd "$(dirname "$0")" && pwd)"
ZEN_DIR="${NEVOFLUX_DIR}/../zen"
ROOT_DIR="${NEVOFLUX_DIR}/../.."
ENGINE_DIR="${ROOT_DIR}/engine"

echo "Applying nevoflux patches to src/zen..."

# 1. Apply all .patch files
find "${NEVOFLUX_DIR}/patches" -type f -name "*.patch" 2>/dev/null | while read -r patch_file; do
  # Calculate the relative path from patches/ and map it to the zen/ directory
  relative_path="${patch_file#${NEVOFLUX_DIR}/patches/}"
  target_dir="${ZEN_DIR}/$(dirname "$relative_path")"
  
  echo "Applying: $patch_file"
  # Apply the patch in the zen directory
  (cd "${ZEN_DIR}" && git apply --ignore-whitespace "$patch_file") || {
    echo "WARN: Failed to apply $patch_file, trying with --3way"
    (cd "${ZEN_DIR}" && git apply --3way "$patch_file")
  }
done

# 2. Copy overlay files (new or overwritten files) to src/zen/
if [ -d "${NEVOFLUX_DIR}/overlays" ] && [ "$(ls -A "${NEVOFLUX_DIR}/overlays" 2>/dev/null)" ]; then
  echo "Copying overlay files to src/zen/..."
  cp -r "${NEVOFLUX_DIR}/overlays/"* "${ZEN_DIR}/"
fi

# 3. Copy root-overlays files (e.g., surfer.json, policies.json) to project root
if [ -d "${NEVOFLUX_DIR}/root-overlays" ]; then
  echo "Copying root overlay files to project root..."
  cp -r "${NEVOFLUX_DIR}/root-overlays/"* "${ROOT_DIR}/"
fi

# 4. Copy engine-overlays to engine/ directory
if [ -d "${NEVOFLUX_DIR}/engine-overlays" ]; then
  echo "Copying engine-overlays to engine/..."
  cp -r "${NEVOFLUX_DIR}/engine-overlays/"* "${ENGINE_DIR}/" 2>/dev/null || true
fi

# 5. Package nevoflux-agent extension as XPI
if [ -f "${ROOT_DIR}/scripts/package-extension.sh" ]; then
  echo "Packaging nevoflux-agent extension..."
  bash "${ROOT_DIR}/scripts/package-extension.sh"
fi

echo "All nevoflux patches applied successfully."