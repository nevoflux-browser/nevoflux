# Update NevoFlux Agent

Scripts to update the agent binary in an existing NevoFlux browser installation, without rebuilding the browser.

NevoFlux browser already handles native messaging registration automatically via `NevofluxNativeHostRegistrar`. These scripts only replace the agent binary in `distribution/bin/`.

## Usage

### Linux/macOS

```bash
# Auto-detect agent binary and browser location
./update-agent.sh

# Download latest release from GitHub
./update-agent.sh --download

# Specify paths explicitly
./update-agent.sh --agent /path/to/nevoflux-agent --browser /path/to/nevoflux
```

### Windows (PowerShell)

```powershell
# Auto-detect
.\update-agent.ps1

# Download latest release from GitHub
.\update-agent.ps1 -Download

# Specify paths explicitly
.\update-agent.ps1 -AgentPath "C:\path\to\nevoflux-agent.exe" -BrowserDir "C:\path\to\nevoflux"
```

## Agent Binary Resolution

1. Explicit path (`--agent` / `-AgentPath`)
2. `NEVOFLUX_AGENT_BIN` environment variable
3. Local build at `../../target/release/nevoflux-agent`
4. GitHub release download (latest from `dorisgyl/nevoflux-agent`)

## Browser Directory Resolution

1. Explicit path (`--browser` / `-BrowserDir`)
2. `NEVOFLUX_BROWSER_DIR` environment variable
3. Dev build at `../nevoflux/engine/obj-*/dist/bin`
4. Common install locations (`/opt/nevoflux`, `C:\Program Files\NevoFlux`, etc.)

## What Gets Copied

- `nevoflux-agent` binary → `<browser>/distribution/bin/`
- `models/` directory → `<browser>/distribution/bin/models/` (if present)

Restart the browser after updating.
