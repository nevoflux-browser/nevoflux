#!/bin/bash
# Pre-import setup: ensure branding directory structure exists before surfer import.
#
# surfer's branding-patch.js expects:
# 1. engine/browser/branding/{release,twilight}/content/ to exist (for file copying)
# 2. engine/browser/branding/unofficial/branding.nsi to exist (used as template)
#
# On fresh downloads (GCB), these may not exist yet.

set -e

ENGINE_DIR="$(cd "$(dirname "$0")/../.." && pwd)/engine"

# Ensure branding content directories exist
mkdir -p "$ENGINE_DIR/browser/branding/release/content"
mkdir -p "$ENGINE_DIR/browser/branding/twilight/content"

# Ensure unofficial/branding.nsi exists (surfer uses it as template in copyMozFiles)
UNOFFICIAL_NSI="$ENGINE_DIR/browser/branding/unofficial/branding.nsi"
if [ ! -f "$UNOFFICIAL_NSI" ]; then
  echo "WARNING: $UNOFFICIAL_NSI not found, creating from configs/branding/release/branding.nsi"
  mkdir -p "$(dirname "$UNOFFICIAL_NSI")"
  CONFIGS_DIR="$(cd "$(dirname "$0")/../.." && pwd)/configs"
  cp "$CONFIGS_DIR/branding/release/branding.nsi" "$UNOFFICIAL_NSI"
fi
