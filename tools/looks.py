#!/usr/bin/env python3
"""Resample RawTherapee's film simulation HaldCLUTs to 33^3 tables for the looks.

    python tools/looks.py HALDCLUT_DIR

HALDCLUT_DIR is https://rawtherapee.com/shared/HaldCLUT.zip, unpacked.
"""
import re
import sys
from pathlib import Path

import numpy as np
from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "web/src/lib/wasm/looks"
SIZE = 33


def looks() -> dict[str, str]:
    source = (ROOT / "core/src/looks.rs").read_text()
    pairs = re.findall(r'id: "([^"]+)".*?from: "([^"]*)"', source, re.S)
    return {look: path for look, path in pairs if path}


def hald(path: Path) -> np.ndarray:
    """The table as [blue][green][red] -> rgb, 0..1."""
    img = np.asarray(Image.open(path).convert("RGB"), dtype=np.float32) / 255
    n = round(img.shape[0] ** (2 / 3))
    return img.reshape(n, n, n, 3)


def sample(cube: np.ndarray, rgb: np.ndarray) -> np.ndarray:
    n = cube.shape[0]
    x = rgb * (n - 1)
    i = np.floor(x).astype(int).clip(0, n - 2)
    f = x - i
    r, g, b = i[..., 0], i[..., 1], i[..., 2]
    out = 0
    for db in (0, 1):
        for dg in (0, 1):
            for dr in (0, 1):
                w = ((f[..., 0:1] if dr else 1 - f[..., 0:1])
                     * (f[..., 1:2] if dg else 1 - f[..., 1:2])
                     * (f[..., 2:3] if db else 1 - f[..., 2:3]))
                out = out + w * cube[b + db, g + dg, r + dr]
    return out


def main(collection: Path) -> None:
    OUT.mkdir(parents=True, exist_ok=True)
    axis = np.linspace(0, 1, SIZE, dtype=np.float32)
    b, g, r = np.meshgrid(axis, axis, axis, indexing="ij")
    grid = np.stack([r, g, b], -1).reshape(-1, 3)      # red changing fastest
    for look, path in looks().items():
        table = sample(hald(collection / path), grid).clip(0, 1)
        out = OUT / f"look-{look}.bin"
        out.write_bytes((table * 255 + 0.5).astype(np.uint8).tobytes())
        print(f"{out.relative_to(ROOT)}  {out.stat().st_size // 1024} KB  <- {path}")


if __name__ == "__main__":
    main(Path(sys.argv[1]))
