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
#    Patch paths are relative to project root (e.g., src/zen/common/jar.inc.mn),
#    so we apply from ROOT_DIR. In non-git environments (Cloud Build tarball uploads),
#    fall back to the patch command.
find "${NEVOFLUX_DIR}/patches" -type f -name "*.nfpatch" 2> /dev/null | while read -r patch_file; do
  echo "Applying: $patch_file"
  # Check if patch is already applied (idempotent)
  if (cd "${ROOT_DIR}" && git apply --check --reverse --ignore-whitespace "$patch_file") 2> /dev/null; then
    echo "  Already applied, skipping."
    continue
  fi
  # Apply from project root where patch paths match
  if (cd "${ROOT_DIR}" && git apply --ignore-whitespace "$patch_file") 2> /dev/null; then
    continue
  fi
  # Fall back to patch command for non-git environments (e.g., Cloud Build tarball)
  echo "  git apply failed, falling back to patch command"
  (cd "${ROOT_DIR}" && patch -p1 --forward --ignore-whitespace < "$patch_file") || {
    echo "ERROR: Failed to apply $patch_file"
    exit 1
  }
done

# 1b. Inject zen-sidebar-right.css into jar.inc.mn (idempotent, portable across GNU/BSD sed)
JAR_INC="${ZEN_DIR}/common/jar.inc.mn"
if [ -f "${JAR_INC}" ] && ! grep -q 'zen-sidebar-right.css' "${JAR_INC}"; then
  echo "Injecting zen-sidebar-right.css into jar.inc.mn..."
  sedi 's|zen-sidebar\.css.*|&\
        content/browser/zen-styles/zen-sidebar-right.css                        (../../zen/common/styles/zen-sidebar-right.css)|' "${JAR_INC}"
fi

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

# 2b. Ensure ALL src/zen/ files have corresponding engine/zen/ symlinks.
#     surfer import only creates symlinks for files it knows about at import time.
#     New directories added by the merge (e.g., spaces/, live-folders/, share/)
#     won't have symlinks. Scan src/zen/ and create any missing ones.
ENGINE_ZEN_DIR="${ENGINE_DIR}/zen"
if [ -d "${ENGINE_ZEN_DIR}" ] && [ -d "${ZEN_DIR}" ]; then
  echo "Syncing all src/zen/ files to engine/zen/..."
  (cd "${ZEN_DIR}" && find . -type f) | while read -r rel_file; do
    rel_file="${rel_file#./}"
    src_file="$(cd "${ZEN_DIR}" && pwd)/${rel_file}"
    engine_file="${ENGINE_ZEN_DIR}/${rel_file}"
    if [ ! -e "${engine_file}" ]; then
      mkdir -p "$(dirname "${engine_file}")"
      ln -s "${src_file}" "${engine_file}"
      echo "  Synced: ${rel_file}"
    fi
  done
fi

# 3. Copy root-overlays files (e.g., surfer.json, policies.json) to project root
#    Preserve displayVersion values set by `surfer ci --display-version` before
#    the overlay overwrites surfer.json with the default placeholder (e.g. 0.0.1).
if [ -d "${NEVOFLUX_DIR}/root-overlays" ]; then
  SURFER_JSON="${ROOT_DIR}/surfer.json"
  SAVED_RELEASE_DV=""
  SAVED_TWILIGHT_DV=""
  if [ -f "${SURFER_JSON}" ] && command -v node > /dev/null 2>&1; then
    SAVED_RELEASE_DV=$(node -e "try{const c=JSON.parse(require('fs').readFileSync('${SURFER_JSON}','utf8'));console.log(c.brands?.release?.release?.displayVersion||'')}catch{}")
    SAVED_TWILIGHT_DV=$(node -e "try{const c=JSON.parse(require('fs').readFileSync('${SURFER_JSON}','utf8'));console.log(c.brands?.twilight?.release?.displayVersion||'')}catch{}")
  fi

  echo "Copying root overlay files to project root..."
  cp -r "${NEVOFLUX_DIR}/root-overlays/"* "${ROOT_DIR}/"

  # Restore displayVersion if surfer ci had set it to something different from the overlay default
  if [ -n "${SAVED_RELEASE_DV}" ] && [ -f "${SURFER_JSON}" ] && command -v node > /dev/null 2>&1; then
    OVERLAY_RELEASE_DV=$(node -e "try{const c=JSON.parse(require('fs').readFileSync('${SURFER_JSON}','utf8'));console.log(c.brands?.release?.release?.displayVersion||'')}catch{}")
    if [ "${SAVED_RELEASE_DV}" != "${OVERLAY_RELEASE_DV}" ]; then
      echo "Restoring release displayVersion: ${SAVED_RELEASE_DV} (was overwritten by overlay with ${OVERLAY_RELEASE_DV})"
      node -e "
        const fs=require('fs');
        const c=JSON.parse(fs.readFileSync('${SURFER_JSON}','utf8'));
        c.brands.release.release.displayVersion='${SAVED_RELEASE_DV}';
        fs.writeFileSync('${SURFER_JSON}',JSON.stringify(c,null,2));
      "
    fi
    if [ -n "${SAVED_TWILIGHT_DV}" ]; then
      OVERLAY_TWILIGHT_DV=$(node -e "try{const c=JSON.parse(require('fs').readFileSync('${SURFER_JSON}','utf8'));console.log(c.brands?.twilight?.release?.displayVersion||'')}catch{}")
      if [ "${SAVED_TWILIGHT_DV}" != "${OVERLAY_TWILIGHT_DV}" ]; then
        echo "Restoring twilight displayVersion: ${SAVED_TWILIGHT_DV}"
        node -e "
          const fs=require('fs');
          const c=JSON.parse(fs.readFileSync('${SURFER_JSON}','utf8'));
          c.brands.twilight.release.displayVersion='${SAVED_TWILIGHT_DV}';
          fs.writeFileSync('${SURFER_JSON}',JSON.stringify(c,null,2));
        "
      fi
    fi
  fi
fi

# 3b. Replace ALL symlinks in engine/ with real file copies.
#     Firefox's preprocessor does not follow symlinks when expanding #include
#     directives. surfer import creates symlinks (engine/zen/ → src/zen/,
#     engine/browser/base/content/ → src/browser/base/content/) which cause
#     FILE_NOT_FOUND errors and deadlocks during config.status generation.
#     Replace every symlink under engine/zen/ and engine/browser/base/content/
#     with a copy of its target.
echo "Replacing symlinks with real files for preprocessor compatibility..."
SYMLINK_COUNT=0
for dir in "${ENGINE_DIR}/zen" "${ENGINE_DIR}/browser/base/content"; do
  if [ -d "$dir" ]; then
    find "$dir" -type l | while read -r link; do
      target=$(readlink "$link")
      if [ -f "$target" ]; then
        cp "$target" "$link.tmp" && rm "$link" && mv "$link.tmp" "$link"
        SYMLINK_COUNT=$((SYMLINK_COUNT + 1))
      fi
    done
  fi
done
echo "  Replaced symlinks with real files in engine/zen/ and engine/browser/base/content/"

# 4. Copy engine-overlays to engine/ directory
if [ -d "${NEVOFLUX_DIR}/engine-overlays" ]; then
  echo "Copying engine-overlays to engine/..."
  cp -r "${NEVOFLUX_DIR}/engine-overlays/"* "${ENGINE_DIR}/" 2> /dev/null || true
fi

# 4.1. Fix NSIS branding URLs generated by surfer (hardcoded to zen-browser.app)
#      surfer's branding-patch.js generates branding.nsi with Zen URLs during `surfer import`.
#      We patch them here since apply-patches.sh runs after surfer import.
for BRANDING_NSI in "${ENGINE_DIR}"/browser/branding/*/branding.nsi; do
  if [ -f "${BRANDING_NSI}" ]; then
    echo "Patching NSIS branding URLs: ${BRANDING_NSI}"
    sedi 's|https://zen-browser.app|https://nevoflux.app|g' "${BRANDING_NSI}"
    sedi 's|https://github.com/zen-browser/desktop/issues|https://github.com/dorisgyl/nevoflux/issues|g' "${BRANDING_NSI}"
  fi
done

# 4.2. Patch NSIS installer to clear NevoFlux extension cache on upgrade
#      Firefox caches the extension XPI in profiles. Without cleanup the
#      bundled extension won't update on browser upgrade. We inject NSIS
#      code into InstallEndCleanup to delete the cached XPI, unpacked dir,
#      and addonStartup.json.lz4 for every NevoFlux profile. extensions.json
#      is NOT deleted so other user-installed extensions are unaffected.
INSTALLER_NSI="${ENGINE_DIR}/browser/installer/windows/nsis/installer.nsi"
if [ -f "${INSTALLER_NSI}" ]; then
  if grep -q "NevoFlux: Clear extension cache" "${INSTALLER_NSI}" 2>/dev/null; then
    echo "NSIS extension cache cleanup already patched, skipping."
  else
    python3 - "${INSTALLER_NSI}" <<'PYEOF' || echo "WARNING: Failed to patch NSIS installer"
import sys
marker = '${InstallEndCleanupCommon}'
cleanup = '''
  ; NevoFlux: Clear extension cache in all profiles so the updated XPI is loaded
  FindFirst $0 $1 "$APPDATA\\Mozilla\\NevoFlux\\Profiles\\*"
  ${DoWhile} $1 != ""
    ${If} $1 != "."
    ${AndIf} $1 != ".."
      Delete "$APPDATA\\Mozilla\\NevoFlux\\Profiles\\$1\\extensions\\agent@nevoflux.com.xpi"
      Delete "$APPDATA\\Mozilla\\NevoFlux\\Profiles\\$1\\addonStartup.json.lz4"
      RMDir /r "$APPDATA\\Mozilla\\NevoFlux\\Profiles\\$1\\extensions\\agent@nevoflux.com"
    ${EndIf}
    FindNext $0 $1
  ${Loop}
  FindClose $0
'''
path = sys.argv[1]
text = open(path, 'r').read()
idx = text.rfind(marker)
if idx == -1:
    print('WARNING: InstallEndCleanupCommon not found in installer.nsi', file=sys.stderr)
    sys.exit(0)
text = text[:idx] + cleanup + '\n  ' + text[idx:]
open(path, 'w').write(text)
print('Patched NSIS installer with extension cache cleanup')
PYEOF
  fi
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

# 6. Append pref includes to firefox.js (loaded via preprocessor #include chain)
# firefox.js → #include zen.js → #include zzz-nevoflux.js
FIREFOX_JS="${ENGINE_DIR}/browser/app/profile/firefox.js"
if [ -f "${FIREFOX_JS}" ]; then
  # Ensure zen.js is included (provides all Zen Browser prefs)
  if [ -f "${ENGINE_DIR}/browser/app/profile/zen.js" ] && ! grep -q '#include zen.js' "${FIREFOX_JS}"; then
    echo "Appending #include zen.js to firefox.js..."
    echo '#include zen.js' >> "${FIREFOX_JS}"
  fi
  # Ensure zzz-nevoflux.js is included (overrides zen.js defaults)
  if [ -f "${ENGINE_DIR}/browser/app/profile/zzz-nevoflux.js" ] && ! grep -q '#include zzz-nevoflux.js' "${FIREFOX_JS}"; then
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

# 9a. Add NevoFlux menu item to native menu bar (Tools menu)
#     Visible on macOS native menu bar, and Linux/Windows menu bar when shown.
MENUBAR_INC="${ENGINE_DIR}/browser/base/content/browser-menubar.inc"
if [ -f "${MENUBAR_INC}" ]; then
  if ! grep -q 'menu_nevoflux' "${MENUBAR_INC}"; then
    echo "Adding NevoFlux to native menu bar (Tools menu)..."
    sedi 's|command="View:PageInfo" data-l10n-id="menu-tools-page-info"/>|command="View:PageInfo" data-l10n-id="menu-tools-page-info"/>\
              <menuseparator id="nevofluxSep"/>\
              <menuitem id="menu_nevoflux"\
                        label="NevoFlux"\
                        oncommand="var w=Services.wm.getMostRecentWindow(\&apos;navigator:browser\&apos;);if(w\&amp;\&amp;w.gBrowser){w.gBrowser.selectedTab=w.gBrowser.addTab(\&apos;nevoflux://settings\&apos;,{triggeringPrincipal:Services.scriptSecurityManager.getSystemPrincipal()});w.focus()}"/>|' "${MENUBAR_INC}"
  fi
fi

# 9b. Add NevoFlux menu item to hamburger menu (app menu)
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

# 12. Distribution files (policies.json, extensions, agent binary) are injected
#     into the final package by CI workflows AFTER mach package completes.
#     Do NOT add distribution/** to package-manifest.in — it breaks Linux PGO
#     builds where mach package runs during the instrumented build phase before
#     distribution files are available.
#     Remove the entry if a previous import added it.
if [ -f "${PACKAGE_MANIFEST}" ]; then
  if grep -q '@RESPATH@/distribution/\*\*' "${PACKAGE_MANIFEST}"; then
    echo "Removing distribution/** from package-manifest.in (handled post-package by CI)..."
    sedi '/; NevoFlux distribution files/d' "${PACKAGE_MANIFEST}"
    sedi '/@RESPATH@\/distribution\/\*\*/d' "${PACKAGE_MANIFEST}"
  fi
fi

# 13. Fix clang-cl cross-compilation: relativize source paths in C/C++ rules
#     clang-cl interprets Unix absolute paths (e.g. /workspace/...) as MSVC flags
#     (the /w prefix looks like a compiler flag). The assembly rule already uses
#     $(call relativize,$<) but C/C++ rules pass raw $< to the compiler.
#     The relativize function is a no-op when WINE is not defined.
RULES_MK="${ENGINE_DIR}/config/rules.mk"
if [ -f "${RULES_MK}" ]; then
  # Fix C/C++ compilation rules
  if ! grep -q 'COMPILE_C.*relativize' "${RULES_MK}"; then
    echo "Patching rules.mk: relativize C/C++ source paths for clang-cl cross-compile..."
    # Replace trailing $< with $(call relativize,$<) on lines containing COMPILE_C
    # (matches COMPILE_CFLAGS, COMPILE_CXXFLAGS, COMPILE_CMFLAGS, COMPILE_CMMFLAGS)
    sedi '/COMPILE_C/s/\$<$/$(call relativize,$<)/' "${RULES_MK}"
  fi
  # Fix resource compilation (.res) rule
  if ! grep -q 'create_res\.py.*relativize' "${RULES_MK}"; then
    echo "Patching rules.mk: relativize .res source path for clang-cl cross-compile..."
    sedi '/create_res\.py/s/\$<$/$(call relativize,$<)/' "${RULES_MK}"
  fi
fi

# 14. Patch toolkit/moz.configure: set NevoFlux vendor and profile names
#     Uses sed instead of git patch for cross-platform reliability (engine file
#     state differs between Linux and macOS after Zen upstream patches).
MOZ_CONFIGURE="${ENGINE_DIR}/toolkit/moz.configure"
if [ -f "${MOZ_CONFIGURE}" ]; then
  if grep -q 'default="Zen Team"' "${MOZ_CONFIGURE}"; then
    echo "Patching toolkit/moz.configure: MOZ_APP_VENDOR → NevoFlux Team..."
    sedi 's/default="Zen Team"/default="NevoFlux Team"/' "${MOZ_CONFIGURE}"
  fi
  if grep -q 'default="zen"' "${MOZ_CONFIGURE}"; then
    echo "Patching toolkit/moz.configure: MOZ_APP_PROFILE → nevoflux..."
    sedi 's/default="zen"/default="nevoflux"/' "${MOZ_CONFIGURE}"
  fi
fi

# 15. Package nevoflux-agent extension as XPI
if [ -f "${ROOT_DIR}/scripts/package-extension.sh" ]; then
  echo "Packaging nevoflux-agent extension..."
  bash "${ROOT_DIR}/scripts/package-extension.sh"
fi

# 16. Copy en-US locale files to engine (Zen FTL files for menus, settings, etc.)
#     CI runs download-language-packs.sh separately for all languages, but local dev
#     needs at least en-US to avoid blank menus. This is idempotent.
if [ -f "${ROOT_DIR}/scripts/copy_language_pack.py" ]; then
  echo "Copying en-US locale files to engine..."
  python3 "${ROOT_DIR}/scripts/copy_language_pack.py" en-US
fi

# 17b. Mirror composition-linter from extension canonical source into
#      chrome-pages overlay so chrome://nevoflux/content/vendor/composition-linter/
#      resolves from render.js and canvas.js.
LINTER_SRC="${ROOT_DIR}/src/nevoflux/extensions/nevoflux-agent/lib/composition-linter"
LINTER_DST="${ROOT_DIR}/src/nevoflux/engine-overlays/browser/components/nevoflux-pages/content/vendor/composition-linter"
if [ -d "$LINTER_SRC" ]; then
  mkdir -p "$(dirname "$LINTER_DST")"
  rm -rf "$LINTER_DST"
  cp -R "$LINTER_SRC" "$LINTER_DST"
  rm -rf "$LINTER_DST/tests"
  echo "  Mirrored composition-linter to chrome-pages vendor dir"
fi

# 17. Restore Zen git patches that were skipped by pre-import.sh
for skip_file in $(find "${ROOT_DIR}/src" -name "*.nevoflux-skip" 2>/dev/null); do
  original="${skip_file%.nevoflux-skip}"
  mv "$skip_file" "$original"
  echo "  Restored skipped patch: ${original#${ROOT_DIR}/}"
done

# 18. Sync .surfer/patchCount with the actual .patch file count.
#     surfer import wrote the count BEFORE step 17 restored skipped patches,
#     so the recorded count is stale. Surfer's patch-check middleware compares
#     this count to the live count and aborts the build with an interactive
#     "Are you sure?" prompt when they disagree (non-TTY CI silently exits 0).
PATCH_COUNT_FILE="${ROOT_DIR}/.surfer/patchCount"
if [ -d "${ROOT_DIR}/.surfer" ]; then
  ACTUAL_PATCH_COUNT=$(find "${ROOT_DIR}/src" -name "*.patch" -type f 2>/dev/null | wc -l | tr -d ' ')
  echo "${ACTUAL_PATCH_COUNT}" > "${PATCH_COUNT_FILE}"
  echo "  Synced .surfer/patchCount to ${ACTUAL_PATCH_COUNT}"
fi

echo "All nevoflux patches applied successfully."
