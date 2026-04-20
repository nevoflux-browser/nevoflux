// Render page orchestration — page-driven pull loop.
//
// Post-actor-rework (2026-04-20) the page is the driver: it fetches the
// composition via NevofluxBridge.canvasVideo.getComposition, iterates each
// frame locally, asks the parent JSActor to drawSnapshot the composition
// iframe, and forwards PNG bytes back to the daemon in 1 MB chunks.

import {
  installPatches,
  setRenderTime,
} from 'chrome://nevoflux/content/pages/render-patches.js';

const CHUNK_SIZE = 1024 * 1024; // 1 MB PNG chunks

const params = new URLSearchParams(window.location.search);
const jobId = params.get('job_id') || '';
const compositionId = params.get('composition_id') || '';

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

  const loaded = new Promise((resolve) => {
    iframe.addEventListener('load', () => resolve(), { once: true });
  });
  iframe.srcdoc = htmlText;
  await loaded;

  // Install determinism patches immediately after load, before any user rAF.
  installPatches(iframe.contentWindow);

  // Wait for web fonts + any composition-declared readiness signals.
  if (iframe.contentWindow.document.fonts) {
    await iframe.contentWindow.document.fonts.ready;
  }
  const readyPromises = iframe.contentWindow.__readyPromises || [];
  await Promise.all(readyPromises);

  // Pause all timelines — compositions should declare them paused but enforce.
  for (const tl of iframe.contentWindow.__timelines || []) {
    if (typeof tl.pause === 'function') tl.pause();
  }
}

async function seekFrame(t) {
  setRenderTime(iframe.contentWindow, t);

  for (const tl of iframe.contentWindow.__timelines || []) {
    if (typeof tl.seek === 'function') tl.seek(t);
  }

  for (const r of iframe.contentWindow.__threeRenderers || []) {
    if (r && r.renderer && r.scene && r.camera) {
      r.renderer.render(r.scene, r.camera);
    }
  }

  // Seek <video>/<audio> (v1 allows at most one of each).
  const medias = iframe.contentWindow.document.querySelectorAll('video, audio');
  await Promise.all(
    [...medias].map(
      (m) =>
        new Promise((res) => {
          if (Math.abs(m.currentTime - t) < 0.001) {
            res();
            return;
          }
          const timeout = setTimeout(res, 1000);
          m.addEventListener(
            'seeked',
            () => {
              clearTimeout(timeout);
              res();
            },
            { once: true }
          );
          m.currentTime = t;
        })
    )
  );

  // Double RAF so layout + paint settle before drawSnapshot.
  await new Promise((r) => iframe.contentWindow.requestAnimationFrame(r));
  await new Promise((r) => iframe.contentWindow.requestAnimationFrame(r));
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

  setStatus(`fetching composition for job ${jobId}`);
  const comp = await bridge().getComposition(jobId);
  if (!comp || !comp.html) {
    throw new Error(`getComposition returned no html (got ${JSON.stringify(comp)})`);
  }

  setStatus(`loading composition (${comp.width}×${comp.height}, ${comp.duration_sec}s @ ${comp.fps}fps)`);
  await loadComposition(comp.html, comp.width, comp.height);

  setStatus('registering job with parent actor');
  await bridge().registerJob(jobId);

  const totalFrames = Math.ceil(comp.duration_sec * comp.fps);
  setStatus(`rendering ${totalFrames} frames`);

  for (let frameIdx = 0; frameIdx < totalFrames; frameIdx++) {
    const t = frameIdx / comp.fps;
    await seekFrame(t);

    const res = await bridge().drawFrame(jobId, comp.width, comp.height);
    if (!res || !res.ok) {
      throw new Error(
        `drawFrame failed at frame ${frameIdx}: ${res?.error || 'unknown'}`
      );
    }
    await forwardPng(frameIdx, res.bytes);

    if (frameIdx % 10 === 0 || frameIdx === totalFrames - 1) {
      setStatus(`rendered ${frameIdx + 1}/${totalFrames}`);
    }
  }

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
