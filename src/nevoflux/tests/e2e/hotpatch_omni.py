#!/usr/bin/env python3
# This Source Code Form is subject to the terms of the Mozilla Public
# License, v. 2.0. If a copy of the MPL was not distributed with this
# file, You can obtain one at http://mozilla.org/MPL/2.0/.
"""Make a smoke copy of an installed NevoFlux with this tree's NevoFlux files
swapped into browser/omni.ja (stdlib only).

Usage:
  python hotpatch_omni.py --installed "C:/Program Files/NevoFlux Browser" \
      --dist engine/obj-x86_64-pc-windows-msvc/dist/bin --out <new dir>

Why: the dev flat build cannot open nevoflux:// pages (its pages dir is
NTFS symlinks), so runtime checks run on an installed build. The installed
copy is never touched; --out must not exist. omni.ja is an optimizejars jar
whose central directory Python's zipfile cannot read, so `unzip` extracts
it and zipfile repacks it ZIP_STORED, like the original. unzip exits 1
(warnings) or 2 ("central directory ... too long ... Compensating") on such
a jar while extracting every entry, so exit 2 is accepted only when the
extracted file count equals `unzip -Z1`'s file listing. Entries are replaced for: the built-in extension tree, the
nevoflux:// pages tree, ext-nevoflux.js, the nevoflux schema, and
actors/modules named Nevoflux*. Prints every added and removed entry name.
"""
import argparse
import fnmatch
import os
import shutil
import subprocess
import sys
import tempfile
import zipfile

# Paths inside browser/omni.ja == paths under <dist>/browser/.
TREES = [
    "chrome/browser/builtin-addons/nevoflux-agent",
    "chrome/nevoflux",
]
FILES = [
    "chrome/browser/content/browser/parent/ext-nevoflux.js",
    "chrome/browser/content/browser/schemas/nevoflux.json",
]
GLOBS = [("actors", "Nevoflux*.sys.mjs"), ("modules", "Nevoflux*.sys.mjs")]


def entries(root):
    out = set()
    for d, _, files in os.walk(root):
        for f in files:
            out.add(os.path.relpath(os.path.join(d, f), root).replace(os.sep, "/"))
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--installed", required=True)
    ap.add_argument("--dist", required=True, help="engine/obj-*/dist/bin")
    ap.add_argument("--out", required=True)
    args = ap.parse_args()
    if os.path.exists(args.out):
        sys.exit("refusing to overwrite existing " + args.out)

    shutil.copytree(args.installed, args.out)
    omni = os.path.join(args.out, "browser", "omni.ja")
    work = tempfile.mkdtemp(prefix="omni-")
    r = subprocess.run(["unzip", "-q", "-o", omni, "-d", work])
    if r.returncode > 2:
        sys.exit("unzip failed with exit code %d" % r.returncode)
    before = entries(work)
    if r.returncode == 2:
        listing = subprocess.run(["unzip", "-Z1", omni], capture_output=True, text=True)
        listed = {n for n in listing.stdout.splitlines() if n and not n.endswith("/")}
        if listed != before:
            sys.exit("unzip exit 2 and %d of %d entries extracted" % (len(before), len(listed)))
    src = os.path.join(args.dist, "browser")
    replaced = []

    def put(rel):
        dst = os.path.join(work, rel)
        os.makedirs(os.path.dirname(dst), exist_ok=True)
        shutil.copyfile(os.path.join(src, rel), dst)  # follows symlinks
        replaced.append(rel)

    for tree in TREES:
        # Replace the whole tree so files deleted on this branch do not linger.
        shutil.rmtree(os.path.join(work, tree), ignore_errors=True)
        for d, _, files in os.walk(os.path.join(src, tree), followlinks=True):
            for f in files:
                put(os.path.relpath(os.path.join(d, f), src).replace(os.sep, "/"))
    for rel in FILES:
        put(rel)
    for folder, pattern in GLOBS:
        for f in sorted(os.listdir(os.path.join(src, folder))):
            if fnmatch.fnmatch(f, pattern):
                put(folder + "/" + f)

    after = entries(work)
    tmp = omni + ".new"
    with zipfile.ZipFile(tmp, "w", zipfile.ZIP_STORED) as z:
        for rel in sorted(after):
            z.write(os.path.join(work, rel), rel)
    os.replace(tmp, omni)
    shutil.rmtree(work, ignore_errors=True)

    print("replaced %d entries" % len(replaced))
    for rel in sorted(after - before):
        print("  added:   " + rel)
    for rel in sorted(before - after):
        print("  removed: " + rel)
    print("smoke copy ready: " + args.out)


if __name__ == "__main__":
    sys.exit(main())
