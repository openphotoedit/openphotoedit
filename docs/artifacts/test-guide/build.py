#!/usr/bin/env python3
"""Embed img/*.jpg into guide.html as data URIs -> index.html (the published page)."""
import base64, pathlib, re
here = pathlib.Path(__file__).parent
html = (here / "guide.html").read_text()
imgs = {}
for p in sorted((here / "img").glob("*.jpg")):
    key = "IMG_" + p.stem.split("-")[0].upper()
    imgs[key] = "data:image/jpeg;base64," + base64.b64encode(p.read_bytes()).decode()
missing = set(re.findall(r'"(IMG_[0-9A-Z]+)"', html)) - set(imgs)
if missing:
    raise SystemExit(f"missing images: {sorted(missing)}")
out = re.sub(r'"(IMG_[0-9A-Z]+)"', lambda m: '"' + imgs[m.group(1)] + '"', html)
(here / "index.html").write_text(out)
print(f"index.html {len(out)/1e6:.2f} MB, {len(imgs)} images")
