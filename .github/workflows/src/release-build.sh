#!/usr/bin/env bash

set -xe

if command -v apt-get &> /dev/null; then
  PACKAGES="python3-launchpadlib xvfb libnvidia-egl-wayland1 mesa-utils libgl1-mesa-dri"
  MISSING=""
  for pkg in $PACKAGES; do
    if ! dpkg -s "$pkg" &> /dev/null; then
      MISSING="$MISSING $pkg"
    fi
  done
  if [ -n "$MISSING" ]; then
    echo "Installing missing packages:$MISSING"
    sudo apt-get update || true
    sudo apt-get install -y $MISSING || true
  fi
fi

mkdir -p ~/.zen-keys
# safebrowsing.dat is provisioned by the workflow's "Insert API Keys" step
# (which carries that secret); do NOT rewrite it here — this script's env does
# not have $ZEN_SAFEBROWSING_API_KEY, so an unconditional echo would blank it.
#
# For the optional Mozilla / geolocation API keyfiles: write only when the key
# is actually set; otherwise REMOVE any stale (possibly empty) file. An empty
# keyfile is worse than a missing one — the mozconfig guards each option with
# `test -f`, so a 0-byte file still gets passed to --with-*-api-keyfile, which
# the Firefox build rejects ("… is empty") and aborts. A missing file is simply
# skipped, leaving that optional service unconfigured. On self-hosted runners a
# failed run skips the keys-cleanup step, so an empty mozilla.dat can linger and
# break every later run until removed here.
if [ -n "$ZEN_MOZILLA_API_KEY" ]; then
  printf '%s' "$ZEN_MOZILLA_API_KEY" > ~/.zen-keys/mozilla.dat
else
  rm -f ~/.zen-keys/mozilla.dat
fi
if [ -n "$ZEN_GOOGLE_LOCATION_SERVICE_API_KEY" ]; then
  printf '%s' "$ZEN_GOOGLE_LOCATION_SERVICE_API_KEY" > ~/.zen-keys/google_location_service.dat
else
  rm -f ~/.zen-keys/google_location_service.dat
fi

. $HOME/.cargo/env

bash ./scripts/mar_sign.sh -i

ulimit -n 4096

if command -v Xvfb &> /dev/null; then
  if ! test "$ZEN_CROSS_COMPILING"; then
    Xvfb :2 -nolisten tcp -noreset -screen 0 1024x768x24 &
    export LLVM_PROFDATA=$HOME/.mozbuild/clang/bin/llvm-profdata
    export DISPLAY=:2
  fi
  export ZEN_RELEASE=1
  npm run build
else
  echo "Xvfb could not be found, running without it"
  echo "ASSUMING YOU ARE RUNNING THIS ON MACOS"

  set -v
  export ZEN_RELEASE=1
  npm run build
fi

echo "Build complete, removing API keys"
rm -rf ~/.zen-keys
