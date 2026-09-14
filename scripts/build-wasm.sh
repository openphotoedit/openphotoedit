#!/usr/bin/env bash
# Builds crates/editor-wasm and writes the JS glue into apps/web/src/wasm-gen.
# `--dev` skips wasm-opt. Serialised with a lock directory, because several
# sessions build at once and two wasm-bindgen runs writing the same files
# produce a module whose glue and binary disagree.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
OUT="$ROOT/apps/web/src/wasm-gen"
LOCK="$ROOT/target/.build-wasm.lock"
mkdir -p "$ROOT/target"
for i in $(seq 1 600); do
  if mkdir "$LOCK" 2>/dev/null; then break; fi
  # A lock older than 15 minutes belongs to a build that died.
  if [ -n "$(find "$LOCK" -maxdepth 0 -mmin +15 2>/dev/null)" ]; then rmdir "$LOCK" 2>/dev/null || true; fi
  sleep 1
done
trap 'rmdir "$LOCK" 2>/dev/null || true' EXIT
cd "$ROOT"
CARGO_TARGET_DIR="$ROOT/target/wasm" cargo build -p editor-wasm --target wasm32-unknown-unknown --release
mkdir -p "$OUT.tmp"
wasm-bindgen --target web --out-dir "$OUT.tmp" --out-name editor_wasm \
  target/wasm/wasm32-unknown-unknown/release/editor_wasm.wasm
if [ "${1:-}" != "--dev" ] && command -v wasm-opt >/dev/null; then
  wasm-opt -O3 --enable-simd --enable-bulk-memory --enable-nontrapping-float-to-int \
    --enable-sign-ext --enable-mutable-globals --enable-reference-types --enable-multivalue \
    "$OUT.tmp/editor_wasm_bg.wasm" -o "$OUT.tmp/editor_wasm_bg.wasm.opt" && mv "$OUT.tmp/editor_wasm_bg.wasm.opt" "$OUT.tmp/editor_wasm_bg.wasm"
fi
rm -rf "$OUT" && mv "$OUT.tmp" "$OUT"
ls -la "$OUT/editor_wasm_bg.wasm"
