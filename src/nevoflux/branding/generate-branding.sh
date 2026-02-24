#!/usr/bin/env bash
# generate-branding.sh — Generate all NevoFlux branding assets from source SVG
#
# Usage:
#   bash src/nevoflux/branding/generate-branding.sh
#
# Sources:
#   src/nevoflux/branding/nevoflux-logo.svg              (required)
#   src/nevoflux/branding/nevoflux-logo-private.svg       (optional, falls back to main)
#   src/nevoflux/branding/nevoflux-wordmark.svg           (optional, replaces firefox-wordmark.svg)
#   src/nevoflux/branding/nevoflux-about-wordmark.svg     (optional, replaces about-wordmark.svg)
#
# Output:
#   configs/branding/{release,twilight}/  (all generated assets)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../../.." && pwd)"

LOGO_SVG="$SCRIPT_DIR/nevoflux-logo.svg"
LOGO_PRIVATE_SVG="$SCRIPT_DIR/nevoflux-logo-private.svg"
WORDMARK_SVG="$SCRIPT_DIR/nevoflux-wordmark.svg"
ABOUT_WORDMARK_SVG="$SCRIPT_DIR/nevoflux-about-wordmark.svg"
OUTPUT_DIR="$ROOT_DIR/configs/branding"

# ── Dependency check ──────────────────────────────────────────────────────────

check_deps() {
  local missing=()
  if ! command -v rsvg-convert &> /dev/null; then
    missing+=("rsvg-convert (install: sudo apt install librsvg2-bin)")
  fi
  if ! command -v convert &> /dev/null; then
    missing+=("convert (install: sudo apt install imagemagick)")
  fi
  if [[ ${#missing[@]} -gt 0 ]]; then
    echo "ERROR: Missing required dependencies:"
    for dep in "${missing[@]}"; do
      echo "  - $dep"
    done
    exit 1
  fi
}

# ── Source SVG check ──────────────────────────────────────────────────────────

check_sources() {
  if [[ ! -f "$LOGO_SVG" ]]; then
    echo "ERROR: Main logo SVG not found: $LOGO_SVG"
    echo "Place your logo SVG at this path and re-run."
    exit 1
  fi

  if [[ ! -f "$LOGO_PRIVATE_SVG" ]]; then
    echo "NOTE: Private logo SVG not found: $LOGO_PRIVATE_SVG"
    echo "      Using main logo for private browsing assets."
    LOGO_PRIVATE_SVG="$LOGO_SVG"
  fi
}

# ── Helper: render SVG to PNG ─────────────────────────────────────────────────

render_png() {
  local svg="$1" size="$2" output="$3"
  rsvg-convert -w "$size" -h "$size" "$svg" -o "$output"
}

# ── Helper: render PNG with canvas padding (for macOS icon) ───────────────────

render_png_padded() {
  local svg="$1" canvas="$2" output="$3"
  # Render icon at 80% of canvas, centered on transparent background
  local icon_size=$((canvas * 80 / 100))
  local tmp_icon
  tmp_icon=$(mktemp /tmp/nevoflux-icon-XXXXXX.png)
  rsvg-convert -w "$icon_size" -h "$icon_size" "$svg" -o "$tmp_icon"
  convert "$tmp_icon" -gravity center -background none \
    -extent "${canvas}x${canvas}" "$output"
  rm -f "$tmp_icon"
}

# ── Helper: render PNG at non-square size (logo centered) ─────────────────────

render_png_rect() {
  local svg="$1" width="$2" height="$3" output="$4"
  # Render logo fitting within the dimensions, centered
  local icon_size=$width
  if [[ $height -lt $width ]]; then
    icon_size=$height
  fi
  # Use 80% of the smaller dimension
  icon_size=$((icon_size * 80 / 100))
  local tmp_icon
  tmp_icon=$(mktemp /tmp/nevoflux-icon-XXXXXX.png)
  rsvg-convert -w "$icon_size" -h "$icon_size" "$svg" -o "$tmp_icon"
  convert "$tmp_icon" -gravity center -background none \
    -extent "${width}x${height}" "$output"
  rm -f "$tmp_icon"
}

# ── Generate assets for one variant ──────────────────────────────────────────

generate_variant() {
  local variant="$1" # release or twilight
  local dir="$OUTPUT_DIR/$variant"
  local content_dir="$dir/content"

  mkdir -p "$dir" "$content_dir"

  echo "  Generating PNG icons..."
  local png_sizes=(16 22 24 32 48 64 128 256 512 1024)
  for size in "${png_sizes[@]}"; do
    render_png "$LOGO_SVG" "$size" "$dir/logo${size}.png"
  done

  # logo.png = 1024x1024 (same as logo1024)
  cp "$dir/logo1024.png" "$dir/logo.png"

  # logo-mac.png = 1024x1024 with padding
  echo "  Generating macOS icon..."
  render_png_padded "$LOGO_SVG" 1024 "$dir/logo-mac.png"

  # ── About page assets ─────────────────────────────────────────────────

  echo "  Generating about page assets..."
  render_png "$LOGO_SVG" 512 "$content_dir/about-logo.png"
  render_png "$LOGO_SVG" 1024 "$content_dir/about-logo@2x.png"
  render_png "$LOGO_PRIVATE_SVG" 192 "$content_dir/about-logo-private.png"
  render_png "$LOGO_PRIVATE_SVG" 384 "$content_dir/about-logo-private@2x.png"

  # Copy SVGs for about page
  cp "$LOGO_SVG" "$content_dir/about-logo.svg"
  cp "$LOGO_PRIVATE_SVG" "$content_dir/about-logo-private.svg"

  # ── Wordmark SVGs ─────────────────────────────────────────────────────

  if [[ -f "$WORDMARK_SVG" ]]; then
    echo "  Copying wordmark SVG..."
    cp "$WORDMARK_SVG" "$content_dir/firefox-wordmark.svg"
  else
    echo "  Skipping wordmark (not provided: nevoflux-wordmark.svg)"
  fi

  if [[ -f "$ABOUT_WORDMARK_SVG" ]]; then
    echo "  Copying about wordmark SVG..."
    cp "$ABOUT_WORDMARK_SVG" "$content_dir/about-wordmark.svg"
  else
    echo "  Skipping about wordmark (not provided: nevoflux-about-wordmark.svg)"
  fi

  # ── ICO files ─────────────────────────────────────────────────────────

  echo "  Generating ICO files..."
  local tmp_dir
  tmp_dir=$(mktemp -d /tmp/nevoflux-ico-XXXXXX)

  # Generate temp PNGs for main logo ICO sizes
  for size in 16 32 48 64 128 256; do
    render_png "$LOGO_SVG" "$size" "$tmp_dir/main-${size}.png"
  done

  # Generate temp PNGs for private logo ICO sizes
  for size in 16 32 48 64 128 256; do
    render_png "$LOGO_PRIVATE_SVG" "$size" "$tmp_dir/priv-${size}.png"
  done

  # firefox.ico — 256, 64, 48, 32, 16
  convert "$tmp_dir/main-256.png" "$tmp_dir/main-64.png" \
    "$tmp_dir/main-48.png" "$tmp_dir/main-32.png" \
    "$tmp_dir/main-16.png" "$dir/firefox.ico"

  # firefox64.ico — same layers as firefox.ico
  cp "$dir/firefox.ico" "$dir/firefox64.ico"

  # pbmode.ico — 256, 128, 64, 48, 32, 16 (private browsing)
  convert "$tmp_dir/priv-256.png" "$tmp_dir/priv-128.png" \
    "$tmp_dir/priv-64.png" "$tmp_dir/priv-48.png" \
    "$tmp_dir/priv-32.png" "$tmp_dir/priv-16.png" "$dir/pbmode.ico"

  # document.ico — 256, 128, 64, 48, 32, 16
  convert "$tmp_dir/main-256.png" "$tmp_dir/main-128.png" \
    "$tmp_dir/main-64.png" "$tmp_dir/main-48.png" \
    "$tmp_dir/main-32.png" "$tmp_dir/main-16.png" "$dir/document.ico"

  # document_pdf.ico — same as document.ico
  cp "$dir/document.ico" "$dir/document_pdf.ico"

  rm -rf "$tmp_dir"

  # ── Windows assets ────────────────────────────────────────────────────

  echo "  Generating Windows assets..."
  # PrivateBrowsing tiles
  render_png "$LOGO_PRIVATE_SVG" 126 "$dir/PrivateBrowsing_70.png"
  render_png "$LOGO_PRIVATE_SVG" 270 "$dir/PrivateBrowsing_150.png"

  # VisualElements tiles (1042x1046, logo centered)
  render_png_rect "$LOGO_SVG" 1042 1046 "$dir/VisualElements_70.png"
  render_png_rect "$LOGO_SVG" 1042 1046 "$dir/VisualElements_150.png"

  # wizWatermark.bmp (164x314)
  echo "  Generating Windows installer watermark..."
  render_png_rect "$LOGO_SVG" 164 314 "$dir/wizWatermark.png"
  convert "$dir/wizWatermark.png" BMP3:"$dir/wizWatermark.bmp"
  rm -f "$dir/wizWatermark.png"

  echo "  Done: $variant"
}

# ── Main ──────────────────────────────────────────────────────────────────────

main() {
  echo "╔══════════════════════════════════════════════╗"
  echo "║   NevoFlux Branding Asset Generator          ║"
  echo "╚══════════════════════════════════════════════╝"
  echo ""

  check_deps
  check_sources

  echo ""
  echo "Source:  $LOGO_SVG"
  echo "Private: $LOGO_PRIVATE_SVG"
  echo "Output:  $OUTPUT_DIR/{release,twilight}/"
  echo ""
  echo "Assets to generate per variant:"
  echo "  - 10 PNG icons (16..1024) + logo.png + logo-mac.png"
  echo "  - 4 about page PNGs + 2 about page SVGs"
  echo "  - 5 ICO files (firefox, firefox64, pbmode, document, document_pdf)"
  echo "  - 2 PrivateBrowsing tiles + 2 VisualElements tiles"
  echo "  - 1 wizWatermark.bmp"
  echo "  - Wordmark SVGs (if provided)"
  echo ""
  echo "Wordmark sources:"
  for f in "$WORDMARK_SVG" "$ABOUT_WORDMARK_SVG"; do
    local name
    name=$(basename "$f")
    if [[ -f "$f" ]]; then
      echo "  [ok] $name"
    else
      echo "  [--] $name (not found, will skip)"
    fi
  done
  echo ""

  for variant in release twilight; do
    echo "[$variant]"
    generate_variant "$variant"
    echo ""
  done

  # ── Summary ───────────────────────────────────────────────────────────

  echo "════════════════════════════════════════════════"
  echo "Generation complete. File counts:"
  echo ""
  for variant in release twilight; do
    local dir="$OUTPUT_DIR/$variant"
    local png_count ico_count bmp_count svg_count total
    png_count=$(find "$dir" -name '*.png' | wc -l)
    ico_count=$(find "$dir" -name '*.ico' | wc -l)
    bmp_count=$(find "$dir" -name '*.bmp' | wc -l)
    svg_count=$(find "$dir" -name '*.svg' | wc -l)
    total=$((png_count + ico_count + bmp_count + svg_count))
    echo "  $variant: ${png_count} PNG, ${ico_count} ICO, ${bmp_count} BMP, ${svg_count} SVG = ${total} files"
  done
  echo ""
  echo "Next steps:"
  echo "  git add configs/branding/"
  echo "  git commit -m 'feat(branding): update NevoFlux logo assets'"
}

main "$@"
