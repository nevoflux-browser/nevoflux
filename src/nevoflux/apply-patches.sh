#!/bin/bash
# Apply nevoflux patches to the zen directory

set -e

NEVOFLUX_DIR="$(cd "$(dirname "$0")" && pwd)"
ZEN_DIR="${NEVOFLUX_DIR}/../zen"
ROOT_DIR="${NEVOFLUX_DIR}/../.."
ENGINE_DIR="${ROOT_DIR}/engine"

echo "Applying nevoflux patches to src/zen..."

# 1. Apply all .nfpatch files (using .nfpatch extension to avoid surfer scanning)
find "${NEVOFLUX_DIR}/patches" -type f -name "*.nfpatch" 2>/dev/null | while read -r patch_file; do
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

  # Replace __EXTENSION_INSTALL_URL__ placeholder with actual path
  POLICIES_FILE="${ROOT_DIR}/build/AppDir/distribution/policies.json"
  if [ -f "${POLICIES_FILE}" ] && grep -q "__EXTENSION_INSTALL_URL__" "${POLICIES_FILE}"; then
    source "${ROOT_DIR}/scripts/lib/detect-objdir.sh"
    OBJ_DIR="$(_detect_objdir "${ROOT_DIR}")"
    EXTENSION_URL="file://${OBJ_DIR}/dist/bin/distribution/extensions/agent@nevoflux.com.xpi"
    sed -i.bak "s|__EXTENSION_INSTALL_URL__|${EXTENSION_URL}|g" "${POLICIES_FILE}"
    rm -f "${POLICIES_FILE}.bak"
    echo "Updated policies.json install_url: ${EXTENSION_URL}"
  fi
fi

# 4. Copy engine-overlays to engine/ directory
if [ -d "${NEVOFLUX_DIR}/engine-overlays" ]; then
  echo "Copying engine-overlays to engine/..."
  cp -r "${NEVOFLUX_DIR}/engine-overlays/"* "${ENGINE_DIR}/" 2>/dev/null || true
fi

# 5. Inject nevoflux-pages into browser/components/moz.build DIRS list
COMPONENTS_MOZBUILD="${ENGINE_DIR}/browser/components/moz.build"
if [ -f "${COMPONENTS_MOZBUILD}" ]; then
  if ! grep -q '"nevoflux-pages"' "${COMPONENTS_MOZBUILD}"; then
    echo "Adding nevoflux-pages to browser/components/moz.build DIRS..."
    sed -i '/"newtab",/a\    "nevoflux-pages",' "${COMPONENTS_MOZBUILD}"
  fi
fi

# 6. Inject NevofluxBridgeRouter and NevofluxContentStore into browser/modules/moz.build EXTRA_JS_MODULES
# NOTE: Must be inserted in alphabetical order (BridgeRouter before ContentStore)
MODULES_MOZBUILD="${ENGINE_DIR}/browser/modules/moz.build"
if [ -f "${MODULES_MOZBUILD}" ]; then
  if ! grep -q '"NevofluxBridgeRouter.sys.mjs"' "${MODULES_MOZBUILD}"; then
    echo "Adding NevofluxBridgeRouter to browser/modules/moz.build..."
    sed -i '/"LinksCache.sys.mjs",/a\    "NevofluxBridgeRouter.sys.mjs",' "${MODULES_MOZBUILD}"
  fi
  if ! grep -q '"NevofluxContentStore.sys.mjs"' "${MODULES_MOZBUILD}"; then
    echo "Adding NevofluxContentStore to browser/modules/moz.build..."
    sed -i '/"NevofluxBridgeRouter.sys.mjs",/a\    "NevofluxContentStore.sys.mjs",' "${MODULES_MOZBUILD}"
  fi
fi

# 7. Package nevoflux-agent extension as XPI
if [ -f "${ROOT_DIR}/scripts/package-extension.sh" ]; then
  echo "Packaging nevoflux-agent extension..."
  bash "${ROOT_DIR}/scripts/package-extension.sh"
fi

echo "All nevoflux patches applied successfully."