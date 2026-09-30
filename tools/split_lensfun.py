#!/usr/bin/env python3
"""Split lensfun.json into cameras.json plus one lens file per mount.

    python tools/split_lensfun.py            write web/src/lib/wasm/lenses/
    python tools/split_lensfun.py --check    fail if they're out of date
"""
from __future__ import annotations

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / "web/src/lib/wasm/lensfun.json"
OUT = ROOT / "web/src/lib/wasm/lenses"


def slug(mount: str) -> str:
    return re.sub(r"[^a-z0-9]+", "-", mount.lower()).strip("-") or "unnamed"


def pieces() -> dict[str, str]:
    db = json.loads(SOURCE.read_text())
    by_mount: dict[str, list] = {}
    for lens in db["lenses"]:
        by_mount.setdefault(lens["mount"].lower(), []).append(lens)
    files: dict[str, str] = {}
    chunks: dict[str, str] = {}
    for mount, lenses in by_mount.items():
        name = f"lens-{slug(mount)}"
        if name in chunks.values():
            raise SystemExit(f"two mounts share the file name {name}")
        chunks[mount] = name
        files[f"{name}.json"] = json.dumps({"mounts": [mount], "lenses": lenses},
                                           separators=(",", ":"))
    files["cameras.json"] = json.dumps({"cameras": db["cameras"], "mounts": [],
                                        "chunks": chunks}, separators=(",", ":"))
    return files


def main() -> None:
    files = pieces()
    if "--check" in sys.argv:
        present = {p.name for p in OUT.glob("*.json")} if OUT.exists() else set()
        stale = [n for n, text in files.items()
                 if not (OUT / n).exists() or (OUT / n).read_text() != text]
        extra = present - files.keys()
        if stale or extra:
            raise SystemExit(f"web/src/lib/wasm/lenses is out of date with lensfun.json "
                             f"({len(stale)} stale, {len(extra)} extra): "
                             f"run python tools/split_lensfun.py")
        print(f"lenses: {len(files)} files, up to date")
        return
    OUT.mkdir(exist_ok=True)
    for old in OUT.glob("*.json"):
        if old.name not in files:
            old.unlink()
    for name, text in files.items():
        (OUT / name).write_text(text)
    print(f"wrote {len(files)} files to {OUT.relative_to(ROOT)}")


if __name__ == "__main__":
    main()
