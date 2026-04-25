// Render page orchestration — page-driven pull loop.
//
// Post-actor-rework (2026-04-20) the page is the driver: it fetches the
// composition via NevofluxBridge.canvasVideo.getComposition, iterates each
// frame locally, asks the parent JSActor to drawSnapshot the composition
// iframe, and forwards PNG bytes back to the daemon in 1 MB chunks.

import {
  withPatches,
  setRenderTime,
} from 'chrome://nevoflux/content/pages/render-patches.js';

const CHUNK_SIZE = 1024 * 1024; // 1 MB PNG chunks

// window.location keeps the original nevoflux:// URL (because the protocol
// handler sets channel.originalURI); query strings on non-http schemes are
// unreliable, so we parse job_id / composition_id from the pathname exactly
// like other NevoFlux pages do. Query string is used only as a fallback when
// the page is loaded directly from its resolved chrome:// URL.
function parseRenderParams() {
  const loc = window.location;
  if (loc.search) {
    const q = new URLSearchParams(loc.search);
    const j = q.get('job_id');
    const c = q.get('composition_id');
    if (j) return { jobId: j, compositionId: c || '' };
  }
  if (loc.protocol === 'nevoflux:') {
    const segments = loc.pathname.replace(/^\/+/, '').split('/').filter(Boolean);
    return {
      jobId: segments[0] || '',
      compositionId: segments[1] || '',
    };
  }
  return { jobId: '', compositionId: '' };
}

const { jobId, compositionId } = parseRenderParams();

const statusEl = document.getElementById('status');
const iframe = document.getElementById('composition-iframe');

function setStatus(text) {
  console.log('[render.js]', text);
  if (statusEl) statusEl.textContent = text;
}

function bridge() {
  const b = window.NevofluxBridge;
  if (!b || !b.canvasVideo) {
    throw new Error('NevofluxBridge.canvasVideo unavailable');
  }
  return b.canvasVideo;
}

async function loadComposition(htmlText, widthPx, heightPx) {
  iframe.width = widthPx;
  iframe.height = heightPx;
  iframe.style.width = widthPx + 'px';
  iframe.style.height = heightPx + 'px';

  // Wait for the iframe-injected patches script to post 'iframeReady' once
  // window.__timelines is populated. Cross-origin ESM imports (gsap from
  // esm.sh) can finish AFTER iframe.load fires, so this is the only reliable
  // signal that the composition has registered its timelines and is ready
  // to be seek-driven. 6s budget covers cold CDN + slow disk.
  const ready = new Promise((resolve) => {
    let settled = false;
    const handler = (evt) => {
      if (evt.source === iframe.contentWindow && evt.data?.__nf_type === 'iframeReady') {
        if (settled) return;
        settled = true;
        window.removeEventListener('message', handler);
        console.log('[render.js] iframeReady tlCount=' + evt.data.tlCount + ' threeCount=' + evt.data.threeCount);
        resolve(evt.data);
      }
    };
    window.addEventListener('message', handler);
    setTimeout(() => {
      if (settled) return;
      settled = true;
      window.removeEventListener('message', handler);
      console.warn('[render.js] iframeReady timeout — proceeding anyway');
      resolve(null);
    }, 6000);
  });

  // Inject determinism patches as the first <script> inside the iframe's own
  // document. Reaching into iframe.contentWindow from chrome code is blocked
  // by the same-origin policy (srcdoc iframes run in a null-principal origin),
  // so we pre-process the HTML instead.
  iframe.srcdoc = withPatches(htmlText);
  await ready;

  // Two extra RAFs after ready to let GSAP apply any from-state and the
  // compositor flush before the first capture.
  await new Promise((r) => window.requestAnimationFrame(r));
  await new Promise((r) => window.requestAnimationFrame(r));
}

async function seekFrame(t) {
  // All per-frame orchestration inside the iframe (timelines, Three.js
  // renderers, <video>/<audio> seeking) is driven from inside the iframe's
  // own context — compositions hook this via the __nf_seek message below.
  // The parent chrome page can't access iframe globals directly because of
  // the null-principal cross-origin boundary.
  setRenderTime(iframe.contentWindow, t);
  iframe.contentWindow.postMessage(
    { __nf_type: 'seek', seconds: t },
    '*'
  );

  // Double RAF in the parent page is enough for the compositor to pick up
  // whatever the iframe rendered in response to the messages above.
  await new Promise((r) => window.requestAnimationFrame(r));
  await new Promise((r) => window.requestAnimationFrame(r));
}

async function forwardPng(frameIdx, pngBytes) {
  const u8 = pngBytes instanceof Uint8Array ? pngBytes : new Uint8Array(pngBytes);
  const total = Math.max(1, Math.ceil(u8.length / CHUNK_SIZE));
  for (let i = 0; i < total; i++) {
    const start = i * CHUNK_SIZE;
    const end = Math.min(u8.length, start + CHUNK_SIZE);
    const slice = u8.subarray(start, end);
    await bridge().forwardFrameChunk({
      job_id: jobId,
      frame_idx: frameIdx,
      chunk_idx: i,
      total_chunks: total,
      is_last: i === total - 1,
      bytes: Array.from(slice),
    });
  }
}

async function runRenderLoop() {
  if (!jobId) {
    setStatus('error: job_id missing from URL');
    return;
  }

  // One-shot diagnostic: log the first few seekAck messages so we know the
  // bridge fired and how many timelines were addressed.
  let _firstAckLogged = false;
  let _ackCount = 0;
  window.addEventListener('message', (evt) => {
    if (evt.data?.__nf_type !== 'seekAck') return;
    _ackCount++;
    if (!_firstAckLogged) {
      _firstAckLogged = true;
      console.log('[render.js] first seekAck t=' + evt.data.t + ' tlCount=' + evt.data.tlCount);
    }
  });

  setStatus(`fetching composition for job ${jobId}`);
  const comp = await bridge().getComposition(jobId);
  if (!comp || !comp.html) {
    throw new Error(`getComposition returned no html (got ${JSON.stringify(comp)})`);
  }

  // P3: pre-render composition lint gate. Runs the same module as the
  // agent tool + canvas editor. A broken linter module (import fail or
  // runtime throw) does NOT block the render — it's an advisory gate.
  let lintReport = null;
  try {
    const mod = await import(
      'chrome://nevoflux/content/vendor/composition-linter/index.js'
    );
    lintReport = mod.lint(comp.html, { composition_id: jobId });
  } catch (err) {
    console.warn('[render] lint gate unavailable, proceeding without it:', err);
    lintReport = null;
  }
  if (lintReport && lintReport.errors.length > 0) {
    const summary = lintReport.errors.slice(0, 3)
      .map(e => `${e.rule_id}: ${e.message}`).join('; ');
    const tail = lintReport.errors.length > 3
      ? ` (+${lintReport.errors.length - 3} more)` : '';
    const msg = `LintFailed: ${lintReport.errors.length} error(s): ${summary}${tail}`;
    setStatus(msg);
    await bridge().reportFailed(jobId, msg);
    return;
  }

  setStatus(`loading composition (${comp.width}×${comp.height}, ${comp.duration_sec}s @ ${comp.fps}fps)`);
  await loadComposition(comp.html, comp.width, comp.height);

  setStatus('registering job with parent actor');
  await bridge().registerJob(jobId);

  const totalFrames = Math.ceil(comp.duration_sec * comp.fps);
  setStatus(`rendering ${totalFrames} frames`);

  // PoC gate telemetry: track drawSnapshot ms reported by the actor so the
  // operator can read the median off this tab's console and fill the §4.6
  // table. Kept in-page because drawMs originates in the parent actor and
  // isn't currently plumbed back to the daemon.
  const drawMsSamples = [];
  const medianOf = (xs) => {
    if (!xs.length) return NaN;
    const sorted = xs.slice().sort((a, b) => a - b);
    return sorted[Math.floor(sorted.length / 2)];
  };

  for (let frameIdx = 0; frameIdx < totalFrames; frameIdx++) {
    const t = frameIdx / comp.fps;
    await seekFrame(t);

    const res = await bridge().drawFrame(jobId, comp.width, comp.height);
    if (!res || !res.ok) {
      throw new Error(
        `drawFrame failed at frame ${frameIdx}: ${res?.error || 'unknown'}`
      );
    }
    if (typeof res.drawMs === 'number') {
      drawMsSamples.push(res.drawMs);
    }
    await forwardPng(frameIdx, res.bytes);

    if (frameIdx % 10 === 0 || frameIdx === totalFrames - 1) {
      setStatus(`rendered ${frameIdx + 1}/${totalFrames}`);
      const median = medianOf(drawMsSamples).toFixed(1);
      console.log(
        `[nevoflux.render] job=${jobId} frame=${frameIdx + 1}/${totalFrames} ` +
          `drawMs median=${median} n=${drawMsSamples.length} ` +
          `size=${comp.width}x${comp.height}`
      );
    }
  }

  const finalMedian = medianOf(drawMsSamples).toFixed(1);
  console.log(
    `[nevoflux.render] POC GATE drawSnapshot median=${finalMedian}ms ` +
      `n=${drawMsSamples.length} at ${comp.width}x${comp.height}`
  );

  console.log('[render.js] total seekAck received: ' + _ackCount + ' / expected ' + totalFrames);

  setStatus(`reporting done (${totalFrames} frames)`);
  await bridge().reportDone(jobId, totalFrames);
  await bridge().unregisterJob(jobId);

  setStatus(`done — ${totalFrames} frames for job ${jobId}`);
}

runRenderLoop().catch(async (err) => {
  const msg = (err && err.message) || String(err);
  setStatus(`error: ${msg}`);
  try {
    if (jobId) {
      await bridge().reportFailed(jobId, msg);
      await bridge().unregisterJob(jobId);
    }
  } catch (_e) {
    // Best effort — page is already in an error state.
  }
});

setStatus(`render page loaded — job ${jobId}, comp ${compositionId}`);
