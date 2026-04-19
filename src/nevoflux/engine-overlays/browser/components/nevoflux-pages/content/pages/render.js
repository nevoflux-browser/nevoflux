// Render page orchestration. Loads the composition HTML into a
// same-origin iframe with determinism patches, then listens for
// seek commands from the daemon (via extension's bridge push).

import { installPatches, setRenderTime } from './render-patches.js';

const CHUNK_SIZE = 1024 * 1024; // 1 MB PNG chunks

const params = new URLSearchParams(window.location.search);
const jobId = params.get('job_id') || '';
const compositionId = params.get('composition_id') || '';

const statusEl = document.getElementById('status');
const iframe = document.getElementById('composition-iframe');

function setStatus(text) {
  if (statusEl) statusEl.textContent = text;
}

/**
 * Load composition HTML into iframe. The HTML is expected to be
 * served via a blob URL created from content delivered through the
 * bridge. For PoC we use an inline srcdoc; Phase B replaces this
 * with a real artifact-backed loader.
 */
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

  // Install patches IMMEDIATELY after load (before any user script rAF).
  installPatches(iframe.contentWindow);

  // Wait for fonts.ready + readyPromises.
  await iframe.contentWindow.document.fonts.ready;
  const readyPromises = iframe.contentWindow.__readyPromises || [];
  await Promise.all(readyPromises);

  // Pause any __timelines (composition should use {paused: true} but enforce).
  for (const tl of (iframe.contentWindow.__timelines || [])) {
    if (typeof tl.pause === 'function') tl.pause();
  }

  setStatus('ready');
}

/**
 * Seek the composition to time `t` (seconds), force layout/paint,
 * then capture via drawSnapshot. Returns Uint8Array of PNG bytes.
 */
async function seekAndCapture(t, widthPx, heightPx) {
  setRenderTime(iframe.contentWindow, t);

  for (const tl of (iframe.contentWindow.__timelines || [])) {
    if (typeof tl.seek === 'function') tl.seek(t);
  }

  for (const r of (iframe.contentWindow.__threeRenderers || [])) {
    if (r && r.renderer && r.scene && r.camera) {
      r.renderer.render(r.scene, r.camera);
    }
  }

  // Seek <video>/<audio> elements (best-effort; v1 enforces at most one of each).
  const medias = iframe.contentWindow.document.querySelectorAll('video, audio');
  await Promise.all([...medias].map((m) => new Promise((res) => {
    if (Math.abs(m.currentTime - t) < 0.001) { res(); return; }
    const timeout = setTimeout(res, 1000);
    m.addEventListener('seeked', () => { clearTimeout(timeout); res(); }, { once: true });
    m.currentTime = t;
  })));

  // Double RAF to let layout + paint settle.
  await new Promise((r) => iframe.contentWindow.requestAnimationFrame(r));
  await new Promise((r) => iframe.contentWindow.requestAnimationFrame(r));

  // drawSnapshot via privileged parent actor.
  // iframe.browsingContext is available because the iframe is in a chrome-privileged page.
  const bc = iframe.browsingContext || iframe.frameLoader?.browsingContext;
  if (!bc) throw new Error('no browsingContext on iframe');
  const wgp = bc.currentWindowGlobal;
  if (!wgp) throw new Error('no WindowGlobalParent');

  const rect = new DOMRect(0, 0, widthPx, heightPx);
  // drawSnapshot(rect, scale, backgroundColor, fullViewport) -> Promise<ImageBitmap>
  const bitmap = await wgp.drawSnapshot(rect, 1, 'transparent');

  // Convert ImageBitmap -> PNG bytes via OffscreenCanvas.
  const canvas = new OffscreenCanvas(widthPx, heightPx);
  const ctx = canvas.getContext('2d');
  ctx.drawImage(bitmap, 0, 0);
  const blob = await canvas.convertToBlob({ type: 'image/png' });
  const buf = await blob.arrayBuffer();
  return new Uint8Array(buf);
}

/**
 * Split PNG bytes into 1 MB chunks and post back to daemon via
 * browser.runtime.sendMessage (the extension background layer
 * forwards them to the daemon bridge).
 */
function* chunkPng(bytes) {
  const total = Math.ceil(bytes.length / CHUNK_SIZE);
  for (let i = 0; i < total; i++) {
    const start = i * CHUNK_SIZE;
    const end = Math.min(bytes.length, start + CHUNK_SIZE);
    yield {
      chunk_idx: i,
      total_chunks: total,
      is_last: i === total - 1,
      bytes: bytes.subarray(start, end),
    };
  }
}

async function sendFrameChunks(frameIdx, bytes) {
  for (const c of chunkPng(bytes)) {
    await browser.runtime.sendMessage({
      type: 'bg:canvas_video_frame_chunk',
      payload: {
        job_id: jobId,
        frame_idx: frameIdx,
        chunk_idx: c.chunk_idx,
        total_chunks: c.total_chunks,
        is_last: c.is_last,
        bytes: Array.from(c.bytes), // structured clone; serialized as numeric array
      },
    });
  }
}

// Message listener: daemon -> extension -> this page
browser.runtime.onMessage.addListener(async (msg) => {
  if (!msg || typeof msg !== 'object') return;
  if (msg.target_job_id && msg.target_job_id !== jobId) return;

  switch (msg.type) {
    case 'render.load_composition': {
      await loadComposition(msg.html, msg.width, msg.height);
      await browser.runtime.sendMessage({
        type: 'bg:canvas_video_ready',
        payload: { job_id: jobId },
      });
      return { ok: true };
    }
    case 'render.seek_and_capture': {
      const t0 = performance.now();
      const bytes = await seekAndCapture(msg.t, msg.width, msg.height);
      const drawMs = performance.now() - t0;
      await sendFrameChunks(msg.frame_idx, bytes);
      return { ok: true, draw_ms: drawMs, frame_size: bytes.length };
    }
    case 'render.close': {
      setStatus('closing');
      window.close();
      return { ok: true };
    }
  }
});

setStatus(`render page ready — job ${jobId}, comp ${compositionId}`);
