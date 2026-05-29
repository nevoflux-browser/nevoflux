#!/bin/bash
# Dereference symlinks in the built-in nevoflux-agent addon dir of a FLAT
# (dev) build, replacing each link with a real copy of its target.
#
# Why: in a flat dev build (`mach build` / `mach build faster`), mach's install
# manifest stages chrome resources as symlinks back into the source tree. Gecko's
# `chrome://` channel follows these fine, but its `resource://builtin-addons/`
# channel — which is how a built-in (system) WebExtension's files are served to
# its `moz-extension://<uuid>/...` pages — does NOT follow NTFS symlinks on
# Windows: it reads them as empty. The result is a blank sidebar (index.html
# served as `<html><head></head><body></body></html>`, no script tag, no error).
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

ADDON_DIR="$DIST_DIR/browser/chrome/browser/builtin-addons/nevoflux-agent"
if [ ! -d "$ADDON_DIR" ]; then
  # Packaged (omni) build or addon not staged yet — nothing to deref.
  exit 0
fi

count=0
# -L so we can stat the target; -type l finds the links themselves.
while IFS= read -r link; do
  [ -n "$link" ] || continue
  target=$(readlink "$link" 2>/dev/null || true)
  if [ -n "$target" ] && [ -e "$target" ]; then
    cp --remove-destination "$target" "$link"
    count=$((count + 1))
  else
    echo "  WARN: broken symlink left as-is: $link -> $target" >&2
  fi
done < <(find "$ADDON_DIR" -type l 2>/dev/null)

if [ "$count" -gt 0 ]; then
  echo "deref-builtin-addon-symlinks: converted $count symlink(s) to real files in"
  echo "  $ADDON_DIR"
fi
