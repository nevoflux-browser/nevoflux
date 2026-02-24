#!/bin/bash
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

# NevoFlux Dioxus UI Build Script
# Builds Chat Sidebar WASM component and copies it to the extension's wasm directory

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
EXT_DIR="$(dirname "$SCRIPT_DIR")"
WASM_DIR="$EXT_DIR/wasm"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

log_info() {
  echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
  echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
  echo -e "${RED}[ERROR]${NC} $1"
}

# Check for required tools
check_prerequisites() {
  log_info "Checking prerequisites..."

  if ! command -v trunk &> /dev/null; then
    log_error "Trunk is not installed. Install with: cargo install trunk"
    exit 1
  fi

  if ! command -v wasm-bindgen &> /dev/null; then
    log_warn "wasm-bindgen CLI not found. Trunk should handle this, but manual install: cargo install wasm-bindgen-cli"
  fi

  # Check for wasm32 target
  if ! rustup target list --installed | grep -q wasm32-unknown-unknown; then
    log_info "Adding wasm32-unknown-unknown target..."
    rustup target add wasm32-unknown-unknown
  fi
}

# Build Chat Sidebar
build_chat_sidebar() {
  log_info "Building Chat Sidebar..."
  cd "$SCRIPT_DIR/chat-sidebar"
  trunk build --release

  # Copy to extension wasm directory
  local DEST="$WASM_DIR/chat-sidebar"
  mkdir -p "$DEST"
  cp -r "$SCRIPT_DIR/dist/chat-sidebar/"* "$DEST/"

  # Fix CSP by extracting inline scripts
  log_info "Fixing CSP for Chat Sidebar..."
  DIST_DIR="$DEST" "$SCRIPT_DIR/scripts/fix-csp.sh" "$DEST"

  log_info "Chat Sidebar built and copied to $DEST"
}

# Clean build artifacts
clean() {
  log_info "Cleaning build artifacts..."
  rm -rf "$SCRIPT_DIR/dist"
  rm -rf "$WASM_DIR/chat-sidebar"
  rm -rf "$SCRIPT_DIR/target"
  log_info "Clean complete"
}

# Show help
show_help() {
  echo "NevoFlux Dioxus UI Build Script"
  echo ""
  echo "Usage: $0 [command]"
  echo ""
  echo "Commands:"
  echo "  build     Build Chat Sidebar (default)"
  echo "  chat      Build Chat Sidebar"
  echo "  clean     Remove all build artifacts"
  echo "  help      Show this help message"
}

# Main
main() {
  local cmd="${1:-build}"

  case "$cmd" in
    build | chat)
      check_prerequisites
      build_chat_sidebar
      log_info "Build complete!"
      ;;
    clean)
      clean
      ;;
    help | --help | -h)
      show_help
      ;;
    *)
      log_error "Unknown command: $cmd"
      show_help
      exit 1
      ;;
  esac
}

main "$@"
