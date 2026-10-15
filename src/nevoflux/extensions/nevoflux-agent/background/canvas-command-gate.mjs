/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Read the daemon's answer to `canvas.policy_check { kind: "command" }`.
 *
 * Only an answer that echoes `kind: "command"` counts. A daemon that predates
 * command checks answers the request as a browser-action check, which can
 * allow it; treating that as a refusal keeps an older daemon from waving
 * every canvas command through.
 */
export function commandVerdict(res) {
  const data = res && res.success ? res.data : null;
  if (data && data.kind === 'command' && typeof data.allow === 'boolean') {
    return data;
  }
  return {
    allow: false,
    code: 'POLICY_UNAVAILABLE',
    message: 'the agent could not check whether this canvas may send that command',
  };
}
