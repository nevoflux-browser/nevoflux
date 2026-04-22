// composition-linter/rules/media.js — ported from the upstream media rule set.
// Rule IDs use the `comp/*` prefix.
// See ../LICENSE-NOTICE.md for attribution.
//
// Adaptation notes:
//
//   Upstream used OpenTag objects (raw tag strings from a streaming parser)
//   with `readAttr(tag.raw, attr)` to extract attributes and a flat `tags`
//   array that included both media and non-media elements.  We use DOM element
//   objects accessible via `ctx.mediaElements` (audio/video) and
//   `ctx.doc.querySelectorAll(...)` for broader queries.  `ctx.raw` is used
//   where raw regex scanning over the full HTML source is required.
//
//   Two rules rely on positional nesting logic that upstream solved by tracking
//   source byte offsets in the tag list.  We adapt these to use DOM
//   `contains()` for parent/child relationships, which is semantically
//   equivalent.
//
//   Rule IDs: camelCase codes in the upstream source are mapped to
//   kebab-case `comp/*` IDs following the pattern established in the other
//   ported rule files.

import { push } from '../utils.js';

// ── Helpers ────────────────────────────────────────────────────────────────

/** Truncate a code snippet to `max` chars for issue messages. */
function truncateSnippet(str, max = 120) {
  if (!str) return undefined;
  const s = str.replace(/\s+/g, ' ').trim();
  if (!s) return undefined;
  return s.length > max ? s.slice(0, max) + '…' : s;
}

// ── Rule: duplicate_media_id + duplicate_media_discovery_risk ──────────────
//
// Upstream iterated `tags` filtered by `isMediaTag(tag.name)`.
// We iterate `ctx.mediaElements` (querySelectorAll('audio, video')).
// Fingerprint uses tagName + src + data-start + data-duration attributes.

function ruleDuplicateMedia(ctx, report) {
  const mediaById = new Map();         // id → Element[]
  const mediaFingerprintCounts = new Map(); // fingerprint → count

  for (const el of ctx.mediaElements) {
    const elementId = el.getAttribute('id');
    if (elementId) {
      const existing = mediaById.get(elementId) || [];
      existing.push(el);
      mediaById.set(elementId, existing);
    }
    const fingerprint = [
      el.tagName.toLowerCase(),
      el.getAttribute('src') || '',
      el.getAttribute('data-start') || '',
      el.getAttribute('data-duration') || '',
    ].join('|');
    mediaFingerprintCounts.set(fingerprint, (mediaFingerprintCounts.get(fingerprint) || 0) + 1);
  }

  for (const [elementId, elements] of mediaById) {
    if (elements.length < 2) continue;
    push(report, {
      severity: 'error',
      rule_id: 'comp/duplicate-media-id',
      element_id: elementId,
      message: `Media id "${elementId}" is defined multiple times.`,
      fix_hint: 'Give each media element a unique id so preview and producer discover the same media graph.',
      snippet: truncateSnippet(elements[0].outerHTML),
    });
  }

  for (const [fingerprint, count] of mediaFingerprintCounts) {
    if (count < 2) continue;
    const [tagName, src, dataStart, dataDuration] = fingerprint.split('|');
    push(report, {
      severity: 'warning',
      rule_id: 'comp/duplicate-media-discovery-risk',
      message: `Detected ${count} matching ${tagName} entries with the same source/start/duration.`,
      fix_hint: 'Avoid duplicated media nodes that can be discovered twice during compilation.',
      snippet: truncateSnippet(`${tagName} src=${src} data-start=${dataStart} data-duration=${dataDuration}`),
    });
  }
}

// ── Rule: video_missing_muted ──────────────────────────────────────────────
//
// Upstream checked `tag.name === "video"` and used `/\bmuted\b/i.test(tag.raw)`.
// DOM elements expose `.muted` as a boolean property; we use that directly.

function ruleVideoMissingMuted(ctx, report) {
  for (const el of ctx.mediaElements) {
    if (el.tagName.toLowerCase() !== 'video') continue;
    if (!el.getAttribute('data-start')) continue;
    if (el.muted) continue;
    const elementId = el.getAttribute('id') || undefined;
    push(report, {
      severity: 'error',
      rule_id: 'comp/video-missing-muted',
      element_id: elementId,
      message: `<video${elementId ? ` id="${elementId}"` : ''}> has data-start but is not muted. The framework expects video to be muted with a separate <audio> element for sound.`,
      fix_hint: 'Add the `muted` attribute to the <video> tag and use a separate <audio> element with the same src for audio playback.',
      snippet: truncateSnippet(el.outerHTML),
    });
  }
}

// ── Rule: video_nested_in_timed_element ───────────────────────────────────
//
// Upstream used source-offset nesting detection on the flat tag list.
// We use DOM `contains()` which is semantically equivalent and simpler.
// The rule fires when a <video data-start> is a descendant of any other
// non-media element that also carries data-start.

function ruleVideoNestedInTimedElement(ctx, report) {
  // Collect all timed non-media elements that could act as parents.
  const timedNonMedia = Array.from(
    ctx.doc.querySelectorAll('[data-start]'),
  ).filter(el => {
    const tag = el.tagName.toLowerCase();
    return tag !== 'video' && tag !== 'audio';
  });

  for (const videoEl of ctx.mediaElements) {
    if (videoEl.tagName.toLowerCase() !== 'video') continue;
    if (!videoEl.getAttribute('data-start')) continue;

    for (const parent of timedNonMedia) {
      if (!parent.contains(videoEl)) continue;
      const parentId = parent.getAttribute('id') || undefined;
      push(report, {
        severity: 'error',
        rule_id: 'comp/video-nested-in-timed-element',
        element_id: videoEl.getAttribute('id') || undefined,
        message: `<video> with data-start is nested inside <${parent.tagName.toLowerCase()}${parentId ? ` id="${parentId}"` : ''}> which also has data-start. The framework cannot manage playback of nested media — video will be FROZEN in renders.`,
        fix_hint: 'Move the <video> to be a direct child of the stage, or remove data-start from the wrapper div (use it as a non-timed visual container).',
        snippet: truncateSnippet(videoEl.outerHTML),
      });
      break; // report once per video element
    }
  }
}

// ── Rule: self_closing_media_tag ───────────────────────────────────────────
//
// DOMParser silently repairs self-closing audio/video tags, so we cannot
// detect them from the DOM.  We scan ctx.raw with a regex, matching the
// upstream approach exactly.

function ruleSelfClosingMediaTag(ctx, report) {
  const selfClosingMediaRe = /<(audio|video)\b[^>]*\/>/gi;
  let m;
  while ((m = selfClosingMediaRe.exec(ctx.raw)) !== null) {
    const tagName = m[1] || 'audio';
    const elementId = (m[0].match(/\bid\s*=\s*["']([^"']+)["']/i) || [])[1] || undefined;
    push(report, {
      severity: 'error',
      rule_id: 'comp/self-closing-media-tag',
      element_id: elementId,
      message: `Self-closing <${tagName}/> is invalid HTML. The browser will leave the tag open, swallowing all subsequent elements as invisible fallback content. This makes compositions INVISIBLE.`,
      fix_hint: `Change <${tagName} .../> to <${tagName} ...></${tagName}> — media elements MUST have explicit closing tags.`,
      snippet: truncateSnippet(m[0]),
    });
  }
}

// ── Rule: placeholder_media_url ────────────────────────────────────────────
//
// Ported verbatim from upstream; adapted to use DOM element src attribute.

const PLACEHOLDER_DOMAINS =
  /\b(placehold\.co|placeholder\.com|placekitten\.com|picsum\.photos|example\.com|via\.placeholder\.com|dummyimage\.com)\b/i;

function rulePlaceholderMediaUrl(ctx, report) {
  for (const el of ctx.mediaElements) {
    const src = el.getAttribute('src');
    if (!src) continue;
    if (!PLACEHOLDER_DOMAINS.test(src)) continue;
    const elementId = el.getAttribute('id') || undefined;
    const tagName = el.tagName.toLowerCase();
    push(report, {
      severity: 'error',
      rule_id: 'comp/placeholder-media-url',
      element_id: elementId,
      message: `<${tagName}${elementId ? ` id="${elementId}"` : ''}> uses a placeholder URL that will 404 at render time: ${src.slice(0, 80)}`,
      fix_hint: 'Replace with a real media URL. Placeholder domains will 404 at render time.',
      snippet: truncateSnippet(el.outerHTML),
    });
  }
}

// ── Rule: base64_media_prohibited ─────────────────────────────────────────
//
// DOMParser normalises src attributes so base64 data URIs are accessible
// via getAttribute.  However the entropy/size heuristics require the raw
// base64 string.  We scan ctx.raw with the same regex as upstream to get
// the full data: value and then apply the heuristics.

function ruleBase64MediaProhibited(ctx, report) {
  const base64MediaRe =
    /src\s*=\s*["'](data:(?:audio|video)\/[^;]+;base64,([A-Za-z0-9+/=]{20,}))["']/gi;
  let m;
  while ((m = base64MediaRe.exec(ctx.raw)) !== null) {
    const b64Data = m[2] || '';
    const sample = b64Data.slice(0, 200);
    const uniqueChars = new Set(sample.replace(/[A-Za-z0-9+/=]/g, c => c)).size;
    const dataSize = Math.round((b64Data.length * 3) / 4);
    const isSuspicious = uniqueChars < 15 || (dataSize > 1000 && dataSize < 50000);
    push(report, {
      severity: 'error',
      rule_id: 'comp/base64-media-prohibited',
      message: `Inline base64 audio/video detected (${(dataSize / 1024).toFixed(0)} KB)${isSuspicious ? ' — likely fabricated data' : ''}. Base64 media is prohibited — it bloats file size and breaks rendering.`,
      fix_hint: 'Use a relative path (assets/music.mp3) or HTTPS URL for the audio/video src. Never embed media as base64.',
      snippet: truncateSnippet((m[1] ?? '').slice(0, 80) + '...'),
    });
  }
}

// ── Rule: media_missing_id + media_missing_src + media_preload_none ─────────
//
// Ported verbatim from upstream; adapted to use DOM element attributes.

function ruleMediaRequiredAttributes(ctx, report) {
  for (const el of ctx.mediaElements) {
    const tagName = el.tagName.toLowerCase();
    const hasDataStart = el.getAttribute('data-start');
    const hasId = el.getAttribute('id');
    const hasSrc = el.getAttribute('src');
    const preload = el.getAttribute('preload');

    if (hasDataStart && !hasId) {
      push(report, {
        severity: 'error',
        rule_id: 'comp/media-missing-id',
        message: `<${tagName}> has data-start but no id attribute. The renderer requires id to discover media elements — this ${tagName === 'audio' ? 'audio will be SILENT' : 'video will be FROZEN'} in renders.`,
        fix_hint: `Add a unique id attribute: <${tagName} id="my-${tagName}" ...>`,
        snippet: truncateSnippet(el.outerHTML),
      });
    }

    if (hasDataStart && hasId && !hasSrc) {
      push(report, {
        severity: 'error',
        rule_id: 'comp/media-missing-src',
        element_id: hasId,
        message: `<${tagName} id="${hasId}"> has data-start but no src attribute. The renderer cannot load this media.`,
        fix_hint: `Add a src attribute to the <${tagName}> element directly. If using <source> children, the renderer still requires src on the parent element.`,
        snippet: truncateSnippet(el.outerHTML),
      });
    }

    if (preload === 'none') {
      push(report, {
        severity: 'warning',
        rule_id: 'comp/media-preload-none',
        element_id: hasId || undefined,
        message: `<${tagName}${hasId ? ` id="${hasId}"` : ''}> has preload="none" which prevents the renderer from loading this media. The compiler strips it for renders, but preview may also have issues.`,
        fix_hint: `Remove preload="none" or change to preload="auto". The framework manages media loading.`,
        snippet: truncateSnippet(el.outerHTML),
      });
    }
  }
}

// ── Export ─────────────────────────────────────────────────────────────────

export default [
  ruleDuplicateMedia,
  ruleVideoMissingMuted,
  ruleVideoNestedInTimedElement,
  ruleSelfClosingMediaTag,
  rulePlaceholderMediaUrl,
  ruleBase64MediaProhibited,
  ruleMediaRequiredAttributes,
];
