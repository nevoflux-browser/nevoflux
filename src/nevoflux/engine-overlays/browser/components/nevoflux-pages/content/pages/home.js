/* This Source Code Form is subject to the terms of the Mozilla Public
 * License, v. 2.0. If a copy of the MPL was not distributed with this
 * file, You can obtain one at http://mozilla.org/MPL/2.0/. */

"use strict";

/**
 * Home page controller -- the NevoFlux launcher.
 *
 * Provides a search input that sends messages to the Sidebar agent,
 * and displays recent conversation history.
 */
const Home = {
  init() {
    this._setupInput();
    this._loadRecentSessions();
  },

  _setupInput() {
    const input = document.getElementById("home-input");
    input.addEventListener("keydown", (e) => {
      if (e.key === "Enter" && input.value.trim()) {
        this._sendToSidebar(input.value.trim());
        input.value = "";
      }
    });
  },

  async _sendToSidebar(message) {
    try {
      await NevofluxPage.sendQuery("sidebar:sendMessage", { message });
    } catch (e) {
      console.error("Failed to send to sidebar:", e);
      // Fallback: try opening sidebar via the browser
      try {
        await NevofluxPage.sendQuery("sidebar:open", {});
        // Retry after brief delay
        setTimeout(async () => {
          try {
            await NevofluxPage.sendQuery("sidebar:sendMessage", { message });
          } catch (e2) {
            console.error("Retry failed:", e2);
          }
        }, 500);
      } catch (e3) {
        console.error("Failed to open sidebar:", e3);
      }
    }
  },

  async _loadRecentSessions() {
    const list = document.getElementById("session-list");

    try {
      const result = await NevofluxPage.sendQuery("contentStore:query", {
        prefix: "session:",
      });

      if (!result?.results?.length) {
        this._showEmptySessions();
        return;
      }

      // Sort by updatedAt descending, take top 10
      const sessions = result.results
        .map(r => ({ id: r.key.replace("session:", ""), ...r.value }))
        .sort((a, b) => (b.updatedAt || 0) - (a.updatedAt || 0))
        .slice(0, 10);

      if (sessions.length === 0) {
        this._showEmptySessions();
        return;
      }

      list.innerHTML = "";
      for (const session of sessions) {
        list.appendChild(this._createSessionItem(session));
      }
    } catch (e) {
      console.error("Failed to load recent sessions:", e);
      this._showEmptySessions();
    }
  },

  _createSessionItem(session) {
    const li = document.createElement("li");

    const item = document.createElement("a");
    item.className = "session-item";
    item.href = "#";
    item.addEventListener("click", (e) => {
      e.preventDefault();
      this._restoreSession(session.id);
    });

    const icon = document.createElement("span");
    icon.className = "icon";
    icon.textContent = "\uD83D\uDCAC"; // speech balloon

    const title = document.createElement("span");
    title.className = "title";
    title.textContent = session.title || "Untitled conversation";

    const time = document.createElement("span");
    time.className = "time";
    time.textContent = this._formatRelativeTime(session.updatedAt);

    item.appendChild(icon);
    item.appendChild(title);
    item.appendChild(time);
    li.appendChild(item);
    return li;
  },

  async _restoreSession(sessionId) {
    try {
      await NevofluxPage.sendQuery("sidebar:restoreSession", { sessionId });
    } catch (e) {
      console.error("Failed to restore session:", e);
    }
  },

  _showEmptySessions() {
    const list = document.getElementById("session-list");
    list.innerHTML = "";
    const empty = document.createElement("li");
    empty.className = "empty-sessions";
    empty.textContent = "No recent conversations yet. Start typing above!";
    list.appendChild(empty);
  },

  _formatRelativeTime(timestamp) {
    if (!timestamp) return "";

    const now = Date.now();
    const diff = now - timestamp;
    const seconds = Math.floor(diff / 1000);
    const minutes = Math.floor(seconds / 60);
    const hours = Math.floor(minutes / 60);
    const days = Math.floor(hours / 24);

    if (seconds < 60) return "just now";
    if (minutes < 60) return `${minutes}m ago`;
    if (hours < 24) return `${hours}h ago`;
    if (days === 1) return "yesterday";
    if (days < 7) return `${days}d ago`;

    const date = new Date(timestamp);
    return date.toLocaleDateString();
  },
};

document.addEventListener("DOMContentLoaded", () => Home.init());
