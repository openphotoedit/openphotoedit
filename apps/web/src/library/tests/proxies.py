"""Luminance proxies of the real test photos and edited variants, for cull tests.

Writes <out>/<name>.gray (raw 8-bit luminance, 512 px long edge, same pipeline
the worker uses: edit at full size, then downscale) and <out>/index.json.
"""
import io, json, os, sys
from PIL import Image, ImageEnhance, ImageFilter

root, out = sys.argv[1], sys.argv[2]
os.makedirs(out, exist_ok=True)
index = {}

def proxy(im, name):
    im = im.convert("RGB")
    s = 512 / max(im.size)
    w, h = max(1, round(im.width * s)), max(1, round(im.height * s))
    small = im.resize((w, h), Image.BOX).convert("L")
    open(os.path.join(out, name + ".gray"), "wb").write(small.tobytes())
    index[name] = [w, h]

for f in sorted(os.listdir(os.path.join(root, "photos"))):
    if not f.endswith(".jpg"):
        continue
    base = f[:-4]
    im = Image.open(os.path.join(root, "photos", f)).convert("RGB")
    proxy(im, base)
    buf = io.BytesIO(); im.save(buf, "JPEG", quality=70); buf.seek(0)
    proxy(Image.open(buf), base + "-recompressed")
    proxy(im.resize((im.width // 2, im.height // 2), Image.LANCZOS), base + "-half")
    cw, ch = int(im.width * 0.04), int(im.height * 0.04)
    proxy(im.crop((cw, ch, im.width - cw, im.height - ch)), base + "-crop")
    r = max(im.size) / 512 * 1.5
    proxy(im.filter(ImageFilter.GaussianBlur(r)), base + "-blur")
    proxy(ImageEnhance.Brightness(im).enhance(2.6), base + "-over")
    proxy(ImageEnhance.Brightness(im).enhance(0.12), base + "-under")
json.dump(index, open(os.path.join(out, "index.json"), "w"))
