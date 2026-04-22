// composition-linter/rules/captions.js — ported from the upstream captions rule set.
// Rule IDs use the `comp/*` prefix.
// See ../LICENSE-NOTICE.md for attribution.
//
// Adaptation notes:
//
//   Upstream receives `ctx.scripts[].content` and `ctx.styles[].content` (plain
//   strings) and `ctx.options.filePath`.  Our LintContext exposes DOM element
//   arrays, so we read `.textContent` for scripts/styles.  The `filePath` check
//   for caption-file detection is replaced by the CSS class name heuristic
//   (which the upstream already uses as a fallback).
//
//   All rule IDs are kebab-cased and prefixed `comp/`, matching the rest of the
//   linter (e.g. `comp/caption-exit-missing-hard-kill`).

import { push } from '../utils.js';

// ── Rule: caption_exit_missing_hard_kill ──────────────────────────────────

function ruleCaptionExitMissingHardKill(ctx, report) {
  for (const script of ctx.scripts) {
    if (script.getAttribute('src')) continue;
    const content = script.textContent || '';

    const hasExitTween = /\.to\s*\([^,]+,\s*\{[^}]*opacity\s*:\s*0/.test(content);
    const hasHardKill =
      /\.set\s*\([^,]+,\s*\{[^}]*(?:visibility\s*:\s*["']hidden["']|opacity\s*:\s*0)/.test(
        content,
      );
    const hasCaptionLoop =
      /forEach|\.forEach\s*\(/.test(content) &&
      /createElement|caption|group|cg-/.test(content);

    if (hasCaptionLoop && hasExitTween && !hasHardKill) {
      push(report, {
        severity: 'warning',
        rule_id: 'comp/caption-exit-missing-hard-kill',
        message:
          'Caption exit animations (tl.to with opacity: 0) detected without a hard tl.set kill. ' +
          'Exit tweens can fail when karaoke word-level tweens conflict, leaving captions stuck on screen.',
        fix_hint:
          'Add `tl.set(groupEl, { opacity: 0, visibility: "hidden" }, group.end)` after every ' +
          'exit tl.to animation as a deterministic kill.',
      });
    }
  }
}

// ── Rule: caption_text_overflow_risk ──────────────────────────────────────

function ruleCaptionTextOverflowRisk(ctx, report) {
  for (const style of ctx.styles) {
    const content = style.textContent || '';
    const captionBlocks = content.matchAll(
      /(\.caption[-_]?(?:group|container|text|line|word)|#caption[-_]?container)\s*\{([^}]+)\}/gi,
    );
    for (const [, selector, body] of captionBlocks) {
      if (!body) continue;
      const hasNowrap = /white-space\s*:\s*nowrap/i.test(body);
      const hasMaxWidth = /max-width/i.test(body);
      if (hasNowrap && !hasMaxWidth) {
        push(report, {
          severity: 'warning',
          rule_id: 'comp/caption-text-overflow-risk',
          selector: (selector ?? '').trim(),
          message: `Caption selector "${(selector ?? '').trim()}" has white-space: nowrap but no max-width. Long phrases will clip off-screen.`,
          fix_hint:
            'Add max-width: 1600px (landscape) or max-width: 900px (portrait) and overflow: hidden.',
        });
      }
    }
  }
}

// ── Rule: caption_transcript_not_inline ──────────────────────────────────
//
// Adaptation note: upstream checks `options.filePath` for the "caption file"
// heuristic.  Our context has no filePath; we rely solely on the CSS class
// name heuristic (same as the upstream's fallback branch).

function ruleCaptionTranscriptNotInline(ctx, report) {
  // Only check files that look like caption compositions
  const isCaptionFile = ctx.styles.some(s =>
    /\.caption[-_]?(?:group|word)/i.test(s.textContent || ''),
  );
  if (!isCaptionFile) return;

  const allScript = ctx.scripts
    .filter(s => !s.getAttribute('src'))
    .map(s => s.textContent || '')
    .join('\n');

  const hasInlineTranscript = /(?:const|let|var)\s+(?:TRANSCRIPT|script)\s*=\s*\[/.test(
    allScript,
  );
  const hasFetchTranscript = /fetch\s*\(\s*["'][^"']*transcript/i.test(allScript);

  if (!hasInlineTranscript && hasFetchTranscript) {
    push(report, {
      severity: 'warning',
      rule_id: 'comp/caption-transcript-not-inline',
      message:
        'Captions composition loads transcript via fetch(). The studio caption editor ' +
        'requires an inline `var TRANSCRIPT = [...]` array to detect and edit captions.',
      fix_hint:
        'Embed the transcript as `var TRANSCRIPT = [{ "text": "...", "start": 0, "end": 1 }, ...]` ' +
        'with JSON-quoted property keys. See the captions skill for details.',
    });
  }

  if (hasInlineTranscript) {
    // Verify the inline transcript can be parsed as JSON
    const varPattern = /(?:const|let|var)\s+(?:TRANSCRIPT|script)\s*=\s*(\[[\s\S]*?\]);/;
    const match = allScript.match(varPattern);
    if (match?.[1]) {
      try {
        JSON.parse(match[1]);
      } catch {
        push(report, {
          severity: 'warning',
          rule_id: 'comp/caption-transcript-parse-error',
          message:
            'Inline TRANSCRIPT array is not valid JSON. The studio caption editor may fail ' +
            'to parse it. Common cause: unquoted property keys with apostrophes in text.',
          fix_hint:
            'Use JSON-quoted keys: { "text": "don\'t", "start": 0, "end": 1 } instead of ' +
            '{ text: "don\'t", start: 0, end: 1 }.',
        });
      }
    }
  }
}

// ── Rule: caption_container_relative_position ─────────────────────────────

function ruleCaptionContainerRelativePosition(ctx, report) {
  for (const style of ctx.styles) {
    const content = style.textContent || '';
    const captionBlocks = content.matchAll(
      /(\.caption[-_]?(?:group|container|text|line)|#caption[-_]?container)\s*\{([^}]+)\}/gi,
    );
    for (const [, selector, body] of captionBlocks) {
      if (!body) continue;
      if (/position\s*:\s*relative/i.test(body)) {
        push(report, {
          severity: 'warning',
          rule_id: 'comp/caption-container-relative-position',
          selector: (selector ?? '').trim(),
          message: `Caption selector "${(selector ?? '').trim()}" uses position: relative which causes overflow and breaks caption stacking.`,
          fix_hint: 'Use position: absolute for all caption elements.',
        });
      }
    }
  }
}

// ── Rule: caption_overflow_clips_scaled_words ─────────────────────────────

function ruleCaptionOverflowClipsScaledWords(ctx, report) {
  const hasScaledWords = ctx.scripts.some(
    s =>
      /scale\s*:\s*1\.[2-9]/.test(s.textContent || '') &&
      /caption|word|cg-/.test(s.textContent || ''),
  );
  if (!hasScaledWords) return;

  for (const style of ctx.styles) {
    const content = style.textContent || '';
    const captionBlocks = content.matchAll(
      /(\.caption[-_]?(?:group|container)|#caption[-_]?(?:layer|container))\s*\{([^}]+)\}/gi,
    );
    for (const [, selector, body] of captionBlocks) {
      if (!body) continue;
      if (/overflow\s*:\s*hidden/i.test(body)) {
        push(report, {
          severity: 'warning',
          rule_id: 'comp/caption-overflow-clips-scaled-words',
          selector: (selector ?? '').trim(),
          message: `"${(selector ?? '').trim()}" has overflow: hidden but GSAP scales caption words above 1.0x. Scaled emphasis words and their glow effects will be clipped.`,
          fix_hint:
            'Use overflow: visible on caption containers. Rely on fitTextFontSize with reduced maxWidth to prevent overflow instead.',
        });
      }
    }
  }
}

// ── Rule: caption_textshadow_on_group_container ───────────────────────────

function ruleCaptionTextshadowOnGroupContainer(ctx, report) {
  const isCaptionFile = ctx.styles.some(s =>
    /\.caption[-_]?(?:group|word)/i.test(s.textContent || ''),
  );
  if (!isCaptionFile) return;

  for (const script of ctx.scripts) {
    if (script.getAttribute('src')) continue;
    const content = script.textContent || '';

    // Detect textShadow tweened on a group container (div with child word spans)
    const groupShadowPattern =
      /\.to\s*\(\s*(?:div|groupEl|el|captionEl|document\.getElementById\s*\(\s*["']cg-)\s*[^,]*,\s*\{[^}]*textShadow/g;
    // Also catch selector-based targeting of group containers
    const selectorShadowPattern =
      /\.to\s*\(\s*["'](?:#cg-\d+|\.caption[-_]?group)["']\s*,\s*\{[^}]*textShadow/g;

    if (groupShadowPattern.test(content) || selectorShadowPattern.test(content)) {
      push(report, {
        severity: 'warning',
        rule_id: 'comp/caption-textshadow-on-group-container',
        message:
          'textShadow is tweened on a caption group container. When children have semi-transparent ' +
          'color (e.g., inactive karaoke words at rgba opacity), the glow renders as a visible ' +
          'rectangle behind the entire group.',
        fix_hint:
          'Apply textShadow to individual active word elements instead of the group container. ' +
          'Use scale on the group for bass-reactive pulsing.',
      });
    }
  }
}

// ── Rule: caption_fittext_scale_mismatch ──────────────────────────────────

function ruleCaptionFittextScaleMismatch(ctx, report) {
  for (const script of ctx.scripts) {
    if (script.getAttribute('src')) continue;
    const content = script.textContent || '';

    const fitTextMatch = content.match(/fitTextFontSize\s*\([^)]*maxWidth\s*:\s*(\d+)/);
    if (!fitTextMatch) continue;
    const maxWidth = parseInt(fitTextMatch[1] ?? '0', 10);
    if (!maxWidth) continue;

    // Find max scale on caption words
    const scaleMatches = [...content.matchAll(/scale\s*:\s*(1\.\d+)/g)];
    const captionContext = /caption|word|cg-|karaoke/i.test(content);
    if (!captionContext || scaleMatches.length === 0) continue;

    let maxScale = 1;
    for (const m of scaleMatches) {
      const val = parseFloat(m[1] ?? '1');
      if (val > maxScale) maxScale = val;
    }

    // Check if maxWidth * maxScale exceeds safe bounds (1920 - reasonable margins)
    const effectiveWidth = maxWidth * maxScale;
    if (effectiveWidth > 1760) {
      push(report, {
        severity: 'warning',
        rule_id: 'comp/caption-fittext-scale-mismatch',
        message:
          `fitTextFontSize uses maxWidth: ${maxWidth}px but emphasis words scale up to ${maxScale}x. ` +
          `Effective width ${Math.round(effectiveWidth)}px may overflow the composition (1920px minus margins).`,
        fix_hint: `Reduce maxWidth to ${Math.floor(1700 / maxScale)}px to leave headroom for scaled emphasis words.`,
      });
    }
  }
}

// ── Export ─────────────────────────────────────────────────────────────────

export default [
  ruleCaptionExitMissingHardKill,
  ruleCaptionTextOverflowRisk,
  ruleCaptionTranscriptNotInline,
  ruleCaptionContainerRelativePosition,
  ruleCaptionOverflowClipsScaledWords,
  ruleCaptionTextshadowOnGroupContainer,
  ruleCaptionFittextScaleMismatch,
];
