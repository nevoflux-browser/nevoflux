#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.
"""Probe for M3 delegation (stdlib only): does a background Canvas tab open
without taking focus, and do background Canvas and web tabs capture non-blank?

Usage: python task_capture_probe.py --exe <smoke copy>/nevoflux.exe --out <dir> [--port 2843]
  e.g. --exe C:/Users/Docker/nevoflux/m3-smoke-browser/nevoflux.exe

--exe names the Task 14 hot-patched COPY of the installed build, never the
installed C:/Program Files/... build itself.

Runs an isolated browser on a temporary profile (the person's own browser is
untouched), with the built-in agent's NEVOFLUX_CONFIG / NEVOFLUX_DATA_DIR
pointed into that profile so it never reaches the person's own daemon, config
or data, and drives it over Marionette in chrome context. It opens a Canvas
tab and an https tab with addTab(..., {inBackground: true}) -- the call
browser.nevoflux.openCanvasTab(id, {inBackground: true}) makes -- waits, then
captures the Canvas preview frame with the parent's drawSnapshot (what
captureTaskScreenshot does) and the web tab with the child actor's screenshot.
Nothing is asserted: it prints whether the selected tab changed and writes
canvas.jpg and web.jpg for a person to look at. The dev flat build cannot open
nevoflux:// pages, hence the hot-patched copy.
"""
import argparse
import base64
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
from canvas_bridge_smoke import Marionette, isolated_agent_env, kill_profile_tree  # noqa: E402

PROBE_HTML = (
    "<!doctype html><html><body style='margin:0;background:#1e6fd9'>"
    "<h1 style='color:#fff;font:64px sans-serif;margin:40px'>PROBE 7</h1></body></html>"
)

CHROME_SCRIPT = """
const done = arguments[arguments.length - 1];
(async () => {
  const { NevofluxContentStore: S } = ChromeUtils.importESModule(
    "resource:///modules/NevofluxContentStore.sys.mjs");
  const sys = Services.scriptSecurityManager.getSystemPrincipal();
  const win = Services.wm.getMostRecentWindow("navigator:browser");
  const gB = win.gBrowser;
  const before = gB.selectedTab;
  S.set("canvas:probe", { id: "probe", type: "html", title: "probe",
    content: __HTML__, state: "complete" });
  const canvasTab = gB.addTab("nevoflux://canvas/probe",
    { triggeringPrincipal: sys, inBackground: true });
  const webTab = gB.addTab("https://example.com/",
    { triggeringPrincipal: sys, inBackground: true });
  await new Promise(r => setTimeout(r, 12000));
  const out = { stillSelected: gB.selectedTab === before,
                canvasSelected: gB.selectedTab === canvasTab };
  const frame = (canvasTab.linkedBrowser.browsingContext.children || [])[0];
  if (frame && frame.currentWindowGlobal) {
    const bmp = await frame.currentWindowGlobal.drawSnapshot(null, 1, "rgb(255,255,255)");
    const c = win.document.createElementNS("http://www.w3.org/1999/xhtml", "html:canvas");
    c.width = bmp.width; c.height = bmp.height;
    c.getContext("2d").drawImage(bmp, 0, 0);
    out.canvas = { w: bmp.width, h: bmp.height,
                   jpeg: c.toDataURL("image/jpeg", 0.6).split(",")[1] };
  } else {
    out.canvas = { error: "no preview frame" };
  }
  const wgp = webTab.linkedBrowser.browsingContext.currentWindowGlobal;
  const web = await wgp.getActor("Nevoflux").sendQuery("execute", {
    action: "screenshot",
    params: { fullPage: false, type: "jpeg", quality: 60, maxWidth: 1280 } });
  out.web = web && web.success
    ? { w: web.width, h: web.height, jpeg: web.data }
    : { error: JSON.stringify(web) };
  out.stillSelectedAfterCapture = gB.selectedTab === before;
  done(out);
})().catch(e => done({ error: String(e) }));
"""


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--exe", required=True, help="nevoflux.exe of the Task 14 smoke copy (never the installed build)")
    ap.add_argument("--out", required=True, help="directory for canvas.jpg and web.jpg")
    ap.add_argument("--port", type=int, default=2843, help="marionette port")
    args = ap.parse_args()
    os.makedirs(args.out, exist_ok=True)

    profile = tempfile.mkdtemp(prefix="nevoflux-capture-probe-")
    with open(os.path.join(profile, "user.js"), "w", encoding="utf-8") as f:
        f.write('user_pref("marionette.port", %d);\n' % args.port)
        f.write('user_pref("browser.shell.checkDefaultBrowser", false);\n')
        # A fresh profile otherwise shows Zen's first-run welcome screen, which
        # leaves the tab panels 0x0: background tabs never load or draw.
        f.write('user_pref("zen.welcome-screen.seen", true);\n')
    proc = None
    try:
        proc = subprocess.Popen(
            [args.exe, "-no-remote", "-profile", profile, "-marionette",
             "--remote-allow-system-access"],
            env=isolated_agent_env(profile),
            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        m = Marionette(args.port, 90)
        m.call("WebDriver:NewSession", {"capabilities": {}})
        m.call("Marionette:SetContext", {"value": "chrome"})
        script = CHROME_SCRIPT.replace("__HTML__", json.dumps(PROBE_HTML))
        out = m.call("WebDriver:ExecuteAsyncScript", {
            "script": script, "args": [], "scriptTimeout": 60000})
        val = out.get("value", out) if isinstance(out, dict) else out
        if not isinstance(val, dict):
            val = {}
        print("selected tab unchanged after background opens:", val.get("stillSelected"))
        print("background Canvas tab became selected (must be False):", val.get("canvasSelected"))
        print("selected tab unchanged after both captures:", val.get("stillSelectedAfterCapture"))
        for name in ("canvas", "web"):
            shot = val.get(name) or {}
            if "jpeg" in shot:
                path = os.path.join(args.out, name + ".jpg")
                data = base64.b64decode(shot["jpeg"])
                with open(path, "wb") as f:
                    f.write(data)
                print("%s: %sx%s, %d bytes -> %s" % (name, shot.get("w"), shot.get("h"), len(data), path))
            else:
                print("%s: FAILED %s" % (name, shot.get("error") or val.get("error")))
        print("Open both images: canvas.jpg must show a blue page reading PROBE 7;")
        print("web.jpg must show the example.com text. A plain white or black frame is blank.")
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
