#!/usr/bin/env python3
"""Second-parser oracle for editor-psd, built on psd-tools.

Dumps what psd-tools sees in a PSD/PSB as JSON: header, the layer tree
(names, kinds, blend modes, opacity, visibility, clipping, bounds, masks,
layer ids) and a coarse fingerprint of the file's own merged image (the
per-channel mean and a 16x16 area-averaged grid of colour over white plus
alpha, 0..255).

Usage:
  oracle.py FILE                 print JSON for one file
  oracle.py --corpus DIR OUTDIR  write OUTDIR/<relative path>.json for every
                                 .psd/.psb under DIR (skips files whose JSON
                                 is newer than the source unless --force)

Set up the interpreter once:
  python3 -m venv target/psd/venv && target/psd/venv/bin/pip install psd-tools
"""

import json
import logging
import os
import sys
import traceback
import warnings

logging.disable(logging.CRITICAL)
warnings.filterwarnings("ignore")

import numpy as np  # noqa: E402
from psd_tools import PSDImage  # noqa: E402

GRID = 16


def blend_key(mode):
    v = getattr(mode, "value", mode)
    if isinstance(v, bytes):
        return v.decode("latin-1")
    return str(v)


def normalize_kind(layer):
    kind = layer.kind
    if kind in ("group", "artboard"):
        return "group"
    if kind == "type":
        return "text"
    if kind in ("solidcolorfill", "gradientfill"):
        return "fill"
    if kind in ("pixel", "shape", "smartobject", "patternfill"):
        return "pixel"
    return "adjustment"


def dump_layer(layer):
    bbox = layer.bbox
    out = {
        "name": layer.name,
        "kind": layer.kind,
        "class": normalize_kind(layer),
        "blend": blend_key(layer.blend_mode),
        "opacity": int(layer.opacity),
        "visible": bool(layer.visible),
        "clipping": bool(getattr(layer, "clipping", False) or getattr(layer, "clipping_layer", False)),
        "bbox": [int(bbox[0]), int(bbox[1]), int(bbox[2]), int(bbox[3])],
        "layer_id": int(layer.layer_id) if layer.layer_id is not None else None,
    }
    try:
        out["fill_opacity"] = int(layer.fill_opacity)
    except Exception:
        out["fill_opacity"] = None
    mask = layer.mask
    if mask is not None and (mask.width > 0 or mask.height > 0):
        out["mask"] = {
            "bbox": [int(mask.left), int(mask.top), int(mask.right), int(mask.bottom)],
            "disabled": bool(mask.disabled),
            "background": int(mask.background_color),
        }
    else:
        out["mask"] = None
    if layer.is_group():
        out["children"] = [dump_layer(c) for c in layer]
    return out


def merged_fingerprint(psd):
    """The merged image as psd-tools decodes it, in straight RGBA 0..255."""
    arr = psd.numpy()  # float 0..1, white matte removed for RGB with alpha
    if arr is None:
        return None
    h, w, c = arr.shape
    mode = int(psd.color_mode)
    if mode in (1, 8):  # grayscale, duotone
        rgb = np.repeat(arr[:, :, :1], 3, axis=2)
        alpha = np.ones((h, w, 1), dtype=np.float32)
    elif mode == 3 and c >= 3:
        rgb = arr[:, :, :3]
        from psd_tools.api.utils import has_transparency

        if c > 3 and has_transparency(psd):
            alpha = arr[:, :, 3:4]
        else:
            alpha = np.ones((h, w, 1), dtype=np.float32)
    else:
        return {"mode": mode, "skipped": True}
    # Colours composited over white, so transparent pixels compare cleanly.
    rgb = rgb * alpha + (1.0 - alpha)
    rgba = np.concatenate([rgb, alpha], axis=2) * 255.0
    mean = rgba.reshape(-1, 4).mean(axis=0)
    grid = []
    for gy in range(GRID):
        y0 = gy * h // GRID
        y1 = max((gy + 1) * h // GRID, y0 + 1)
        row = []
        for gx in range(GRID):
            x0 = gx * w // GRID
            x1 = max((gx + 1) * w // GRID, x0 + 1)
            cell = rgba[y0:y1, x0:x1].reshape(-1, 4).mean(axis=0)
            row.append([round(float(v), 2) for v in cell])
        grid.append(row)
    return {"mode": mode, "mean": [round(float(v), 3) for v in mean], "grid": grid}


def dump(path):
    psd = PSDImage.open(path)
    out = {
        "psd_tools": __import__("psd_tools").__version__,
        "version": int(psd.version),
        "width": int(psd.width),
        "height": int(psd.height),
        "depth": int(psd.depth),
        "channels": int(psd.channels),
        "color_mode": int(psd.color_mode),
        "layers": [dump_layer(l) for l in psd],
    }
    try:
        out["merged"] = merged_fingerprint(psd)
    except Exception as e:  # noqa: BLE001
        out["merged"] = {"error": f"{type(e).__name__}: {e}"}
    return out


def main(argv):
    if len(argv) >= 3 and argv[0] == "--corpus":
        root, outdir = argv[1], argv[2]
        force = "--force" in argv
        failures = 0
        for dirpath, _, files in os.walk(root):
            for name in sorted(files):
                if not name.lower().endswith((".psd", ".psb")):
                    continue
                src = os.path.join(dirpath, name)
                rel = os.path.relpath(src, root)
                dst = os.path.join(outdir, rel + ".json")
                if not force and os.path.exists(dst) and os.path.getmtime(dst) >= os.path.getmtime(src):
                    continue
                os.makedirs(os.path.dirname(dst), exist_ok=True)
                try:
                    data = dump(src)
                except Exception as e:  # noqa: BLE001
                    failures += 1
                    data = {"error": f"{type(e).__name__}: {e}"}
                with open(dst, "w") as f:
                    json.dump(data, f, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
                    f.write("\n")
        print(f"oracle: done ({failures} files psd-tools could not open)")
        return 0
    if len(argv) != 1:
        print(__doc__)
        return 2
    try:
        print(json.dumps(dump(argv[0]), ensure_ascii=False, sort_keys=True))
    except Exception as e:  # noqa: BLE001
        print(json.dumps({"error": f"{type(e).__name__}: {e}", "trace": traceback.format_exc()}))
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
