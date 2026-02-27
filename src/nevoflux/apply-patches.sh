#!/bin/bash
# Apply nevoflux patches to the zen directory

set -e

NEVOFLUX_DIR="$(cd "$(dirname "$0")" && pwd)"
ZEN_DIR="${NEVOFLUX_DIR}/../zen"
ROOT_DIR="${NEVOFLUX_DIR}/../.."
ENGINE_DIR="${ROOT_DIR}/engine"

# Cross-platform sed -i (GNU vs BSD)
sedi() {
  if sed --version > /dev/null 2>&1; then
    sed -i "$@"
  else
    sed -i '' "$@"
  fi
}

echo "Applying nevoflux patches to src/zen..."

# 1. Apply all .nfpatch files (using .nfpatch extension to avoid surfer scanning)
find "${NEVOFLUX_DIR}/patches" -type f -name "*.nfpatch" 2> /dev/null | while read -r patch_file; do
  echo "Applying: $patch_file"
  # Check if patch is already applied (idempotent)
  if (cd "${ZEN_DIR}" && git apply --check --reverse --ignore-whitespace "$patch_file") 2> /dev/null; then
    echo "  Already applied, skipping."
    continue
  fi
  # Apply the patch in the zen directory
  (cd "${ZEN_DIR}" && git apply --ignore-whitespace "$patch_file") || {
    echo "WARN: Failed to apply $patch_file, trying with --3way"
    (cd "${ZEN_DIR}" && git apply --3way "$patch_file")
  }
done

# 2. Copy overlay files (new or overwritten files) to src/zen/
#    Then create symlinks in engine/zen/ for new files added by overlays.
#    surfer import creates symlinks engine/zen/ -> src/zen/ but only for files
#    that existed at import time. Overlay files added afterwards need manual symlinks.
if [ -d "${NEVOFLUX_DIR}/overlays" ] && [ "$(ls -A "${NEVOFLUX_DIR}/overlays" 2> /dev/null)" ]; then
  echo "Copying overlay files to src/zen/..."
  cp -r "${NEVOFLUX_DIR}/overlays/"* "${ZEN_DIR}/"

  # Sync new overlay files to engine/zen/ via symlinks
  ENGINE_ZEN_DIR="${ENGINE_DIR}/zen"
  if [ -d "${ENGINE_ZEN_DIR}" ]; then
    echo "Syncing overlay symlinks to engine/zen/..."
    (cd "${NEVOFLUX_DIR}/overlays" && find . -type f) | while read -r rel_file; do
      rel_file="${rel_file#./}"
      src_file="$(cd "${ZEN_DIR}" && pwd)/${rel_file}"
      engine_file="${ENGINE_ZEN_DIR}/${rel_file}"
      if [ ! -e "${engine_file}" ]; then
        mkdir -p "$(dirname "${engine_file}")"
        ln -s "${src_file}" "${engine_file}"
        echo "  Symlinked: ${rel_file}"
      fi
    done
  fi
fi

# 3. Copy root-overlays files (e.g., surfer.json, policies.json) to project root
if [ -d "${NEVOFLUX_DIR}/root-overlays" ]; then
  echo "Copying root overlay files to project root..."
  cp -r "${NEVOFLUX_DIR}/root-overlays/"* "${ROOT_DIR}/"
fi

# 4. Copy engine-overlays to engine/ directory
if [ -d "${NEVOFLUX_DIR}/engine-overlays" ]; then
  echo "Copying engine-overlays to engine/..."
  cp -r "${NEVOFLUX_DIR}/engine-overlays/"* "${ENGINE_DIR}/" 2> /dev/null || true
fi

# 5. Sync engine-overlay locale files to locales/en-US/ for CI language pack step
#    CI runs download-language-packs.sh AFTER import, which copies locales/en-US/browser/
#    to engine/browser/locales/en-US/, overwriting our engine-overlays.
#    Auto-sync keeps both in sync so our customizations survive the language pack step.
OVERLAY_LOCALE_DIR="${NEVOFLUX_DIR}/engine-overlays/browser/locales/en-US"
if [ -d "${OVERLAY_LOCALE_DIR}" ]; then
  echo "Syncing engine-overlay locale files to locales/en-US/..."
  find "${OVERLAY_LOCALE_DIR}" -name "*.ftl" | while read -r ftl_file; do
    rel_path="${ftl_file#${OVERLAY_LOCALE_DIR}/}"
    target="${ROOT_DIR}/locales/en-US/browser/${rel_path}"
    mkdir -p "$(dirname "${target}")"
    cp "${ftl_file}" "${target}"
    echo "  Synced: ${rel_path}"
  done
fi

# 6. Append NevoFlux pref overrides to firefox.js (loaded via preprocessor #include chain)
# firefox.js → #include zen.js → #include zzz-nevoflux.js
# Our zzz-nevoflux.js overrides zen.js defaults (e.g., sidebar position)
FIREFOX_JS="${ENGINE_DIR}/browser/app/profile/firefox.js"
if [ -f "${FIREFOX_JS}" ] && [ -f "${ENGINE_DIR}/browser/app/profile/zzz-nevoflux.js" ]; then
  if ! grep -q "zzz-nevoflux.js" "${FIREFOX_JS}"; then
    echo "Appending #include zzz-nevoflux.js to firefox.js..."
    echo '#include zzz-nevoflux.js' >> "${FIREFOX_JS}"
  fi
fi

# 7. Inject nevoflux-pages into browser/components/moz.build DIRS list
#    Using sed s/// with newline instead of a\ for cross-platform compatibility
COMPONENTS_MOZBUILD="${ENGINE_DIR}/browser/components/moz.build"
if [ -f "${COMPONENTS_MOZBUILD}" ]; then
  if ! grep -q '"nevoflux-pages"' "${COMPONENTS_MOZBUILD}"; then
    echo "Adding nevoflux-pages to browser/components/moz.build DIRS..."
    sedi 's/"newtab",/"newtab",\'$'\n''    "nevoflux-pages",/' "${COMPONENTS_MOZBUILD}"
  fi
fi

# 8. Inject NevofluxBridgeRouter and NevofluxContentStore into browser/modules/moz.build EXTRA_JS_MODULES
# NOTE: Must be inserted in alphabetical order (BridgeRouter before ContentStore)
MODULES_MOZBUILD="${ENGINE_DIR}/browser/modules/moz.build"
if [ -f "${MODULES_MOZBUILD}" ]; then
  if ! grep -q '"NevofluxBridgeRouter.sys.mjs"' "${MODULES_MOZBUILD}"; then
    echo "Adding NevofluxBridgeRouter to browser/modules/moz.build..."
    sedi 's/"LinksCache.sys.mjs",/"LinksCache.sys.mjs",\'$'\n''    "NevofluxBridgeRouter.sys.mjs",/' "${MODULES_MOZBUILD}"
  fi
  if ! grep -q '"NevofluxContentStore.sys.mjs"' "${MODULES_MOZBUILD}"; then
    echo "Adding NevofluxContentStore to browser/modules/moz.build..."
    sedi 's/"NevofluxBridgeRouter.sys.mjs",/"NevofluxBridgeRouter.sys.mjs",\'$'\n''    "NevofluxContentStore.sys.mjs",/' "${MODULES_MOZBUILD}"
  fi
  if ! grep -q '"NevofluxNativeHostRegistrar.sys.mjs"' "${MODULES_MOZBUILD}"; then
    echo "Adding NevofluxNativeHostRegistrar to browser/modules/moz.build..."
    sedi 's/"NevofluxContentStore.sys.mjs",/"NevofluxContentStore.sys.mjs",\'$'\n''    "NevofluxNativeHostRegistrar.sys.mjs",/' "${MODULES_MOZBUILD}"
  fi
fi

# 9. Add NevoFlux menu item to hamburger menu (app menu)
APPMENU_XHTML="${ENGINE_DIR}/browser/base/content/appmenu-viewcache.inc.xhtml"
if [ -f "${APPMENU_XHTML}" ]; then
  if ! grep -q 'appMenu-nevoflux-button' "${APPMENU_XHTML}"; then
    echo "Adding NevoFlux button to app menu..."
    sedi 's|<toolbarbutton id="appMenu-more-button2"|<toolbarbutton id="appMenu-nevoflux-button"\
                     class="subviewbutton"\
                     label="NevoFlux"\
                     />\
      <toolbarbutton id="appMenu-more-button2"|' "${APPMENU_XHTML}"
  fi
fi

# 10. Add NevoFlux command handler to panelUI.js
PANELUI_JS="${ENGINE_DIR}/browser/components/customizableui/content/panelUI.js"
if [ -f "${PANELUI_JS}" ]; then
  if ! grep -q 'appMenu-nevoflux-button' "${PANELUI_JS}"; then
    echo "Adding NevoFlux command handler to panelUI.js..."
    sedi 's|case "appMenu-more-button2":|case "appMenu-nevoflux-button":\
        switchToTabHavingURI("nevoflux://settings", true);\
        break;\
      case "appMenu-more-button2":|' "${PANELUI_JS}"
  fi
fi

# 11. Add nevoflux chrome resources to package manifest
PACKAGE_MANIFEST="${ENGINE_DIR}/browser/installer/package-manifest.in"
if [ -f "${PACKAGE_MANIFEST}" ]; then
  if ! grep -q 'nevoflux@JAREXT@' "${PACKAGE_MANIFEST}"; then
    echo "Adding nevoflux chrome to package-manifest.in..."
    sedi '/devtools-startup@JAREXT@/i\
; NevoFlux pages\
@RESPATH@/browser/chrome/nevoflux@JAREXT@\
@RESPATH@/browser/chrome/nevoflux.manifest\
' "${PACKAGE_MANIFEST}"
  fi
fi

# 12. Package nevoflux-agent extension as XPI
if [ -f "${ROOT_DIR}/scripts/package-extension.sh" ]; then
  echo "Packaging nevoflux-agent extension..."
  bash "${ROOT_DIR}/scripts/package-extension.sh"
fi

echo "All nevoflux patches applied successfully."
