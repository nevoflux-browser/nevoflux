#!/bin/bash
# Detect the engine obj-* directory and dist directory for the current platform
# Usage: source scripts/lib/detect-objdir.sh
#   Provides: OBJ_DIR  (e.g., engine/obj-x86_64-pc-linux-gnu)
#             DIST_DIR (e.g., engine/obj-*/dist/bin on Linux,
#                             engine/obj-*/dist/<App>.app/Contents/Resources on macOS)

_detect_objdir() {
  local project_root="${1:-.}"
  local engine_dir="$project_root/engine"

  # Try to find existing obj-* directory
  local found
  found=$(find "$engine_dir" -maxdepth 1 -name 'obj-*' -type d 2>/dev/null | head -1)
  if [ -n "$found" ]; then
    echo "$found"
    return 0
  fi

  # Fallback: construct from platform detection
  local arch
  local os_triple
  arch="$(uname -m)"
  case "$(uname -s)" in
    Linux*)
      os_triple="${arch}-pc-linux-gnu"
      ;;
    Darwin*)
      [ "$arch" = "arm64" ] && arch="aarch64"
      os_triple="${arch}-apple-darwin"
      ;;
    MINGW*|MSYS*|CYGWIN*)
      os_triple="${arch}-pc-windows-msvc"
      ;;
    *)
      echo "ERROR: Unsupported platform: $(uname -s)" >&2
      return 1
      ;;
  esac

  echo "$engine_dir/obj-${os_triple}"
}

# Detect the platform-specific dist directory within obj-*
# Linux:  obj-*/dist/bin
# macOS:  obj-*/dist/<AppName>.app/Contents/Resources
_detect_distdir() {
  local obj_dir="$1"

  # Linux: dist/bin
  if [ -d "$obj_dir/dist/bin" ]; then
    echo "$obj_dir/dist/bin"
    return 0
  fi

  # macOS: dist/<AppName>.app/Contents/Resources
  local app_bundle
  app_bundle=$(find "$obj_dir/dist" -maxdepth 1 -name "*.app" -type d 2>/dev/null | head -1)
  if [ -n "$app_bundle" ]; then
    echo "$app_bundle/Contents/Resources"
    return 0
  fi

  # Fallback based on platform
  case "$(uname -s)" in
    Darwin*)
      echo "$obj_dir/dist/NevoFlux Browser.app/Contents/Resources"
      ;;
    *)
      echo "$obj_dir/dist/bin"
      ;;
  esac
}

# Auto-set OBJ_DIR and DIST_DIR if PROJECT_ROOT is available
if [ -n "$PROJECT_ROOT" ]; then
  OBJ_DIR="$(_detect_objdir "$PROJECT_ROOT")"
  DIST_DIR="$(_detect_distdir "$OBJ_DIR")"
fi
