#!/usr/bin/env bash
# Downloads the RAW test corpus into testdata/raw/ from raw.pixls.us.
# Every file is CC0 (public domain dedication); see testdata/raw/README.md.
# Files already present with the right sha256 are skipped. Re-runnable.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
DEST="$ROOT/testdata/raw"
mkdir -p "$DEST"

# local-name  sha256  url
FILES=(
  "canon-eos-r8.cr3 df33cf394573645ce03dea1b2e9f0b5cc2b7734e9e3391ee7114151414cf2812 https://raw.pixls.us/getfile.php/6587/nice/Canon%20-%20EOS%20R8%20-%203:2.CR3"
  "canon-eos-7d-mark-ii.cr2 b9f71c56adb6e2861e6653efb312fb5cc88ba4aa17fa809c6a18b8db044360c3 https://raw.pixls.us/getfile.php/1064/nice/Canon%20-%20EOS%207D%20Mark%20II%20-%20RAW%20(3:2).cr2"
  "nikon-z6.nef 3cbd9acf9e9bfd7d0c62c46436109556a101f7535c5fd25f86dad6ea61bf3886 https://raw.pixls.us/getfile.php/3584/nice/Nikon%20-%20Z%206%20-%2012bit%2012bit%20compressed%20(3:2).NEF"
  "sony-a7c.arw 6bfac3286f0e931e949b23e1bb236b737bb9e4117b8b42fa09e36f40f0fa8408 https://raw.pixls.us/getfile.php/4144/nice/Sony%20-%20ILCE-7C%20-%2014bit%2014bit%20compressed%20(3:2).ARW"
  "fujifilm-x-s10.raf 9419d1408ebf850395e5dd563beb96309466546252caf541466034c9cb7e9724 https://raw.pixls.us/getfile.php/4190/nice/Fujifilm%20-%20X-S10%20-%2014bit%2014bit%20compressed%20(3:2).RAF"
  "google-pixel-3a.dng 78c7bec867f3f739d43df6f027fad36ead73502d062570ce6ca14f59cdc4a0dd https://raw.pixls.us/getfile.php/3496/nice/Google%20-%20Pixel%203a%20-%2016bit%20(4:3).dng"
  "olympus-e-m1-mark-iii.orf 1e7a256eff14df75055dd7fe7a68aabefb4d8ed475317e80ebf7ff77d5bcfd3f https://raw.pixls.us/getfile.php/3800/nice/Olympus%20-%20E-M1MarkIII%20-%2016bit%20(4:3).ORF"
)

sha() { shasum -a 256 "$1" | cut -d' ' -f1; }

for entry in "${FILES[@]}"; do
  read -r name want url <<<"$entry"
  out="$DEST/$name"
  if [ -f "$out" ] && [ "$(sha "$out")" = "$want" ]; then
    echo "ok       $name"
    continue
  fi
  echo "fetching $name"
  curl -fL --retry 3 -o "$out.part" "$url"
  got="$(sha "$out.part")"
  if [ "$got" != "$want" ]; then
    rm -f "$out.part"
    echo "sha256 mismatch for $name: got $got, want $want" >&2
    exit 1
  fi
  mv "$out.part" "$out"
done
