#!/bin/bash

# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.

#
# Build NevoFlux Dioxus WASM UI components
#
# Prerequisites:
#   - Rust with wasm32-unknown-unknown target: rustup target add wasm32-unknown-unknown
#   - Trunk: cargo install trunk
#   - wasm-bindgen-cli: cargo install wasm-bindgen-cli
#

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(dirname "$SCRIPT_DIR")"
DIOXUS_DIR="$ROOT_DIR/dioxus-ui"
WASM_DIR="$ROOT_DIR/wasm"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}  NevoFlux Dioxus WASM Builder${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""

# Check prerequisites
check_prereqs() {
  echo "Checking prerequisites..."

  if ! command -v rustc &> /dev/null; then
    echo -e "${RED}Error: Rust is not installed${NC}"
    echo "Install from: https://rustup.rs/"
    exit 1
  fi

  if ! rustup target list --installed | grep -q "wasm32-unknown-unknown"; then
    echo -e "${YELLOW}Adding wasm32-unknown-unknown target...${NC}"
    rustup target add wasm32-unknown-unknown
  fi

  if ! command -v trunk &> /dev/null; then
    echo -e "${YELLOW}Installing trunk...${NC}"
    cargo install trunk
  fi

  if ! command -v wasm-bindgen &> /dev/null; then
    echo -e "${YELLOW}Installing wasm-bindgen-cli...${NC}"
    cargo install wasm-bindgen-cli
  fi

  echo -e "${GREEN}✓ All prerequisites met${NC}"
  echo ""
}

# Build a specific crate
build_crate() {
  local crate_name=$1
  local crate_dir="$DIOXUS_DIR/$crate_name"

  if [ ! -d "$crate_dir" ]; then
    echo -e "${RED}Error: Crate directory not found: $crate_dir${NC}"
    return 1
  fi

  echo -e "${YELLOW}Building $crate_name...${NC}"
  cd "$crate_dir"

  # Build with Trunk
  trunk build --release

  echo -e "${GREEN}✓ $crate_name built successfully${NC}"
  cd "$ROOT_DIR"
}

# Copy built assets to wasm/ directory
copy_assets() {
  local crate_name=$1
  local src_dir="$DIOXUS_DIR/dist/$crate_name"
  local dest_dir="$WASM_DIR/$crate_name"

  if [ ! -d "$src_dir" ]; then
    echo -e "${RED}Error: Build output not found: $src_dir${NC}"
    return 1
  fi

  echo "Copying $crate_name assets..."
  rm -rf "$dest_dir"
  mkdir -p "$dest_dir"
  cp -r "$src_dir"/* "$dest_dir/"

  echo -e "${GREEN}✓ Assets copied to wasm/$crate_name/${NC}"
}

# Main build process
main() {
  check_prereqs

  # Build shared protocol first (it's a dependency)
  echo -e "${YELLOW}Building shared-protocol...${NC}"
  cd "$DIOXUS_DIR/shared-protocol"
  cargo build --release --target wasm32-unknown-unknown
  echo -e "${GREEN}✓ shared-protocol built${NC}"
  cd "$ROOT_DIR"
  echo ""

  # Build Chat Sidebar
  build_crate "chat-sidebar"
  echo ""

  # Build Content Sidebar
  build_crate "content-sidebar"
  echo ""

  # Copy assets
  echo "Copying build assets..."
  copy_assets "chat-sidebar"
  copy_assets "content-sidebar"
  echo ""

  echo -e "${GREEN}========================================${NC}"
  echo -e "${GREEN}  Build Complete!${NC}"
  echo -e "${GREEN}========================================${NC}"
  echo ""
  echo "WASM files are in: $WASM_DIR/"
  echo ""
  echo "To package the extension:"
  echo "  cd $ROOT_DIR && npm run build"
  echo ""
}

# Run main
main "$@"
