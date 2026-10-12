/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

/**
 * Which `ui:notification:agent` events become an OS notification, and with
 * what text. Pure so Node can test it; background.js handleUserNotification
 * supplies whether a window is focused and whether the sidebar is open there.
 *
 * The in-app toast lives in the sidebar (EventBusListener), so the OS
 * notification is the fallback for when the person cannot see it:
 *  - `notify_user` reminders: only when no browser window is focused
 *    (unchanged behaviour);
 *  - `agent_approval` outcomes (M2b §5): also when the focused window's
 *    sidebar is closed, because an approval's outcome must reach the desktop
 *    exactly once.
 */

const OS_NOTIFY_SOURCES = new Set(['notify_user', 'agent_approval']);
const DEFAULT_TITLE = 'NevoFlux';

/** Whether an event from `source` may raise an OS notification now. */
export function mayShowOsNotification(source, windowFocused, sidebarOpen) {
  if (source === 'notify_user') {
    return !windowFocused;
  }
  if (source === 'agent_approval') {
    return !windowFocused || !sidebarOpen;
  }
  return false;
}

/**
 * The OS notification's title and message, or null when this event never
 * becomes one. `body` is the canonical text (the sidebar toast shows it too);
 * `message` is tolerated as a fallback. An approval outcome's title already
 * says what happened, so one without a body is shown by its title alone.
 */
export function osNotificationContent(data) {
  if (!OS_NOTIFY_SOURCES.has(data?.source)) {
    return null;
  }
  const raw = typeof data.body === 'string' ? data.body : data.message;
  const body = typeof raw === 'string' ? raw.trim() : '';
  const title = typeof data.title === 'string' && data.title.trim() ? data.title.trim() : DEFAULT_TITLE;
  if (body) {
    return { title, message: body };
  }
  if (data.source === 'agent_approval' && title !== DEFAULT_TITLE) {
    return { title, message: title };
  }
  return null;
}
