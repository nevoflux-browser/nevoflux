#!/bin/bash
# Dereference symlinks in the built-in nevoflux-agent addon dir of a FLAT
# (dev) build, replacing each link with a real copy of its target.
# Also handles nevoflux:// page resources for the same reason.
#
# Why: in a flat dev build (`mach build` / `mach build faster`), mach's install
# manifest stages chrome resources as symlinks back into the source tree. Gecko's
# `chrome://` channel follows these fine, but its `resource://builtin-addons/`
# channel — which is how a built-in (system) WebExtension's files are served to
# its `moz-extension://<uuid>/...` pages — does NOT follow NTFS symlinks on
# Windows: it reads them as empty. The result is a blank sidebar (index.html
# served as `<html><head></head><body></body></html>`, no script tag, no error).
# Similarly, nevoflux:// pages load in a web content process that cannot follow
# symlinks, causing the canvas page to stay at about:blank.
#
# Converting the addon's files to real copies makes the resource:// reader return
# the real bytes. This only matters for flat dev builds; packaged builds bundle
# the addon inside omni.ja (real bytes in a zip, no symlinks), so the addon dir
# below simply won't exist there and this script no-ops.
#
# Safe to run unconditionally and repeatedly: no symlinks -> nothing to do.
set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
source "$SCRIPT_DIR/lib/detect-objdir.sh"

deref_dir() {
  local dir="$1"
  [ -d "$dir" ] || return 0
  local count=0 link target
  while IFS= read -r link; do
    [ -n "$link" ] || continue
    target=$(readlink "$link" 2>/dev/null || true)
    if [ -n "$target" ] && [ -e "$target" ]; then
      rm -f "$link" && cp "$target" "$link"
      count=$((count + 1))
    else
      echo "  WARN: broken symlink left as-is: $link -> $target" >&2
    fi
  done < <(find "$dir" -type l 2>/dev/null)
  if [ "$count" -gt 0 ]; then
    echo "deref-builtin-addon-symlinks: converted $count symlink(s) to real files in"
    echo "  $dir"
  fi
}

# The built-in extension (resource://builtin-addons/ cannot read NTFS symlinks).
deref_dir "$DIST_DIR/browser/chrome/browser/builtin-addons/nevoflux-agent"
# nevoflux:// pages: the canvas page loads in a web content process, which
# cannot follow a symlink out to engine/, so the tab stays at about:blank.
deref_dir "$DIST_DIR/browser/chrome/nevoflux"
