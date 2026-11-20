"""Absorb an upstream mirror into infra/ according to infra/UPSTREAM.<name>.toml.

    python3 tools/upstream/absorb.py init layerfs [--force]
        Copy the mapped crates from the mirror branch at base_commit into
        infra/crates/, renaming crates and Rust paths. Run once; afterwards the
        code is ours and is changed in place.

    python3 tools/upstream/absorb.py diff layerfs <from-commit> <to-commit>
        Print the upstream change between two mirror commits, translated into
        our paths and crate names, ready for review and `git apply -3`.

按 infra/UPSTREAM.<name>.toml 把上游镜像吸收进 infra/。

    python3 tools/upstream/absorb.py init layerfs [--force]
        从镜像分支把映射的 crate 复制到 infra/crates/，重命名 crate 和 Rust 路径。
        只执行一次；之后代码归我们所有，原地修改。

    python3 tools/upstream/absorb.py diff layerfs <from-commit> <to-commit>
        输出两个镜像提交之间的上游变更，翻译成我们的路径和 crate 名，可直接评审并 `git apply -3`。
"""
from __future__ import annotations

import io
import os
import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import tomllib
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
TEXT_SUFFIXES = {".rs", ".toml", ".md", ".sql", ".json", ".txt", ".sh", ".py", ".yml", ".yaml"}


def load(name: str) -> dict:
    with (REPO / "infra" / f"UPSTREAM.{name}.toml").open("rb") as f:
        return tomllib.load(f)


def git(*args: str, stdin: bytes | None = None) -> bytes:
    return subprocess.run(["git", "-C", str(REPO), *args], input=stdin, check=True, capture_output=True).stdout


class Translator:
    def __init__(self, cfg: dict):
        self.crates = cfg["crate"]
        self.extra = [tuple(p) for p in cfg.get("transform", {}).get("replace", [])]
        # longest names first so layerfs_store_service wins over layerfs_store
        # 名字长的先替换，这样 layerfs_store_service 优先于 layerfs_store
        by_len = sorted(self.crates, key=lambda c: -len(c["upstream"]))
        self.ident = [(re.compile(rf"\b{c['upstream'].replace('-', '_')}\b"), c["package"].replace("-", "_")) for c in by_len]
        self.dirs = [(re.compile(rf"(?<![\w-]){re.escape(c['upstream'])}/"), c["dir"] + "/") for c in by_len]
        self.relpaths = [(re.compile(rf"(\.\./){re.escape(c['upstream'])}(?=[\"/])"), rf"\g<1>{c['dir']}") for c in by_len]
        self.pkgs = [(re.compile(rf"(?<![\w-]){re.escape(c['upstream'])}(?![\w-])"), c["package"]) for c in by_len]

    def rust(self, text: str) -> str:
        for a, b in self.extra:
            text = text.replace(a, b)
        for rx, new in self.ident:
            text = rx.sub(new, text)
        for rx, new in self.dirs:  # #[path = "../../layerfs-api/tests/..."]
            text = rx.sub(new, text)
        return text

    def manifest(self, text: str, edition: str, rust_version: str) -> str:
        text = self.rust(text)
        for rx, new in self.relpaths:  # path = "../layerfs-content"
            text = rx.sub(new, text)
        for rx, new in self.pkgs:  # package names and dependency keys / 包名和依赖键
            text = rx.sub(new, text)
        text = text.replace("edition.workspace = true", f'edition = "{edition}"')
        text = text.replace("rust-version.workspace = true", f'rust-version = "{rust_version}"')
        return text

    def path(self, upstream_path: str) -> str | None:
        for c in self.crates:
            prefix = f"core/crates/{c['upstream']}/"
            if upstream_path.startswith(prefix):
                rest = upstream_path[len(prefix):]
                for old, new in c.get("renames", []):
                    if rest == old:
                        rest = new
                return f"infra/crates/{c['dir']}/{rest}"
        return None


def extract(branch_or_commit: str, paths: list[str], into: Path) -> None:
    data = git("archive", "--format=tar", branch_or_commit, *paths)
    with tarfile.open(fileobj=io.BytesIO(data)) as tar:
        tar.extractall(into, filter="data")


def init(name: str, force: bool) -> int:
    cfg = load(name)
    up = cfg["upstream"]
    tr = Translator(cfg)
    edition = cfg["transform"]["edition"]
    rust_version = cfg["transform"]["rust_version"]
    sources = [f"core/crates/{c['upstream']}" for c in cfg["crate"]] + [c["upstream"] for c in cfg.get("copy", [])]
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        extract(up["mirror_commit"], sources, tmp)
        for c in cfg["crate"]:
            src = tmp / "core" / "crates" / c["upstream"]
            dst = REPO / "infra" / "crates" / c["dir"]
            if dst.exists():
                if not force:
                    print(f"{dst.relative_to(REPO)} exists; absorbed code is ours now (use --force to overwrite)", file=sys.stderr)
                    return 1
                shutil.rmtree(dst)
            for f in sorted(p for p in src.rglob("*") if p.is_file()):
                rel = f.relative_to(tmp).as_posix()
                out = REPO / tr.path(rel)
                out.parent.mkdir(parents=True, exist_ok=True)
                raw = f.read_bytes()
                if f.suffix in TEXT_SUFFIXES:
                    try:
                        text = raw.decode("utf-8")
                    except UnicodeDecodeError:
                        out.write_bytes(raw)
                        continue
                    text = tr.manifest(text, edition, rust_version) if f.name == "Cargo.toml" else tr.rust(text)
                    out.write_text(text, encoding="utf-8")
                else:
                    out.write_bytes(raw)
                shutil.copymode(f, out)
            print(f"absorbed {c['upstream']:24} -> infra/crates/{c['dir']} ({c['package']})")
        for c in cfg.get("copy", []):
            src = tmp / c["upstream"]
            dst = REPO / c["dest"]
            if dst.exists():
                shutil.rmtree(dst) if dst.is_dir() else dst.unlink()
            dst.parent.mkdir(parents=True, exist_ok=True)
            shutil.copytree(src, dst) if src.is_dir() else shutil.copy2(src, dst)
            print(f"copied   {c['upstream']:24} -> {c['dest']}")
    return 0


def diff(name: str, a: str, b: str) -> int:
    cfg = load(name)
    tr = Translator(cfg)
    paths = [f"core/crates/{c['upstream']}" for c in cfg["crate"]] + [c["upstream"] for c in cfg.get("copy", [])]
    patch = git("diff", "--no-color", "--no-renames", f"{a}..{b}", "--", *paths).decode("utf-8", "replace")
    out = []
    for line in patch.splitlines(keepends=True):
        if line.startswith(("diff --git ", "--- a/", "+++ b/")):
            def sub(m: re.Match) -> str:
                mapped = tr.path(m.group(2))
                return m.group(1) + (mapped if mapped else m.group(2))
            line = re.sub(r"([ab]/)(core/crates/\S+)", sub, line)
        elif line[:1] in "+- " and not line.startswith(("+++", "---")):
            line = line[0] + tr.rust(line[1:])
        out.append(line)
    sys.stdout.write("".join(out))
    return 0


def main(argv: list[str]) -> int:
    if len(argv) >= 2 and argv[0] == "init":
        return init(argv[1], "--force" in argv)
    if len(argv) == 4 and argv[0] == "diff":
        return diff(argv[1], argv[2], argv[3])
    print(__doc__, file=sys.stderr)
    return 2


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
