// Determinism patches for the composition iframe.
//
// Spec: docs/superpowers/specs/2026-04-19-video-skill-design.md §4.2
//
// The patches must run INSIDE the iframe's own JS context — the render page is
// chrome-privileged but the composition iframe (srcdoc) is a null-principal
// document, which blocks any cross-origin property write from the parent
// window. So instead of calling functions that reach into contentWindow, we
// export the patch body as a string and prepend it to the composition HTML
// as an inline <script>. Runtime control (setRenderTime) flows via
// postMessage.

/**
 * JavaScript source embedded as the first <script> inside the composition
 * iframe. Installs determinism patches on that iframe's own globals.
 */
export const PATCHES_SOURCE = `
(function () {
  // Mulberry32 seeded PRNG for Math.random.
  var _prngS = 42 | 0;
  Math.random = function () {
    _prngS |= 0;
    _prngS = (_prngS + 0x6D2B79F5) | 0;
    var t = Math.imul(_prngS ^ (_prngS >>> 15), 1 | _prngS);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };

  // Timeline-driven Date.now / performance.now.
  var BASE_WALL_CLOCK = 1700000000000;
  window.__nfRenderTime = 0; // seconds
  var origDate = Date;
  var PatchedDate = function () {
    if (arguments.length === 0) {
      return new origDate(BASE_WALL_CLOCK + window.__nfRenderTime * 1000);
    }
    return new (Function.prototype.bind.apply(origDate, [null].concat(Array.prototype.slice.call(arguments))))();
  };
  PatchedDate.now = function () { return BASE_WALL_CLOCK + window.__nfRenderTime * 1000; };
  PatchedDate.parse = origDate.parse;
  PatchedDate.UTC = origDate.UTC;
  PatchedDate.prototype = origDate.prototype;
  // eslint-disable-next-line no-global-assign
  Date = PatchedDate;
  performance.now = function () { return window.__nfRenderTime * 1000; };

  // Fetch whitelist.
  var origFetch = window.fetch.bind(window);
  window.fetch = function (url, opts) {
    var u = typeof url === 'string' ? url : (url && url.url) || '';
    if (u.startsWith('assets/') || u.startsWith('./assets/')) return origFetch(url, opts);
    if (u.startsWith('https://esm.sh/')) return origFetch(url, opts);
    return Promise.reject(new Error('fetch blocked in render: ' + u));
  };

  // GSAP ticker freeze after load (if composition uses GSAP).
  window.addEventListener('DOMContentLoaded', function () {
    if (window.gsap && window.gsap.ticker) {
      window.gsap.ticker.sleep();
      window.gsap.ticker.lagSmoothing(0);
    }
  });

  // crypto.getRandomValues + crypto.randomUUID (deterministic via patched Math.random).
  if (window.crypto) {
    window.crypto.getRandomValues = function (array) {
      for (var i = 0; i < array.length; i++) array[i] = Math.floor(Math.random() * 256);
      return array;
    };
    window.crypto.randomUUID = function () {
      var hex = function () { return Math.floor(Math.random() * 16).toString(16); };
      var s = function (n) { var out = ''; for (var i = 0; i < n; i++) out += hex(); return out; };
      var variantDigit = (8 + Math.floor(Math.random() * 4)).toString(16);
      return s(8) + '-' + s(4) + '-4' + s(3) + '-' + variantDigit + s(3) + '-' + s(12);
    };
  }

  // postMessage-driven render-time clock updates + timeline seek bridge.
  //
  // Render.js sends two messages per frame: 'setRenderTime' (advances the
  // patched clock so Date.now/performance.now/Math.random are deterministic)
  // and 'seek' (advances every paused GSAP timeline registered in
  // window.__timelines). Without the seek bridge, compositions whose
  // elements start at opacity:0 (the standard fromTo pattern in our
  // /video templates) capture as 900 identical t=0 frames — i.e., a
  // 76KB all-black 30-second MP4.
  window.addEventListener('message', function (evt) {
    var d = evt.data;
    if (!d || typeof d.seconds !== 'number') return;
    if (d.__nf_type === 'setRenderTime') {
      window.__nfRenderTime = d.seconds;
    } else if (d.__nf_type === 'seek') {
      var tls = window.__timelines || [];
      window.__nfSeekCalls = (window.__nfSeekCalls || 0) + 1;
      window.__nfSeekLastT = d.seconds;
      window.__nfSeekTlCount = tls.length;

      // Clip visibility windowing. Templates ship with .clip { visibility:
      // hidden } so non-active scenes don't bleed in during render. Each
      // .clip carries data-start (seconds) and data-duration (seconds);
      // a clip is on iff start <= t < start + duration. Without this every
      // scene container stays hidden -> 76KB all-black MP4 even with the
      // seek bridge wired.
      var clips = document.querySelectorAll('.clip');
      for (var c = 0; c < clips.length; c++) {
        var el = clips[c];
        var s = parseFloat(el.dataset.start || '0');
        var du = parseFloat(el.dataset.duration);
        if (!isFinite(du)) du = Infinity;
        el.style.visibility = (d.seconds >= s && d.seconds < s + du) ? 'visible' : 'hidden';
      }

      for (var i = 0; i < tls.length; i++) {
        try { tls[i].seek(d.seconds); } catch (_) {}
      }
      // Ack back to parent so render.js can verify delivery + capture state.
      try {
        parent.postMessage({
          __nf_type: 'seekAck',
          t: d.seconds,
          tlCount: tls.length,
          clipCount: clips.length,
          callNo: window.__nfSeekCalls,
        }, '*');
      } catch (_) {}
      if (window.__nfSeekCalls === 1) {
        console.log('[render-patches] first seek t=' + d.seconds + ' tls=' + tls.length + ' clips=' + clips.length);
      }
    }
  });

  // ── Composition-ready signal ──────────────────────────────────────────
  //
  // Render.js' iframe-load event resolves when the HTML is parsed + classic
  // scripts done, but composition modules import GSAP from
  // https://esm.sh/gsap (cross-origin ESM) which can finish AFTER load. If
  // render starts iterating frames while __timelines is still empty, every
  // tl.seek loop iterates 0 times and the 900 captured frames stay at
  // initial opacity:0 state — the all-black-MP4 symptom.
  //
  // Wait until __timelines has at least one entry, OR a 5s safety budget
  // expires, then post 'iframeReady' to parent. Render.js blocks on this
  // before the frame loop.
  function _signalReady() {
    var tls = window.__timelines || [];
    var threes = window.__threeRenderers || [];
    try {
      parent.postMessage({
        __nf_type: 'iframeReady',
        tlCount: tls.length,
        threeCount: threes.length,
      }, '*');
    } catch (_) {}
  }
  function _waitForTimelines() {
    // Use a setTimeout-counted budget instead of performance.now() because
    // we just patched performance.now() to return __nfRenderTime * 1000
    // (always 0 here) -- the patched clock would make timeout impossible.
    var ticks = 0;
    var maxTicks = 100; // 100 * 50ms = 5s safety budget
    function poll() {
      var tls = window.__timelines || [];
      if (tls.length > 0 || ticks++ >= maxTicks) {
        _signalReady();
        return;
      }
      setTimeout(poll, 50);
    }
    poll();
  }
  if (document.readyState === 'complete') {
    _waitForTimelines();
  } else {
    window.addEventListener('load', _waitForTimelines);
  }
})();
`;

/**
 * Wrap the composition's HTML by injecting the patches <script> as the very
 * first executable element inside <head>. If the HTML has no <head> we wrap
 * it in a minimal document so the patches still run first.
 */
export function withPatches(compositionHtml) {
  const patchTag =
    '<script>' + PATCHES_SOURCE + '</script>';

  // The `\b` word boundary is critical: without it, `<head[^>]*>` greedily
  // matches `<HEADLINE_LINE_1>` inside template HTML comments (e.g. the
  // tiktok-hook AGENT USAGE block) because `<HEAD` + `LINE_1` + `>` satisfies
  // the pattern. That caused the determinism patches (setRenderTime / seek
  // bridge / clip windowing) to be injected mid-comment instead of after
  // the real <head>, leaving the patches script un-executed. Symptom:
  // render had no scene visibility windowing → all .clip elements remained
  // visible simultaneously → the last DOM scene (#scene-3, accent color)
  // covered the others, producing a single-color MP4.
  const headMatch = compositionHtml.match(/<head\b[^>]*>/i);
  if (headMatch) {
    const insertAt = headMatch.index + headMatch[0].length;
    return (
      compositionHtml.slice(0, insertAt) +
      patchTag +
      compositionHtml.slice(insertAt)
    );
  }

  const htmlMatch = compositionHtml.match(/<html\b[^>]*>/i);
  if (htmlMatch) {
    const insertAt = htmlMatch.index + htmlMatch[0].length;
    return (
      compositionHtml.slice(0, insertAt) +
      '<head>' +
      patchTag +
      '</head>' +
      compositionHtml.slice(insertAt)
    );
  }

  return (
    '<!doctype html><html><head>' +
    patchTag +
    '</head><body>' +
    compositionHtml +
    '</body></html>'
  );
}

/**
 * Advance the render-time clock in the composition iframe.
 * Uses postMessage because the iframe lives in a null-principal origin.
 */
export function setRenderTime(iframeWindow, seconds) {
  iframeWindow.postMessage({ __nf_type: 'setRenderTime', seconds }, '*');
}
