#!/usr/bin/env bash
# Puts every model OpenPhotoshop serves into .vendor/models/ (sha256-pinned),
# links them into apps/web/public/models/ for Vite, and copies the
# onnxruntime-web runtime into apps/web/public/ort/. Nothing is fetched from
# a third party at run time; this script is the only thing that downloads.
#
#   scripts/fetch-models.sh            fetch/verify all models, link, copy ORT
#   scripts/fetch-models.sh --tier1    only the small default models
#   scripts/fetch-models.sh --no-link  do not touch apps/web/public
#
# Order of sources for each file: already in .vendor/models with the right
# hash → a sibling checkout's .vendor (openpixels, openphotoid) with the right
# hash (APFS clone, instant) → download to `.part`, verify, rename.
#
# Real-ESRGAN publishes PyTorch weights only. If no sibling copy exists, run
# openpixels/scripts/export-models.py (needs torch) and re-run this script;
# the exported files are deterministic and match the pins below.
#
# Provenance and licences: MODELS.md.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEST="$ROOT/.vendor/models"
SIBLINGS=("$ROOT/../openpixels/.vendor/models" "$ROOT/../openphotoid/.vendor/models")
WEB="$ROOT/apps/web"
TIER1=0
LINK=1
for arg in "$@"; do
  case "$arg" in
    --tier1) TIER1=1 ;;
    --no-link) LINK=0 ;;
    *) echo "unknown option $arg" >&2; exit 2 ;;
  esac
done

HF=https://huggingface.co
# tier|path under models/|bytes|sha256|url ("-" = no direct download)
MODELS=(
  "1|migan_pipeline_v2.onnx|28079181|6f1f3530a1a2324b19752018ce756088b07973cda8d7d890034ace5c8a48c40b|$HF/andraniksargsyan/migan/resolve/main/migan_pipeline_v2.onnx"
  "2|lama_fp32.onnx|208044816|1faef5301d78db7dda502fe59966957ec4b79dd64e16f03ed96913c7a4eb68d6|$HF/Carve/LaMa-ONNX/resolve/main/lama_fp32.onnx"
  "1|modnet_photographic_portrait_matting.onnx|25888640|07c308cf0fc7e6e8b2065a12ed7fc07e1de8febb7dc7839d7b7f15dd66584df9|https://github.com/Zeyi-Lin/HivisionIDPhotos/releases/download/pretrained-model/modnet_photographic_portrait_matting.onnx"
  "1|u2netp.onnx|4574861|309c8469258dda742793dce0ebea8e6dd393174f89934733ecc8b14c76f4ddd8|https://github.com/danielgatis/rembg/releases/download/v0.0.0/u2netp.onnx"
  "1|edgetam/vision_encoder.onnx|192225|ed068218eba96760fe02d04ce899c449660ac813a088d80ed7f42c8bb01e7cec|$HF/onnx-community/EdgeTAM-ONNX/resolve/main/onnx/vision_encoder.onnx"
  "1|edgetam/vision_encoder.onnx_data|19532576|21e75dba7077dfcb53e8c9a6e99977156f2240ff3f1f9cddc43d66aa1ecb528e|$HF/onnx-community/EdgeTAM-ONNX/resolve/main/onnx/vision_encoder.onnx_data"
  "1|edgetam/prompt_encoder_mask_decoder.onnx|213114|d3668299ec3edf70fbb139ec642b54bf3d4be453fd1b688a6b5938e0856fe546|$HF/onnx-community/EdgeTAM-ONNX/resolve/main/onnx/prompt_encoder_mask_decoder.onnx"
  "1|edgetam/prompt_encoder_mask_decoder.onnx_data|20958208|dfa2125e30d08d388732f20c18fb63ab0f0f590cd270eb307f0c2b65919d1be8|$HF/onnx-community/EdgeTAM-ONNX/resolve/main/onnx/prompt_encoder_mask_decoder.onnx_data"
  "1|face_detection_yunet_2023mar.onnx|232589|8f2383e4dd3cfbb4553ea8718107fc0423210dc964f9f4280604804ed2552fa4|https://media.githubusercontent.com/media/opencv/opencv_zoo/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx"
  "1|realesr_general_x4v3.onnx|4866422|0e121299bf9b23a764d3f9e2120d0d0727d5ac5032e03a46cb78d28bcc7c6872|-"
  "1|realesr_general_x4v3_dn50.onnx|4866422|b345bc59c38dbb78d871b5adf9519c475fe243c198f70cc9d0b97a464715d50b|-"
  "1|realesr_general_wdn_x4v3.onnx|4866422|be536211515f088de05f1aa799a8079e92f49978b47aad6cec007f9fca62bb68|-"
  "2|realesrgan_x4plus.onnx|67051644|800a80063abcc8db53f6579407347ac2d53a9f5697dcc839cff38fbd806faf37|-"
  "2|gfpgan_1.4.onnx|340299087|accc4757b26bdb89b32b4d3500d4f79c9dff97c1dd7c7104bf9dcb95e3311385|$HF/facefusion/models-3.0.0/resolve/main/gfpgan_1.4.onnx"
  "2|deoldify_artistic.onnx|255044725|9ac296cf05fecbdb604f50211f632b722402bc1f9a96ee5a8987b01c0c3c688f|$HF/facefusion/models-3.0.0/resolve/main/deoldify_artistic.onnx"
)

sha() { shasum -a 256 "$1" | cut -d' ' -f1; }

fail=0
echo "Models → $DEST"
for entry in "${MODELS[@]}"; do
  IFS='|' read -r tier path bytes want url <<<"$entry"
  if [ "$TIER1" = 1 ] && [ "$tier" != 1 ]; then continue; fi
  dst="$DEST/$path"
  mkdir -p "$(dirname "$dst")"
  if [ -f "$dst" ] && [ "$(stat -f%z "$dst" 2>/dev/null || stat -c%s "$dst")" = "$bytes" ] && [ "$(sha "$dst")" = "$want" ]; then
    echo "  ok      $path"
    continue
  fi
  found=""
  for s in "${SIBLINGS[@]}"; do
    cand="$s/$(basename "$path")"
    if [ -f "$cand" ] && [ "$(sha "$cand")" = "$want" ]; then found="$cand"; break; fi
  done
  if [ -n "$found" ]; then
    cp -c "$found" "$dst.part" 2>/dev/null || cp "$found" "$dst.part"
    mv "$dst.part" "$dst"
    echo "  sibling $path  ($found)"
    continue
  fi
  if [ "$url" = "-" ]; then
    echo "  MISSING $path — no direct download; run openpixels/scripts/export-models.py (needs torch), then re-run" >&2
    fail=1
    continue
  fi
  echo "  get     $path"
  # .part until the checksum passes: an interrupted fetch must never leave a
  # truncated model that looks cached next time.
  curl -fL --retry 3 --progress-bar -o "$dst.part" "$url"
  got="$(sha "$dst.part")"
  if [ "$got" != "$want" ]; then
    rm -f "$dst.part"
    echo "  MISMATCH $path: expected $want, got $got" >&2
    fail=1
    continue
  fi
  mv "$dst.part" "$dst"
done

if [ "$LINK" = 1 ]; then
  # Per-file symlinks inside a real directory: the directory is gitignored
  # (a symlinked directory would show up as an untracked file), Vite's dev
  # server follows the links, and `vite build` copies the real bytes.
  echo "Linking → apps/web/public/models"
  for entry in "${MODELS[@]}"; do
    IFS='|' read -r tier path bytes want url <<<"$entry"
    [ -f "$DEST/$path" ] || continue
    link="$WEB/public/models/$path"
    mkdir -p "$(dirname "$link")"
    ln -sfn "$DEST/$path" "$link"
  done

  echo "onnxruntime-web → apps/web/public/ort"
  ORT_DIST="$WEB/node_modules/onnxruntime-web/dist"
  if [ -d "$ORT_DIST" ]; then
    mkdir -p "$WEB/public/ort"
    # The WebGPU build loads the asyncify runtime, the CPU build the plain
    # one. Both are served from this origin; the default is a CDN.
    for f in ort-wasm-simd-threaded.wasm ort-wasm-simd-threaded.mjs ort-wasm-simd-threaded.asyncify.wasm ort-wasm-simd-threaded.asyncify.mjs; do
      cp "$ORT_DIST/$f" "$WEB/public/ort/$f"
    done
    grep '"version"' "$WEB/node_modules/onnxruntime-web/package.json" | head -1 | sed 's/^ */  onnxruntime-web /'
  else
    echo "  apps/web/node_modules missing — run npm install in apps/web, then re-run" >&2
    fail=1
  fi
fi

[ "$fail" = 0 ] || { echo "fetch-models.sh: some models are missing or failed verification" >&2; exit 1; }
echo "Done."
