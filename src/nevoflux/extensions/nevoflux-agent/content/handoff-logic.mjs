/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * DOM-free handoff logic for Record & Replay (Plan 3, Task 1).
 *
 * Pure functions: no Date.now(), no Math.random(), no DOM, no Node built-ins.
 * The caller supplies any varying seed inside `parts`.
 */

/**
 * FNV-1a 32-bit hash of a string, returned as an 8-char lowercase hex string.
 * @param {string} str
 * @returns {string}
 */
function fnv1a32(str) {
  // FNV offset basis and prime for 32-bit
  let hash = 0x811c9dc5;
  for (let i = 0; i < str.length; i++) {
    hash ^= str.charCodeAt(i);
    // Multiply by FNV prime 0x01000193, keeping result in 32-bit unsigned range
    hash = (hash * 0x01000193) >>> 0;
  }
  return hash.toString(16).padStart(8, '0');
}

/**
 * Mint a deterministic recording id from caller-supplied seed parts.
 *
 * `parts` should include any varying seed (e.g. tabId + monotonic counter)
 * so the caller controls uniqueness; this function stays pure.
 *
 * @param {string[]} parts  Array of strings joined with '|' before hashing.
 * @returns {string}        e.g. "rec_3d2f9a1b"
 */
export function makeRecordingId(parts) {
  const slug = fnv1a32(parts.join('|'));
  return `rec_${slug}`;
}

/**
 * Build the absolute trace file path for a recording.
 *
 * Uses forward-slash join (does NOT depend on Node's `path` module) so the
 * result is consistent across platforms and safe for extension/daemon use.
 *
 * @param {string} recordingsDir  Absolute directory path (forward slashes).
 * @param {string} recordingId    e.g. "rec_3d2f9a1b"
 * @returns {string}              e.g. "/data/recordings/rec_3d2f9a1b.jsonl"
 */
export function recordingTracePath(recordingsDir, recordingId) {
  return `${recordingsDir}/${recordingId}.jsonl`;
}

/**
 * Build the skill-creator opening prompt for trigger A (stop → agent session).
 *
 * The prompt instructs the skill-creator agent to:
 *   1. Read the NDJSON trace at `tracePath` (header line first, then step
 *      lines sorted by ts_ms).
 *   2. Follow the **Record & Replay** section of the skill to generate
 *      SKILL.md from the trace.
 *
 * Both `tracePath` and `goalHint` are embedded literally so the agent can
 * act on them directly.
 *
 * @param {{ tracePath: string, goalHint: string }} options
 * @returns {string}
 */
export function buildSkillCreatorOpeningPrompt({ tracePath, goalHint }) {
  return `A browser recording is ready. Your task is to create a reusable skill from it.

Recording trace path: ${tracePath}
Goal: ${goalHint}

Instructions:
1. Read the NDJSON trace at the path above. The first line is the header (type: "header"); remaining lines are step events. Sort all step lines by the ts_ms field.
2. Follow the **Record & Replay** section of the skill to analyse the trace and produce a SKILL.md that captures the recorded workflow as a replayable skill.
3. Output the completed SKILL.md content when done.`;
}
