#!/usr/bin/env bash
# Semantic test for configs/windows/mozconfig.
# Sources the mozconfig in a sandbox that captures ac_add_options/mk_add_options
# calls and asserts the expected option set for each env combination.
set -eu

MOZCONFIG="$(dirname "$0")/../../configs/windows/mozconfig"
PASS=0
FAIL=0

run_case() {
  local name="$1"
  local expected="$2"
  shift 2

  # Reset env (use `unset` rather than =empty so `test "$VAR"` is reliably false)
  unset ZEN_CROSS_COMPILING SURFER_COMPAT ZEN_GA_GENERATE_PROFILE ZEN_GA_DISABLE_PGO \
        NEVOFLUX_PGO_PROFILE_DIR WINSYSROOT WINE MOZ_LTO MOZ_STUB_INSTALLER \
        MOZ_PKG_FORMAT CROSS_BUILD WIN32_REDIST_DIR

  # Apply this case's env vars (passed as KEY=VAL after expected)
  for kv in "$@"; do
    export "$kv"
  done

  # Sandbox: capture ac_add_options / mk_add_options calls to a tmpfile
  local tmp
  tmp=$(mktemp)
  (
    # The real mozconfig is sourced by mach without `set -u`, so unset env vars
    # are expected to evaluate to empty in `test "$VAR"` checks. Disable -u here.
    set +u
    ac_add_options() { printf 'ac %s\n' "$*" >> "$tmp"; }
    mk_add_options() { printf 'mk %s\n' "$*" >> "$tmp"; }
    # shellcheck disable=SC1090
    . "$MOZCONFIG"
  )

  local actual
  actual=$(sort "$tmp")
  rm -f "$tmp"

  local expected_sorted
  expected_sorted=$(printf '%s\n' "$expected" | sort)

  if [ "$actual" = "$expected_sorted" ]; then
    echo "PASS: $name"
    PASS=$((PASS+1))
  else
    echo "FAIL: $name"
    echo "  expected:"
    printf '    %s\n' "$expected_sorted"
    echo "  actual:"
    printf '    %s\n' "$actual"
    FAIL=$((FAIL+1))
  fi
}

# --- GCB x64 Stage 1 (instrument) ---
run_case "gcb-x64-stage1" \
"ac --without-ccache
ac --disable-maintenance-service
ac --disable-bits-download
ac --target=x86_64-pc-windows-msvc
ac --enable-eme=widevine,wmfcdm
mk export MOZ_AUTOMATION_PACKAGE_GENERATED_SOURCES=0
ac --enable-profile-generate=cross" \
  ZEN_CROSS_COMPILING=1 SURFER_COMPAT=x86_64 ZEN_GA_GENERATE_PROFILE=1

# --- GCB x64 Stage 3 (profile-use): no profdata file present ---
run_case "gcb-x64-stage3-no-profdata" \
"ac --without-ccache
ac --disable-maintenance-service
ac --disable-bits-download
ac --target=x86_64-pc-windows-msvc
ac --enable-eme=widevine,wmfcdm" \
  ZEN_CROSS_COMPILING=1 SURFER_COMPAT=x86_64

# --- GCB x64 Stage 3 with profdata present (NEVOFLUX_PGO_PROFILE_DIR set) ---
PGO_FIXTURE=$(mktemp -d)
touch "$PGO_FIXTURE/merged.profdata" "$PGO_FIXTURE/en-US.log"
run_case "gcb-x64-stage3-with-profdata" \
"ac --without-ccache
ac --disable-maintenance-service
ac --disable-bits-download
ac --target=x86_64-pc-windows-msvc
ac --enable-eme=widevine,wmfcdm
ac --enable-profile-use=cross
ac --with-pgo-profile-path=$PGO_FIXTURE/merged.profdata
ac --with-pgo-jarlog=$PGO_FIXTURE/en-US.log" \
  ZEN_CROSS_COMPILING=1 SURFER_COMPAT=x86_64 NEVOFLUX_PGO_PROFILE_DIR="$PGO_FIXTURE"
rm -rf "$PGO_FIXTURE"

# --- GCB x64 DISABLE_PGO ---
run_case "gcb-x64-disable-pgo" \
"ac --without-ccache
ac --disable-maintenance-service
ac --disable-bits-download
ac --target=x86_64-pc-windows-msvc
ac --enable-eme=widevine,wmfcdm" \
  ZEN_CROSS_COMPILING=1 SURFER_COMPAT=x86_64 ZEN_GA_DISABLE_PGO=1

# --- GCB aarch64 (no PGO ever) ---
run_case "gcb-aarch64" \
"ac --without-ccache
ac --disable-maintenance-service
ac --disable-bits-download
ac --target=aarch64-pc-windows-msvc
ac --enable-eme=widevine
ac --enable-lto=cross,thin" \
  ZEN_CROSS_COMPILING=1 SURFER_COMPAT=aarch64

# --- Native (no ZEN_CROSS_COMPILING) x64 Stage 1 ---
# NOTE: --without-ccache MUST NOT appear (native can use sccache)
run_case "native-x64-stage1" \
"ac --disable-maintenance-service
ac --disable-bits-download
ac --target=x86_64-pc-windows-msvc
ac --enable-eme=widevine,wmfcdm
mk export MOZ_AUTOMATION_PACKAGE_GENERATED_SOURCES=0
ac --enable-profile-generate=cross" \
  SURFER_COMPAT=x86_64 ZEN_GA_GENERATE_PROFILE=1

# --- Native x64 Stage 3 with profdata at NEVOFLUX_PGO_PROFILE_DIR ---
PGO_FIXTURE=$(mktemp -d)
touch "$PGO_FIXTURE/merged.profdata" "$PGO_FIXTURE/en-US.log"
run_case "native-x64-stage3" \
"ac --disable-maintenance-service
ac --disable-bits-download
ac --target=x86_64-pc-windows-msvc
ac --enable-eme=widevine,wmfcdm
ac --enable-profile-use=cross
ac --with-pgo-profile-path=$PGO_FIXTURE/merged.profdata
ac --with-pgo-jarlog=$PGO_FIXTURE/en-US.log" \
  SURFER_COMPAT=x86_64 NEVOFLUX_PGO_PROFILE_DIR="$PGO_FIXTURE"
rm -rf "$PGO_FIXTURE"

# --- Native aarch64 (no PGO) ---
run_case "native-aarch64" \
"ac --disable-maintenance-service
ac --disable-bits-download
ac --target=aarch64-pc-windows-msvc
ac --enable-eme=widevine
ac --enable-lto=cross,thin" \
  SURFER_COMPAT=aarch64 ZEN_GA_DISABLE_PGO=1

echo
echo "Passed: $PASS"
echo "Failed: $FAIL"
[ "$FAIL" -eq 0 ]
