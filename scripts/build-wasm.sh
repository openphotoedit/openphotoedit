#!/usr/bin/env bash
# Builds crates/editor-wasm and writes the JS glue into apps/web/src/wasm-gen.
# `--dev` skips wasm-opt and uses the faster release-dev profile.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/apps/web/src/wasm-gen"
cd "$ROOT"
cargo build -p editor-wasm --target wasm32-unknown-unknown --release
mkdir -p "$OUT"
wasm-bindgen --target web --out-dir "$OUT" --out-name editor_wasm \
  target/wasm32-unknown-unknown/release/editor_wasm.wasm
if [ "${1:-}" != "--dev" ] && command -v wasm-opt >/dev/null; then
  # --enable-* must match what rustc emitted (simd128, and the features the
  # current stable enables by default), or wasm-opt rejects the module.
  wasm-opt -O3 --enable-simd --enable-bulk-memory --enable-nontrapping-float-to-int \
    --enable-sign-ext --enable-mutable-globals --enable-reference-types --enable-multivalue \
    "$OUT/editor_wasm_bg.wasm" -o "$OUT/editor_wasm_bg.wasm.opt" && mv "$OUT/editor_wasm_bg.wasm.opt" "$OUT/editor_wasm_bg.wasm"
fi
ls -la "$OUT/editor_wasm_bg.wasm"
