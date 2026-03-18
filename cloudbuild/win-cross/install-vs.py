#!/usr/bin/env python3
# Standalone VS2022 installer for Docker image pre-installation.
# Adapted from engine/taskcluster/scripts/misc/get_vs.py without
# the dependency on Mozilla's buildconfig module.

import argparse
import os
import shutil
import ssl
from pathlib import Path
from tempfile import TemporaryDirectory

import certifi
import yaml
from urllib import request
from vsdownload import downloadPackages, extractPackages

# Hook certifi for SSL
_urlopen = request.urlopen


def urlopen(url, data=None):
    return _urlopen(
        url, data, context=ssl.create_default_context(cafile=certifi.where())
    )


request.urlopen = urlopen


if __name__ == "__main__":
    parser = argparse.ArgumentParser(
        description="Download and install VS2022 cross-compile toolchain"
    )
    parser.add_argument("manifest", help="Path to vs2022.yaml manifest")
    parser.add_argument("outdir", help="Output directory")
    args = parser.parse_args()

    out_dir = Path(args.outdir)
    with open(args.manifest) as f:
        selected = yaml.safe_load(f.read())
    with TemporaryDirectory(prefix="get_vs", dir=".") as tmpdir:
        tmpdir = Path(tmpdir)
        dl_cache = tmpdir / "cache"
        downloadPackages(selected, dl_cache)
        unpacked = tmpdir / "unpack"
        extractPackages(selected, dl_cache, unpacked)
        vfs = {}
        for subpath in ("VC", "Windows Kits/10", "DIA SDK"):
            dest = subpath
            program_files_subpath = unpacked / "Program Files" / subpath
            if program_files_subpath.exists():
                subpath = program_files_subpath
            else:
                subpath = unpacked / subpath
            dest = Path(dest)
            for root, dirs, files in os.walk(subpath):
                relpath = Path(root).relative_to(subpath)
                for f in files:
                    path = Path(root) / f
                    mode = os.stat(path).st_mode
                    with open(path, "rb") as fh:
                        lower_f = f.lower()
                        if lower_f.endswith(".lib"):
                            f = lower_f
                        name = str(dest / relpath / f)
                        if lower_f.endswith(".exe"):
                            mode |= (mode & 0o444) >> 2
                        print("Adding", name)
                        out_file = out_dir / name
                        out_file.parent.mkdir(parents=True, exist_ok=True)
                        with out_file.open("wb") as out_fh:
                            shutil.copyfileobj(fh, out_fh)
                        os.chmod(out_file, mode)
                        if lower_f.endswith((".h", ".idl")):
                            vfs.setdefault(str(dest / relpath), []).append(f)
        overlay = {
            "version": 0,
            "case-sensitive": False,
            "root-relative": "overlay-dir",
            "overlay-relative": True,
            "roots": [
                {
                    "name": p,
                    "type": "directory",
                    "contents": [
                        {
                            "name": f,
                            "type": "file",
                            "external-contents": f"{p}/{f}",
                        }
                        for f in files
                    ],
                }
                for p, files in vfs.items()
            ],
        }
        overlay_yaml = out_dir / "overlay.yaml"
        with overlay_yaml.open("w") as fh:
            fh.write(yaml.dump(overlay))
    print("VS2022 installation complete at", out_dir)
