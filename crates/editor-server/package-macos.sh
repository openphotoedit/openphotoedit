#!/usr/bin/env bash
# Build OpenPhotoEdit as one thing a person downloads and double-clicks.
#
#   crates/editor-server/package-macos.sh        -> dist-app/OpenPhotoEdit.app
#
# The web app is embedded in the binary at compile time, so it is built first:
# a stale apps/web/dist would be baked in silently. Nothing is signed or
# uploaded; the bundle is for local testing and for the release pipeline to
# sign later.
set -euo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO="$(cd "$HERE/../.." && pwd)"
cd "$REPO"

APP_NAME="OpenPhotoEdit"
OUT="$REPO/dist-app"
BUNDLE="$OUT/$APP_NAME.app"
export CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$REPO/target/server}"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$REPO/Cargo.toml" | head -n1)"

echo "1/4  Building the web app (it is embedded in the binary)"
if [ ! -f "$REPO/apps/web/src/wasm-gen/editor_wasm_bg.wasm" ]; then
  echo "     no wasm build yet; running scripts/build-wasm.sh"
  "$REPO/scripts/build-wasm.sh"
fi
(cd "$REPO/apps/web" && npm run build --silent >/dev/null)
[ -f "$REPO/apps/web/dist/index.html" ] || { echo "apps/web/dist/index.html is missing after the build" >&2; exit 1; }

echo "2/4  Building the binary"
cargo build --release -p editor-server -q
BIN="$CARGO_TARGET_DIR/release/openphotoedit"
# A placeholder page means the embed did not pick up the build; refuse to ship that.
if strings "$BIN" | grep -q "This build has no web app inside"; then
  echo "the binary embedded the placeholder page, not apps/web/dist" >&2
  exit 1
fi

echo "3/4  Assembling $APP_NAME.app"
rm -rf "$BUNDLE"
mkdir -p "$BUNDLE/Contents/MacOS" "$BUNDLE/Contents/Resources"
cp "$BIN" "$BUNDLE/Contents/MacOS/$APP_NAME"
strip -x "$BUNDLE/Contents/MacOS/$APP_NAME" 2>/dev/null || true

echo "4/4  Icon"
ICON_KEY=""
# Placeholder icon: the favicon rendered by Quick Look, scaled into an iconset.
# Any step failing just means a bundle with the generic icon.
if command -v qlmanage >/dev/null && command -v iconutil >/dev/null; then
  TMP="$(mktemp -d)"
  if qlmanage -t -s 1024 -o "$TMP" "$REPO/apps/web/public/favicon.svg" >/dev/null 2>&1 && [ -f "$TMP/favicon.svg.png" ]; then
    SET="$TMP/icon.iconset"
    mkdir -p "$SET"
    for s in 16 32 128 256 512; do
      sips -z $s $s "$TMP/favicon.svg.png" --out "$SET/icon_${s}x${s}.png" >/dev/null
      sips -z $((s * 2)) $((s * 2)) "$TMP/favicon.svg.png" --out "$SET/icon_${s}x${s}@2x.png" >/dev/null
    done
    if iconutil -c icns "$SET" -o "$BUNDLE/Contents/Resources/icon.icns" 2>/dev/null; then
      ICON_KEY="<key>CFBundleIconFile</key><string>icon</string>"
    fi
  fi
  rm -rf "$TMP"
fi

cat > "$BUNDLE/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$APP_NAME</string>
  <key>CFBundleDisplayName</key><string>$APP_NAME</string>
  <key>CFBundleIdentifier</key><string>com.openphotoedit.desktop</string>
  <key>CFBundleExecutable</key><string>$APP_NAME</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  $ICON_KEY
  <!-- In the Dock, so it can be quit like every other app. A background-only
       agent would leave a server running that nobody could find or stop. -->
  <key>LSUIElement</key><false/>
  <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST

echo
echo "Built $BUNDLE"
du -sh "$BUNDLE" | awk '{print "  size: " $1}'
echo
echo "Unsigned: the first open needs right-click -> Open, once."
echo "Try it from a terminal: \"$BUNDLE/Contents/MacOS/$APP_NAME\" serve"
