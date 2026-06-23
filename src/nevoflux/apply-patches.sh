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

# 3b. Sync src/zen → engine/zen as real file copies.
#     - On Linux/macOS, surfer creates engine/zen entries as symlinks → src/zen.
#       Firefox's preprocessor doesn't follow symlinks during #include expansion,
#       which causes FILE_NOT_FOUND errors during config.status generation.
#     - On Windows (MSYS2 bash), surfer creates engine/zen as REAL FILE COPIES
#       at import time (no symlinks). Our step 1b sed and step 2 overlay-cp
#       update src/zen, but the engine/zen copies stay stale unless we re-sync.
#       The previous "find -type l" loop did nothing on Windows because there
#       were no symlinks to replace — engine/zen/common/jar.inc.mn shipped
#       without the zen-sidebar-right.css entry the overlay had added.
#     Unified fix: walk every src/zen file and force-replace the engine/zen
#     counterpart with a real-file copy. Works on both symlink and non-symlink
#     platforms.
echo "Syncing src/zen → engine/zen as real file copies (overlays + sed edits)..."
ENGINE_ZEN_DIR_SYNC="${ENGINE_DIR}/zen"
if [ -d "${ZEN_DIR}" ] && [ -d "${ENGINE_ZEN_DIR_SYNC}" ]; then
  (cd "${ZEN_DIR}" && find . -type f) | while read -r rel_file; do
    rel_file="${rel_file#./}"
    src_file="$(cd "${ZEN_DIR}" && pwd)/${rel_file}"
    engine_file="${ENGINE_ZEN_DIR_SYNC}/${rel_file}"
    mkdir -p "$(dirname "${engine_file}")"
    # rm -f handles both symlinks (Linux/macOS) and stale real-file copies (Windows)
    rm -f "${engine_file}"
    cp "${src_file}" "${engine_file}"
  done
fi
# engine/browser/base/content also has symlinks on Linux/macOS pointing into
# src/browser/base/content. Windows builds use the engine-overlays mechanism
# for files outside src/zen, so this only matters when symlinks exist.
if [ -d "${ENGINE_DIR}/browser/base/content" ]; then
  find "${ENGINE_DIR}/browser/base/content" -type l 2>/dev/null | while read -r link; do
    target=$(readlink "$link")
    if [ -f "$target" ]; then
      cp "$target" "$link.tmp" && rm "$link" && mv "$link.tmp" "$link"
    fi
  done
fi
echo "  Synced engine/zen ← src/zen; replaced symlinks in engine/browser/base/content"

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
    sedi 's|https://github.com/zen-browser/desktop/issues|https://github.com/nevoflux-browser/nevoflux/issues|g' "${BRANDING_NSI}"
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

# 15. Install nevoflux-agent as a built-in (system) WebExtension.
#     Replaces the old XPI packaging in distribution/extensions/ which left
#     Firefox showing "could not be verified for use in NevoFlux" because the
#     XPI wasn't AMO-signed. Built-in extensions live under
#     browser/extensions/<name>/, get bundled into omni.ja at
#     builtin-addons/<name>/ via jar.mn, and are treated as
#     SIGNEDSTATE_SYSTEM = 3 by XPIProvider — no warning, privileged WebExt
#     APIs available, and the addon is discovered automatically by
#     gen_built_in_addons.py walking builtin-addons/*/manifest.json.
#
#     moz.build + jar.mn for the addon are placed by step 4 (engine-overlays
#     cp) from src/nevoflux/engine-overlays/browser/extensions/nevoflux-agent/.
#     Here we stage the runtime extension files alongside them, excluding the
#     dioxus-ui/ Rust source (only its compiled WASM bundle ships) and other
#     dev artifacts.
AGENT_SRC="${ROOT_DIR}/src/nevoflux/extensions/nevoflux-agent"
AGENT_DST="${ENGINE_DIR}/browser/extensions/nevoflux-agent"
if [ -d "${AGENT_SRC}" ]; then
  echo "Installing nevoflux-agent as built-in extension..."
  mkdir -p "${AGENT_DST}"
  # Copy each runtime entry individually so we don't clobber the moz.build /
  # jar.mn that step 4 already placed from engine-overlays.
  for entry in manifest.json background content icons lib scripts utils wasm; do
    if [ -e "${AGENT_SRC}/${entry}" ]; then
      rm -rf "${AGENT_DST:?}/${entry}"
      cp -r "${AGENT_SRC}/${entry}" "${AGENT_DST}/${entry}"
    fi
  done

  # CSP fix on dioxus dist output (port from old package-extension.sh)
  DIOXUS_DIST="${AGENT_SRC}/dioxus-ui/dist/chat-sidebar"
  WASM_DIR="${AGENT_DST}/wasm/chat-sidebar"
  FIX_CSP_SCRIPT="${AGENT_SRC}/dioxus-ui/scripts/fix-csp.py"
  if [ -d "${DIOXUS_DIST}" ]; then
    if [ -f "${FIX_CSP_SCRIPT}" ] && command -v python3 > /dev/null 2>&1; then
      echo "  Fixing CSP in dioxus dist..."
      python3 "${FIX_CSP_SCRIPT}" "${DIOXUS_DIST}"
    fi
    echo "  Copying WASM bundle from dioxus dist into wasm/chat-sidebar/..."
    mkdir -p "${WASM_DIR}"
    cp -r "${DIOXUS_DIST}/"* "${WASM_DIR}/"
  fi

  # Version injection — give every build a distinct manifest.json version so
  # Firefox re-extracts on upgrade. Priority: env > surfer.json displayVersion
  # > date-based fallback.
  AGENT_MANIFEST="${AGENT_DST}/manifest.json"
  AGENT_EXT_VERSION="${NEVOFLUX_EXT_VERSION:-}"
  if [ -z "${AGENT_EXT_VERSION}" ]; then
    AGENT_SURFER_JSON="${ROOT_DIR}/surfer.json"
    if [ -f "${AGENT_SURFER_JSON}" ] && command -v python3 > /dev/null 2>&1; then
      AGENT_EXT_VERSION=$(python3 - "${AGENT_SURFER_JSON}" <<'PYEOF'
import json, sys
s = json.load(open(sys.argv[1]))
for b in s.get('brands', {}).values():
    dv = b.get('release', {}).get('displayVersion', '')
    if dv and dv != '0.0.1' and dv != '0.0.1-dev':
        print(dv); break
PYEOF
)
    fi
  fi
  if [ -z "${AGENT_EXT_VERSION}" ] && [ -n "${GITHUB_SHA:-}" ]; then
    AGENT_SHA_NUM=$(printf '%d' "0x$(printf '%s' "$GITHUB_SHA" | cut -c1-7)")
    AGENT_EXT_VERSION="0.$(date -u +%Y).${AGENT_SHA_NUM}"
  fi
  if [ -z "${AGENT_EXT_VERSION}" ]; then
    AGENT_EXT_VERSION="0.$(date -u +%Y).$(date -u +%j%H%M)"
  fi
  if [ -f "${AGENT_MANIFEST}" ] && command -v python3 > /dev/null 2>&1; then
    echo "  Injecting manifest.json version: ${AGENT_EXT_VERSION}"
    python3 - "${AGENT_MANIFEST}" "${AGENT_EXT_VERSION}" <<'PYEOF'
import json, sys
p, ver = sys.argv[1], sys.argv[2]
m = json.load(open(p))
m['version'] = ver
json.dump(m, open(p, 'w'), indent=2)
open(p, 'a').write('\n')
PYEOF
  fi

  # SRI refresh in chat-sidebar/index.html so the integrity= attrs match
  # whatever bytes we just shipped under wasm/chat-sidebar/.
  SIDEBAR_INDEX="${WASM_DIR}/index.html"
  if [ -f "${SIDEBAR_INDEX}" ] && command -v python3 > /dev/null 2>&1; then
    echo "  Refreshing SRI hashes in chat-sidebar/index.html..."
    python3 - "${SIDEBAR_INDEX}" "${WASM_DIR}" <<'PYEOF'
import base64, hashlib, pathlib, re, sys
index_path = pathlib.Path(sys.argv[1])
base_dir = pathlib.Path(sys.argv[2])
html = index_path.read_text(encoding='utf-8')
def sri(p):
    return 'sha384-' + base64.b64encode(hashlib.sha384(p.read_bytes()).digest()).decode('ascii')
pattern = re.compile(
    r'(<link[^>]*?\shref=)(["\']?)(\.\/[^\s"\'>]+)\2'
    r'([^>]*?\sintegrity=)(["\']?)[^"\'>\s]+\5',
    re.DOTALL,
)
def replace(m):
    href = m.group(3).lstrip('./')
    target = base_dir / href
    if not target.is_file():
        return m.group(0)
    return (m.group(1) + m.group(2) + m.group(3) + m.group(2) +
            m.group(4) + m.group(5) + sri(target) + m.group(5))
index_path.write_text(pattern.sub(replace, html), encoding='utf-8')
PYEOF
  fi
  echo "  Done: nevoflux-agent staged at ${AGENT_DST}"
fi

# 15b. Register nevoflux-agent in engine/browser/extensions/moz.build's DIRS
#      so mach traverses our overlay and packs the addon into omni.ja.
EXT_MOZBUILD="${ENGINE_DIR}/browser/extensions/moz.build"
if [ -f "${EXT_MOZBUILD}" ] && ! grep -q '"nevoflux-agent"' "${EXT_MOZBUILD}"; then
  echo "Adding nevoflux-agent to browser/extensions/moz.build DIRS..."
  sedi 's/"newtab",/"newtab",\'$'\n''    "nevoflux-agent",/' "${EXT_MOZBUILD}"
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

# 18. Sync .surfer/patchCount with the actual .patch count.
#     surfer import records the count BEFORE step 17 restores skipped files,
#     so .surfer/patchCount is stale by the number of restored patches. surfer
#     build's patchCheck middleware would then fire hardWarning(), whose
#     interactive prompts() call exits with code 0 in CI's non-TTY environment
#     — silently skipping mach build entirely.
PATCH_COUNT_FILE="${ROOT_DIR}/.surfer/patchCount"
if [ -f "${PATCH_COUNT_FILE}" ]; then
  ACTUAL=$(find "${ROOT_DIR}/src" -type f -name "*.patch" 2>/dev/null | wc -l | tr -d ' ')
  RECORDED=$(tr -d ' \n' < "${PATCH_COUNT_FILE}")
  if [ "${ACTUAL}" != "${RECORDED}" ]; then
    echo "Syncing .surfer/patchCount: ${RECORDED} → ${ACTUAL}"
    echo "${ACTUAL}" > "${PATCH_COUNT_FILE}"
  fi
fi

echo "All nevoflux patches applied successfully."
