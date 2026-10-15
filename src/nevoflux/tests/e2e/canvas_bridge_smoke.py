#!/usr/bin/env python3
"""Malicious-canvas smoke test for the canvas bridge gates (stdlib only).

Usage: python canvas_bridge_smoke.py --exe <nevoflux.exe> [--port 2841]

Launches an isolated browser on a temporary profile, drives it over Marionette
(raw TCP, `<len>:<json>` frames), loads fixtures/canvas-bridge-attack.html as
canvas "smoke", waits, then prints each result next to its expected outcome.
Nothing is asserted; a human compares the columns.
"""
import argparse
import json
import os
import shutil
import socket
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
FIXTURE = os.path.join(HERE, "fixtures", "canvas-bridge-attack.html")

# Expected outcome per fixture attempt (human-checked, never asserted).
EXPECTED = [
    ("parent.document", "refused"),
    ("parent.NevofluxBridge", "refused"),
    ("ChromeUtils", "refused (undefined)"),
    ("file fetch", "refused"),
    ("set config tier", "ok (value lands in app:smoke:config:..., real config untouched)"),
    ("query everything", "ok, only this canvas's keys; no config:/canvas:; no legacy flat keys"),
    ("sendCommand remote.agent_set_tier", "refused FORBIDDEN_COMMAND"),
    ("sendCommand account.status", "refused FORBIDDEN_COMMAND"),
    ("sendCommand status", "refused FORBIDDEN_COMMAND"),
    ("sendCommand schedule.run_now", "refused COMMAND_NOT_AVAILABLE"),
    ("sendCommand artifact.get other", "refused NOT_OWN_ARTIFACT"),
    ("chat foreign session", "refused NOT_OWN_SESSION"),
    ("cancel foreign session", "refused NOT_OWN_SESSION"),
    ("sidebar.notify raw", "ok (OS notification 'Canvas: smoke - session_title'; sidebar title unchanged)"),
    ("notify flood 1", "ok"),
    ("notify flood 2", "ok"),
    ("notify flood 3", "refused RATE_LIMITED (4th call in 30 s)"),
    ("publish ui:soul:active", "refused"),
    ("get legacy notes:old", 'ok ["legacy-old"] (migrated)'),
    ("delete notes:old", "ok"),
    ("delete then get legacy", "ok null (tombstone, not resurrected)"),
    ("sendCommand daemon.info", "ok with daemon info (needs daemon online; else POLICY_UNAVAILABLE)"),
    ("storage roundtrip", "ok [1]"),
]


def recv_frame(sock):
    buf = b""
    while not buf.endswith(b":"):
        c = sock.recv(1)
        if not c:
            raise ConnectionError("marionette closed")
        buf += c
    n = int(buf[:-1])
    data = b""
    while len(data) < n:
        chunk = sock.recv(n - len(data))
        if not chunk:
            raise ConnectionError("marionette closed")
        data += chunk
    return json.loads(data.decode("utf-8"))


class Marionette:
    def __init__(self, port, timeout):
        self.sock = None
        self.next_id = 0
        deadline = time.time() + timeout
        last = None
        while time.time() < deadline:
            try:
                s = socket.create_connection(("127.0.0.1", port), timeout=5)
                s.settimeout(120)
                recv_frame(s)  # hello
                self.sock = s
                return
            except (OSError, ValueError) as e:
                last = e
                time.sleep(1)
        raise RuntimeError("cannot connect to marionette: %r" % (last,))

    def call(self, name, params=None):
        self.next_id += 1
        cid = self.next_id
        msg = json.dumps([0, cid, name, params or {}])
        self.sock.sendall(("%d:%s" % (len(msg.encode("utf-8")), msg)).encode("utf-8"))
        while True:
            r = recv_frame(self.sock)
            if r[0] == 1 and r[1] == cid:
                if r[2]:
                    raise RuntimeError("%s failed: %s" % (name, r[2]))
                return r[3]


CHROME_SCRIPT = """
const done = arguments[arguments.length - 1];
(async () => {
  const { NevofluxContentStore: S } = ChromeUtils.importESModule(
    "resource:///modules/NevofluxContentStore.sys.mjs");
  S.set("notes:items", ["legacy"]);
  S.set("notes:old", ["legacy-old"]);
  S.set("canvas:smoke", { id: "smoke", type: "html", title: "smoke",
    content: __FIXTURE__, state: "complete" });
  const win = Services.wm.getMostRecentWindow("navigator:browser");
  const tab = win.gBrowser.addTab("nevoflux://canvas/smoke",
    { triggeringPrincipal: Services.scriptSecurityManager.getSystemPrincipal() });
  win.gBrowser.selectedTab = tab;
  await new Promise(r => setTimeout(r, 15000));
  done({
    results: S.get("app:smoke:smoke:results"),
    config_tier: S.get("config:settings.general.agentExecution"),
    migrated_notes_items: S.get("app:smoke:notes:items"),
    tombstone_notes_old: S.get("app:smoke:notes:old"),
    legacy_notes_old: S.get("notes:old"),
  });
})().catch(e => done({ error: String(e) }));
"""


def kill_profile_tree(profile):
    """Kill only processes whose command line contains our temp profile path."""
    if os.name == "nt":
        needle = profile.replace("'", "''")
        ps = ("Get-CimInstance Win32_Process | Where-Object { $_.CommandLine -and "
              "$_.CommandLine.Contains('%s') } | ForEach-Object { "
              "taskkill /F /T /PID $_.ProcessId | Out-Null }" % needle)
        subprocess.run(["powershell", "-NoProfile", "-Command", ps],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    else:
        subprocess.run(["pkill", "-9", "-f", profile],
                       stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exe", required=True, help="path to nevoflux.exe")
    ap.add_argument("--port", type=int, default=2841, help="marionette port")
    args = ap.parse_args()

    with open(FIXTURE, encoding="utf-8") as f:
        fixture_html = f.read()

    profile = tempfile.mkdtemp(prefix="nevoflux-canvas-smoke-")
    with open(os.path.join(profile, "user.js"), "w", encoding="utf-8") as f:
        f.write('user_pref("marionette.port", %d);\n' % args.port)
        f.write('user_pref("browser.shell.checkDefaultBrowser", false);\n')
    proc = None
    try:
        proc = subprocess.Popen(
            [args.exe, "-no-remote", "-profile", profile, "-marionette",
             "--remote-allow-system-access"],
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        m = Marionette(args.port, 90)
        m.call("WebDriver:NewSession", {"capabilities": {}})
        m.call("Marionette:SetContext", {"value": "chrome"})
        script = CHROME_SCRIPT.replace("__FIXTURE__", json.dumps(fixture_html))
        out = m.call("WebDriver:ExecuteAsyncScript", {
            "script": script, "args": [], "scriptTimeout": 60000})
        val = out.get("value", out) if isinstance(out, dict) else out
        if not isinstance(val, dict):
            val = {}
        print("== raw ==")
        print(json.dumps(val, indent=1, ensure_ascii=False))
        print("\n== results vs expected (human check, not asserted) ==")
        res = val.get("results")
        res = res if isinstance(res, dict) else {}
        for name, exp in EXPECTED:
            print("%-36s | %s" % (name, res.get(name, "<missing>")))
            print("%-36s |   expected: %s" % ("", exp))
        print("\nconfig:settings.general.agentExecution (expect null/original):",
              val.get("config_tier"))
        print("app:smoke:notes:items (expect [\"legacy\"], migrated):",
              val.get("migrated_notes_items"))
        print("app:smoke:notes:old (expect tombstone object):",
              val.get("tombstone_notes_old"))
        print("notes:old (expect legacy left in place):",
              val.get("legacy_notes_old"))
    finally:
        if proc is not None:
            try:
                proc.kill()
            except OSError:
                pass
        kill_profile_tree(profile)
        time.sleep(2)
        shutil.rmtree(profile, ignore_errors=True)


if __name__ == "__main__":
    sys.exit(main())
