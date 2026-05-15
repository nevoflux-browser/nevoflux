#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

cd "$REPO_ROOT"

SSH_MODE=false
FALLBACK_MODE=false
RAW_MODE=false
MACH_RUN_ARGS=()

while (($#)); do
  case "$1" in
    --ssh)
      SSH_MODE=true
      shift
      ;;
    --fallback)
      FALLBACK_MODE=true
      shift
      ;;
    --raw)
      RAW_MODE=true
      shift
      ;;
    --)
      shift
      MACH_RUN_ARGS+=("$@")
      break
      ;;
    *)
      MACH_RUN_ARGS+=("$1")
      shift
      ;;
  esac
done

if [[ "$RAW_MODE" == true && ("$SSH_MODE" == true || "$FALLBACK_MODE" == true) ]]; then
  echo "Error: --raw cannot be combined with --ssh or --fallback." >&2
  echo "--raw is now the default launch behavior; --ssh and --fallback force runtime overrides." >&2
  exit 1
fi

require_command() {
  local cmd="$1"
  local hint="$2"

  if ! command -v "$cmd" >/dev/null 2>&1; then
    echo "Missing required command: $cmd" >&2
    echo "Install hint: $hint" >&2
    exit 1
  fi
}

check_dependencies() {
  require_command python3 "Install Python 3 and rerun the NevoFlux bootstrap if needed."
  require_command npm "Install Node.js/npm."
  require_command node "Install Node.js."
  require_command cargo "Install Rust via rustup."
  require_command rustc "Install Rust via rustup."
  require_command trunk "Install Trunk with: cargo install trunk"
  require_command zip "Install zip."
  require_command unzip "Install unzip."

  if [[ ! -x engine/mach ]]; then
    echo "Missing engine/mach. Run the repository setup/bootstrap first." >&2
    exit 1
  fi

  if [[ ! -f native/nevoflux-agent/Cargo.toml ]]; then
    echo "Missing native/nevoflux-agent/Cargo.toml. The native agent is expected in the monorepo." >&2
    exit 1
  fi
}

find_obj_dir() {
  find engine -maxdepth 1 -type d -name 'obj-*' | sort | head -n 1
}

find_dist_dir() {
  local obj_dir="$1"
  local app_bundle

  if [[ -d "$obj_dir/dist/bin" ]]; then
    echo "$obj_dir/dist/bin"
    return 0
  fi

  app_bundle="$(find "$obj_dir/dist" -maxdepth 1 -type d -name '*.app' 2>/dev/null | head -n 1)"
  if [[ -n "$app_bundle" ]]; then
    echo "$app_bundle/Contents/Resources"
    return 0
  fi

  return 1
}

write_native_host_manifest() {
  local agent_path="$1"
  local host_dir="$HOME/.mozilla/native-messaging-hosts"

  mkdir -p "$host_dir"

  for host in com.nevoflux.agent com.nevoflux.agent.mcp; do
    local description="NevoFlux AI Agent"
    if [[ "$host" == "com.nevoflux.agent.mcp" ]]; then
      description="NevoFlux MCP Agent"
    fi

    printf '{"name":"%s","description":"%s","path":"%s","type":"stdio","allowed_extensions":["agent@nevoflux.com"]}\n' \
      "$host" "$description" "$agent_path" > "$host_dir/$host.json"
  done
}

configure_ssh_display() {
  echo "SSH remote display mode enabled."

  if [[ -z "${DISPLAY:-}" ]]; then
    echo "Error: DISPLAY is not set. Start SSH with X forwarding, for example: ssh -Y <host>" >&2
    exit 1
  fi

  if [[ "$DISPLAY" =~ ^localhost(:[0-9]+(\.[0-9]+)?)$ ]]; then
    DISPLAY="127.0.0.1${BASH_REMATCH[1]}"
    export DISPLAY
    echo "Normalized SSH display to: $DISPLAY"
  else
    echo "Using remote display: $DISPLAY"
  fi

  if [[ "$DISPLAY" =~ ^([^:]+):([0-9]+)(\.[0-9]+)?$ ]]; then
    local display_host="${BASH_REMATCH[1]}"
    local display_num="${BASH_REMATCH[2]}"
    local display_port=$((6000 + display_num))

    if command -v timeout >/dev/null 2>&1; then
      if ! timeout 3 bash -c ":</dev/tcp/$display_host/$display_port" 2>/dev/null; then
        echo "Error: cannot reach X11 forwarding at $display_host:$display_port for DISPLAY=$DISPLAY" >&2
        echo "Check that you connected with ssh -Y or ssh -X and that X11 forwarding is enabled on the SSH server." >&2
        exit 1
      fi
      echo "Verified X11 forwarding endpoint: $display_host:$display_port"
    else
      echo "Warning: timeout is not installed; skipping X11 forwarding port preflight." >&2
    fi
  fi
}

if [[ "$SSH_MODE" == true ]]; then
  configure_ssh_display
fi

check_dependencies

echo "Building NevoFlux browser from current source..."
(cd engine && python3 ./mach build)

OBJ_DIR="$(find_obj_dir)"
if [[ -z "$OBJ_DIR" ]]; then
  echo "No engine/obj-* build directory found after mach build." >&2
  exit 1
fi

DIST_DIR="$(find_dist_dir "$OBJ_DIR")"
APP_BIN="$DIST_DIR/nevoflux"
if [[ "$(uname -s)" == MINGW* || "$(uname -s)" == MSYS* || "$(uname -s)" == CYGWIN* ]]; then
  APP_BIN="$DIST_DIR/nevoflux.exe"
fi

if [[ ! -x "$APP_BIN" ]]; then
  echo "Current build binary is missing or not executable: $APP_BIN" >&2
  exit 1
fi

echo "Building NevoFlux native agent from monorepo source..."
cargo build --release --manifest-path native/nevoflux-agent/Cargo.toml --bin nevoflux-agent

AGENT_NAME="nevoflux-agent"
if [[ "$(uname -s)" == MINGW* || "$(uname -s)" == MSYS* || "$(uname -s)" == CYGWIN* ]]; then
  AGENT_NAME="nevoflux-agent.exe"
fi
AGENT_BUILD="native/nevoflux-agent/target/release/$AGENT_NAME"

if [[ ! -x "$AGENT_BUILD" ]]; then
  echo "Native agent build did not produce executable: $AGENT_BUILD" >&2
  exit 1
fi

echo "Building latest agent panel UI..."
(cd src/nevoflux/extensions/nevoflux-agent && env -u NO_COLOR npm run build:chat)

echo "Packaging latest agent panel extension..."
bash scripts/package-extension.sh

DIST_BUNDLE="$DIST_DIR/distribution"
mkdir -p "$DIST_BUNDLE/bin" "$DIST_BUNDLE/extensions"

if [[ -d build/AppDir/distribution ]]; then
  cp -R build/AppDir/distribution/. "$DIST_BUNDLE/"
fi

if [[ ! -f "$DIST_BUNDLE/extensions/agent@nevoflux.com.xpi" ]]; then
  echo "Packaged agent extension was not staged to $DIST_BUNDLE/extensions/agent@nevoflux.com.xpi" >&2
  exit 1
fi

install -m 0755 "$AGENT_BUILD" "$DIST_BUNDLE/bin/$AGENT_NAME"
write_native_host_manifest "$DIST_BUNDLE/bin/$AGENT_NAME"

RUN_PREFS=()

if [[ "$RAW_MODE" == true ]]; then
  echo "Raw launch mode is now the default. Continuing without launcher runtime env or graphics pref overrides."
fi

if [[ "$FALLBACK_MODE" == true || "$SSH_MODE" == true ]]; then
  if [[ "$FALLBACK_MODE" == true ]]; then
    echo "Fallback launch mode enabled. Applying launcher runtime env and pref overrides."
  fi

  export LANG="${LANG:-en_US.UTF-8}"
  export LANGUAGE="${LANGUAGE:-en_US:en}"
  export GDK_BACKEND="${GDK_BACKEND:-x11}"
  export MOZ_ENABLE_WAYLAND="${MOZ_ENABLE_WAYLAND:-0}"
  export MOZ_WEBRENDER_SOFTWARE="${MOZ_WEBRENDER_SOFTWARE:-1}"
  export MOZ_DISABLE_GFX_SANITY_TEST="${MOZ_DISABLE_GFX_SANITY_TEST:-1}"
  export LIBGL_ALWAYS_SOFTWARE="${LIBGL_ALWAYS_SOFTWARE:-1}"

  RUN_PREFS+=(
    --setpref intl.locale.requested=en-US
    --setpref intl.locale.matchOS=false
    --setpref app.update.disabledForTesting=true
    --setpref app.update.auto=false
    --setpref app.update.enabled=false
    --setpref app.update.background.enabled=false
  )
fi

if [[ "$SSH_MODE" == true ]]; then
  export MOZ_X11_EGL=0
  export MOZ_DISABLE_RDD_SANDBOX="${MOZ_DISABLE_RDD_SANDBOX:-1}"
  export MOZ_DISABLE_GPU_SANDBOX="${MOZ_DISABLE_GPU_SANDBOX:-1}"

  RUN_PREFS+=(
    --setpref layers.acceleration.disabled=true
    --setpref gfx.webrender.all=false
    --setpref gfx.webrender.software=true
    --setpref gfx.x11-egl.force-disabled=true
    --setpref media.ffmpeg.vaapi.enabled=false
    --setpref widget.dmabuf.force-enabled=false
    --setpref widget.dmabuf.force-disabled=true
  )
fi

echo "Using build: $APP_BIN"
echo "Staged extension: $DIST_BUNDLE/extensions/agent@nevoflux.com.xpi"
echo "Staged native agent: $DIST_BUNDLE/bin/$AGENT_NAME"

exec npm run start -- \
  --new-instance \
  --temp-profile \
  "${RUN_PREFS[@]}" \
  "${MACH_RUN_ARGS[@]}"
