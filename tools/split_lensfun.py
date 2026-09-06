#!/usr/bin/env python3
"""Split the baked lens database into the pieces the web app loads.

    python tools/split_lensfun.py            write web/src/lib/wasm/lenses/
    python tools/split_lensfun.py --check    fail if what is there is stale

The whole database is 2.7 MB (about 0.37 MB compressed), a fifth of a first
visit, and a photograph only ever needs one mount's lenses: a lens is looked
up among those of its camera's mount (`Database::find_lens`). So the page
loads the cameras alone, which also give the crop factor the noise and
diffraction estimates need, and fetches a mount's lenses when a photograph
says which it needs.

    cameras.json         every camera, "mounts": [] (no lenses yet), and
                         "chunks": which file holds each mount's lenses
    lens-<mount>.json    one mount's lenses, in the whole database's order,
                         with "mounts": [<mount>]

A camera whose mount has no lenses of its own is looked up against every
lens (that is `find_lens`'s fallback), so for those the page loads the whole
lensfun.json instead, which stays where it is for that and for the native
harnesses.
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
