/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * DOM-free logic backing the On-device group on nevoflux://settings.
 *
 * Kept separate from settings.js so it can be unit-tested without a browser.
 * settings.js imports it dynamically from chrome://nevoflux/content/pages/.
 *
 * Every input here is a daemon wire shape from `crates/daemon/src/local/`:
 * `LocalState` is internally tagged on `state`, `LocalError` on `code`, and
 * `Fit` on `fit` — all snake_case. Read them, do not invent them: a view-model
 * built against an imagined payload passes its own tests and is still wrong.
 */

const KB = 1024;
const MB = 1024 * KB;
const GB = 1024 * MB;

/**
 * Human byte size: one decimal for GB, whole numbers below that.
 *
 * settings.js has a similar helper that takes megabytes; this one takes bytes,
 * because every size the daemon reports is in bytes.
 */
export function formatBytes(bytes) {
  const n = Number(bytes) || 0;
  if (n >= GB) {
    return `${(n / GB).toFixed(1)} GB`;
  }
  if (n >= MB) {
    return `${Math.round(n / MB)} MB`;
  }
  if (n >= KB) {
    return `${Math.round(n / KB)} KB`;
  }
  return `${n} B`;
}

/** Percentage done, clamped, for a (done, total) pair. */
function pct(done, total) {
  if (!total) {
    return 0;
  }
  return Math.max(0, Math.min(100, Math.round((Number(done) / Number(total)) * 100)));
}

/** "vulkan" → "Vulkan". Backends arrive kebab-case from the daemon. */
function backendLabel(backend) {
  const s = String(backend || '');
  return s ? s.charAt(0).toUpperCase() + s.slice(1) : '';
}

/**
 * A one-line description of any state the daemon can publish.
 *
 * The default arm names the unknown state rather than returning something
 * bland: a state added to the daemon and not here should be visibly missing,
 * not silently rendered as "Working…".
 */
export function stateText(state) {
  const s = state && state.state;
  switch (s) {
    case 'idle':
      return 'Not set up';
    case 'probing':
      return 'Checking this machine';
    case 'estimating_fit':
      return 'Working out what fits';
    case 'awaiting_consent':
      return 'Waiting for you to confirm the download';
    case 'downloading_engine':
      return `Downloading engine — ${pct(state.done, state.total)}%`;
    case 'installing_engine':
      return 'Installing engine';
    case 'verifying_engine':
      return 'Verifying engine';
    case 'downloading_model':
      return `Downloading model — ${pct(state.done, state.total)}%`;
    case 'engine_starting':
      return 'Starting';
    case 'ready':
      return state.degraded
        ? `Ready on ${backendLabel(state.backend)}, reduced`
        : `Ready on ${backendLabel(state.backend)}`;
    case 'stopped':
      return 'Installed, not running';
    case 'failed':
      return errorText(state.error);
    default:
      return `Unknown state: ${s}`;
  }
}

/** A sentence for each `LocalError` code. */
function errorText(error) {
  const code = error && error.code;
  switch (code) {
    case 'download_failed':
      return 'The download failed.';
    case 'checksum_mismatch':
      return "A downloaded file didn't match its checksum.";
    case 'archive_corrupt':
      return 'The downloaded archive was damaged.';
    case 'sentinel_missing':
      return 'The engine files are incomplete.';
    case 'backend_unavailable':
      return 'No usable compute backend on this machine.';
    case 'engine_crash':
      return 'The engine crashed.';
    case 'engine_insecure':
      return 'The engine failed its security self-check.';
    case 'engine_corrupt':
      return 'The engine files are damaged.';
    case 'engine_update_required':
      return 'This engine is too old to run and an update is required.';
    case 'model_too_large_for_memory':
      return 'This model is too large for this machine.';
    case 'no_space':
      return 'Not enough disk space.';
    case 'busy':
      return 'On-device inference is held by another NevoFlux window.';
    case 'offline':
      return 'No network connection.';
    case 'engine_unreadable':
      return 'The engine files could not be read.';
    default:
      return `Stopped: ${code}`;
  }
}

/**
 * The on-device card: title, one-line subtitle, badge, buttons, and any
 * progress or notes.
 *
 * `status` is a `local.status` payload. `status.activeProvider` is added by
 * settings.js — the daemon does not report which provider is active, and the
 * card needs it to decide whether to offer "Set as default".
 */
export function cardView(status) {
  const state = (status && status.state) || {};
  const engine = (status && status.engine) || null;
  const active = status && status.activeProvider;
  const card = {
    title: 'On-device inference',
    subtitle: stateText(state),
    badge: 'Off',
    actions: [],
    progress: null,
    degradedNote: null,
    updateNote: null,
  };

  switch (state.state) {
    case 'idle':
      card.badge = 'Off';
      card.actions.push({ id: 'setup', label: 'Set up', primary: true });
      break;

    case 'probing':
    case 'estimating_fit':
    case 'installing_engine':
    case 'verifying_engine':
    case 'engine_starting':
      card.badge = 'Working';
      break;

    case 'awaiting_consent':
      card.badge = 'Working';
      card.actions.push({ id: 'consent', label: 'Review download', primary: true });
      card.actions.push({ id: 'cancel', label: 'Cancel' });
      break;

    case 'downloading_engine':
    case 'downloading_model': {
      const p = pct(state.done, state.total);
      const what = state.state === 'downloading_engine' ? 'engine' : 'model';
      card.badge = 'Working';
      card.progress = {
        pct: p,
        label: `${p}% of ${formatBytes(state.total)} (${what})`,
      };
      card.actions.push({ id: 'cancel', label: 'Cancel' });
      break;
    }

    case 'ready':
      card.badge = 'Ready';
      card.actions.push({ id: 'stop', label: 'Stop' });
      if (state.degraded && state.reason) {
        card.degradedNote = `Running reduced: ${state.reason}`;
      }
      break;

    case 'stopped':
      card.badge = 'Idle';
      card.actions.push({ id: 'start', label: 'Start' });
      break;

    case 'failed': {
      const code = state.error && state.error.code;
      card.badge = 'Problem';
      // Repair re-downloads and re-verifies the engine files, which is the
      // fix for damage and for nothing else. An engine that failed its
      // security self-check is not damaged — the files are intact and wrong —
      // so offering repair there would just reinstall the same problem.
      if (code === 'engine_corrupt' || code === 'engine_unreadable' || code === 'sentinel_missing') {
        card.actions.push({ id: 'repair', label: 'Repair', primary: true });
      } else if (code === 'engine_update_required') {
        card.actions.push({ id: 'update', label: 'Update engine', primary: true });
      } else if (state.retryable) {
        card.actions.push({ id: 'retry', label: 'Try again', primary: true });
      }
      break;
    }

    default:
      break;
  }

  // Ready but some other provider is answering: the engine is warm and unused,
  // which is worth one button rather than a paragraph.
  if (state.state === 'ready' && active && active !== 'local') {
    card.subtitle = `${card.subtitle} · Currently using ${active}`;
    card.actions.push({ id: 'set-default', label: 'Set as default', primary: true });
  }

  if (engine && engine.update === 'available') {
    card.updateNote = `A newer engine build (${engine.tag}) is available.`;
    card.actions.push({ id: 'update', label: 'Update engine' });
  } else if (engine && engine.update === 'required') {
    card.updateNote = `This engine build is no longer supported; an update is required.`;
    if (!card.actions.some((a) => a.id === 'update')) {
      card.actions.push({ id: 'update', label: 'Update engine', primary: true });
    }
  }

  return card;
}

/**
 * One entry in the model picker.
 *
 * The four states are a join of two different daemon fields: the file's state
 * (`present`/`partial`/`missing`, from `model_file_state`) and whether the
 * model fits this machine at all (`fit.fit`). A model that does not fit is
 * disabled whatever its file state — downloading five gigabytes for something
 * that cannot load is the worst outcome available.
 */
export function modelOptionLabel(model, quant) {
  const fits = !model || !model.fit || model.fit.fit !== 'insufficient';
  const size = formatBytes(quant.bytes);
  const base = `${model.display_name} · ${quant.bits} · ${size}`;

  if (!fits) {
    return { text: `${base} — won't fit this machine`, disabled: true, action: null };
  }

  switch (quant.state) {
    case 'present':
      return { text: base, disabled: false, action: 'select' };
    case 'partial': {
      const p = pct(quant.have, quant.bytes);
      return { text: `${base} — ${p}% downloaded`, disabled: false, action: 'resume' };
    }
    default:
      return { text: `${base} — download`, disabled: false, action: 'download' };
  }
}

/**
 * Why a model is unavailable, and what to do instead.
 *
 * Naming a model that would actually work turns a dead end into one click. If
 * nothing in the catalog fits, the daemon's own reason is more useful than a
 * suggestion that does not exist.
 */
export function insufficientText(fit, catalog) {
  const alternative = (catalog || []).find((m) => m.fit && m.fit.fit !== 'insufficient');
  if (alternative) {
    return `This machine can't run this model — try ${alternative.display_name}`;
  }
  const reason = (fit && fit.reason) || 'not enough memory';
  return `This machine can't run this model — ${reason}`;
}

/**
 * What the consent dialog shows before anything is downloaded.
 *
 * The alternative button exists because the CUDA engine is an order of
 * magnitude larger than the Vulkan one, and someone on a metered connection
 * should be able to see that and choose — but only when it is actually
 * cheaper, so a costlier fallback is never presented as a saving.
 */
export function consentSummary(plan) {
  const lines = [
    { label: `Engine (${backendLabel(plan.backend)})`, bytes: plan.engine_bytes },
    { label: 'Model', bytes: plan.model_bytes },
  ];
  const total = plan.engine_bytes + plan.model_bytes;

  let altButton = null;
  const cheaper = (plan.alternatives || [])
    .filter((a) => a.engine_bytes < plan.engine_bytes)
    .sort((a, b) => a.engine_bytes - b.engine_bytes)[0];
  if (cheaper) {
    const saved = formatBytes(plan.engine_bytes - cheaper.engine_bytes);
    altButton = {
      label: `Use ${backendLabel(cheaper.backend)} instead, save ${saved}`,
      backend: cheaper.backend,
    };
  }

  const disk = plan.disk || { needed: 0, available: 0 };
  return {
    lines,
    total,
    altButton,
    sources: plan.sources || [],
    disk: {
      needed: disk.needed,
      available: disk.available,
      enough: disk.available >= disk.needed,
    },
  };
}

/**
 * Whether switching providers needs a confirmation.
 *
 * The LocalOnly latch is a one-way promise for the life of the process: once
 * on-device inference has run, nothing in this process may talk to a network
 * model. Switching away therefore cannot take effect until a restart, and
 * saying so beforehand beats appearing to do nothing.
 */
export function needsExitConfirm(latched, targetProviderId) {
  return Boolean(latched) && targetProviderId !== 'local';
}
