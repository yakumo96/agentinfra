"""Check the absorbed crates' manifests without building them.

Temporarily lists every absorbed crate as a workspace member (plus the fuser
patch they need), runs `cargo metadata --no-deps --offline`, and restores the
root Cargo.toml. Nothing is compiled and no build script runs.
"""
from __future__ import annotations

import json
import re
import subprocess
import sys
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]


def main() -> int:
    with (REPO / "infra" / "UPSTREAM.layerfs.toml").open("rb") as f:
        cfg = tomllib.load(f)
    crates = [f"infra/crates/{c['dir']}" for c in cfg["crate"]]
    root = REPO / "Cargo.toml"
    original = root.read_text()
    patched = original.replace("members = [\n", "members = [\n" + "".join(f'    "{c}",\n' for c in crates), 1)
    patched = re.sub(r"exclude = \[.*?\]\n", 'exclude = ["infra/vendor/fuser-0.18.0"]\n', patched, count=1, flags=re.S)
    patched += '\n[patch.crates-io]\nfuser = { version = "=0.18.0", path = "infra/vendor/fuser-0.18.0" }\n'
    try:
        root.write_text(patched)
        res = subprocess.run(["cargo", "metadata", "--no-deps", "--offline", "--format-version", "1"],
                             cwd=REPO, capture_output=True, text=True)
    finally:
        root.write_text(original)
    if res.returncode != 0:
        print(res.stderr, file=sys.stderr)
        return 1
    pkgs = {p["name"]: p for p in json.loads(res.stdout)["packages"]}
    problems = []
    for c in cfg["crate"]:
        p = pkgs.get(c["package"])
        if p is None:
            problems.append(f"{c['package']}: missing from metadata")
        elif p["edition"] != cfg["transform"]["edition"]:
            problems.append(f"{c['package']}: edition {p['edition']}")
        else:
            for d in p["dependencies"]:
                if d["name"].startswith("layerfs-"):
                    problems.append(f"{c['package']}: still depends on {d['name']}")
    for line in problems:
        print(line, file=sys.stderr)
    if not problems:
        print(f"absorbed manifests ok: {len(cfg['crate'])} crates")
    return 1 if problems else 0


if __name__ == "__main__":
    sys.exit(main())
