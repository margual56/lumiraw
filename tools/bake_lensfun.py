#!/usr/bin/env python3
"""Convert the lensfun XML database to the JSON the pipeline reads.

    python tools/bake_lensfun.py /usr/share/lensfun/version_1 web/src/lib/wasm/lensfun.json
"""
from __future__ import annotations

import json
import sys
import xml.etree.ElementTree as ET
from pathlib import Path


def text(node, tag, default=""):
    for child in node.findall(tag):
        # the untranslated entry has no xml:lang
        if not child.attrib.get("{http://www.w3.org/XML/1998/namespace}lang"):
            return (child.text or "").strip()
    child = node.find(tag)
    return (child.text or "").strip() if child is not None else default


def fnum(node, key, default=None):
    v = node.attrib.get(key)
    if v is None:
        return default
    try:
        return round(float(v), 6)
    except ValueError:
        return default


def main(src: Path, dst: Path) -> None:
    cameras, lenses = [], []
    for path in sorted(src.glob("*.xml")):
        root = ET.parse(path).getroot()
        for cam in root.findall("camera"):
            cameras.append({
                "maker": text(cam, "maker"),
                "model": text(cam, "model"),
                "mount": text(cam, "mount"),
                "crop": fnum_text(cam, "cropfactor", 1.0),
            })
        for lens in root.findall("lens"):
            calib = lens.find("calibration")
            if calib is None:
                continue
            entry = {
                "maker": text(lens, "maker"),
                "model": text(lens, "model"),
                "mount": text(lens, "mount"),
                "crop": fnum_text(lens, "cropfactor", 1.0),
                "dist": [], "tca": [], "vign": [],
            }
            for d in calib.findall("distortion"):
                entry["dist"].append({
                    "f": fnum(d, "focal", 0.0), "m": d.attrib.get("model", ""),
                    "a": fnum(d, "a", 0.0), "b": fnum(d, "b", 0.0), "c": fnum(d, "c", 0.0),
                    "k1": fnum(d, "k1", 0.0), "k2": fnum(d, "k2", 0.0),
                })
            for t in calib.findall("tca"):
                entry["tca"].append({
                    "f": fnum(t, "focal", 0.0), "m": t.attrib.get("model", ""),
                    "br": fnum(t, "br", 0.0), "cr": fnum(t, "cr", 0.0), "vr": fnum(t, "vr", 1.0),
                    "bb": fnum(t, "bb", 0.0), "cb": fnum(t, "cb", 0.0), "vb": fnum(t, "vb", 1.0),
                    "kr": fnum(t, "kr", 1.0), "kb": fnum(t, "kb", 1.0),
                })
            for v in calib.findall("vignetting"):
                if v.attrib.get("model") != "pa":
                    continue
                entry["vign"].append({
                    "f": fnum(v, "focal", 0.0), "ap": fnum(v, "aperture", 0.0),
                    "d": fnum(v, "distance", 1000.0),
                    "k1": fnum(v, "k1", 0.0), "k2": fnum(v, "k2", 0.0), "k3": fnum(v, "k3", 0.0),
                })
            if entry["dist"] or entry["tca"] or entry["vign"]:
                lenses.append(entry)

    dst.parent.mkdir(parents=True, exist_ok=True)
    dst.write_text(json.dumps({"cameras": cameras, "lenses": lenses}, separators=(",", ":")))
    print(f"{len(cameras)} cameras, {len(lenses)} calibrated lenses -> {dst} "
          f"({dst.stat().st_size / 1e6:.2f} MB)")


def fnum_text(node, tag, default):
    child = node.find(tag)
    try:
        return round(float((child.text or "").strip()), 4)
    except (AttributeError, ValueError):
        return default


if __name__ == "__main__":
    src = Path(sys.argv[1] if len(sys.argv) > 1 else "/usr/share/lensfun/version_1")
    dst = Path(sys.argv[2] if len(sys.argv) > 2 else "web/src/lib/wasm/lensfun.json")
    main(src, dst)
