# NevoFlux Patch System

This directory contains the patching infrastructure for customizing Zen Browser into NevoFlux without directly modifying upstream files, ensuring conflict-free syncing with upstream updates.

## Directory Structure

```
src/nevoflux/
├── README.md              # This file
├── apply-patches.sh       # Main script to apply all patches
├── patches/               # Patch files for src/zen/ (mirrors zen structure)
│   └── common/
│       └── modules/
│           └── ZenStartup-mjs.patch
├── overlays/              # New or fully replaced files for src/zen/
└── root-overlays/         # Files to overlay on project root (e.g., surfer.json)
    └── surfer.json        # NevoFlux branding configuration
```

## How It Works

```
git rebase upstream/main   → No conflicts (src/zen/ unchanged)
         ↓
npm run import             → Runs: surfer import && apply-patches.sh
         ↓
apply-patches.sh           → 1. Applies patches to src/zen/
                             2. Copies overlays to src/zen/
                             3. Copies root-overlays to project root
         ↓
npm run build              → Builds with NevoFlux branding
```

## Workflow: Creating Patches

### Step 1: Make Changes

Edit files directly in `src/zen/`:

```bash
# Edit one or more files
vim src/zen/common/modules/ZenStartup.mjs
vim src/zen/workspaces/ZenWorkspaces.mjs
```

### Step 2: Test Your Changes

Build and test to ensure everything works:

```bash
npm run build:ui
npm run start

# For agent panel changes, build and launch the full browser/agent/panel stack:
npm run start:full

# Fallback mode adds conservative display/runtime overrides if the local desktop has issues.
./scripts/launch-nevoflux.sh --fallback

# Backup only for SSH X11 forwarding. This is slow for full browser UI testing.
./scripts/launch-nevoflux.sh --ssh

# Compatibility alias for the default no-overrides launch behavior.
./scripts/launch-nevoflux.sh --raw
```

### Step 3: Export Patches

Once satisfied, export all modified files as patches:

```bash
./scripts/export-nevoflux-patches.sh
```

This will:

- Scan `src/zen/` for modified files
- Generate `.patch` files in `src/nevoflux/patches/` with matching directory structure

### Step 4: Revert Changes

Revert `src/zen/` to keep it clean for upstream sync:

```bash
./scripts/revert-zen-changes.sh
```

### Step 5: Commit

```bash
git add src/nevoflux/
git commit -m "Add NevoFlux patches for feature X"
```

## Helper Scripts

| Script                               | Purpose                                                 |
| ------------------------------------ | ------------------------------------------------------- |
| `scripts/export-nevoflux-patches.sh` | Export all `src/zen/` changes as patches                |
| `scripts/revert-zen-changes.sh`      | Revert all `src/zen/` changes                           |
| `src/nevoflux/apply-patches.sh`      | Apply patches during build (called by `npm run import`) |

## Branding Configuration

The `root-overlays/surfer.json` file contains NevoFlux branding:

- `name`: "NevoFlux"
- `appId`: "nevoflux"
- `binaryName`: "nevoflux"
- `brandFullName`: "NevoFlux Browser"
- `updateHostname`: "updates.nevoflux.com"

This file overwrites the root `surfer.json` during `npm run import`, avoiding merge conflicts with upstream.

## Handling Upstream Updates

```bash
# 1. Sync with upstream
git fetch upstream
git rebase upstream/main # No conflicts in src/zen/

# 2. Re-import and apply patches
npm run import

# 3. If patches fail to apply:
#    - Check which patch failed
#    - Update the patch based on new upstream code
#    - Re-run: npm run import
```

## Continuing Development After Upstream Updates

If you already have patches and want to make further modifications after upstream updates:

```bash
# 1. Apply existing patches to src/zen/ (for development)
./scripts/apply-nevoflux-patches-dev.sh

# 2. Make additional changes on top of patched files
vim src/zen/common/modules/ZenStartup.mjs

# 3. Test browser-only changes
npm run build:ui && npm run start

# For agent panel changes, test the full browser/agent/panel stack
npm run start:full

# Fallback mode adds conservative display/runtime overrides if the local desktop has issues.
./scripts/launch-nevoflux.sh --fallback

# Backup only for SSH X11 forwarding. Prefer remote desktop/VNC for performance.
./scripts/launch-nevoflux.sh --ssh

# Compatibility alias for the default no-overrides launch behavior.
./scripts/launch-nevoflux.sh --raw

# 4. Re-export patches (overwrites old patches with combined changes)
./scripts/export-nevoflux-patches.sh

# 5. Revert src/zen/
./scripts/revert-zen-changes.sh

# 6. Commit updated patches
git add src/nevoflux/patches/
git commit -m "Update patches for upstream changes"
```

> **Note**: The exported patch will contain ALL your changes (old + new) relative to the current upstream version.

## Best Practices

1. **Keep patches small and focused** - One patch per feature/fix
2. **Mirror directory structure** - Match `patches/` layout to `src/zen/`
3. **Never commit src/zen/ changes** - Always export as patches first
4. **Test before exporting** - Ensure changes work before creating patches
