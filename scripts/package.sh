#!/usr/bin/env bash
# Builds release binaries and assembles a distributable folder in dist/. Run from the repo root.
set -euo pipefail
OS="${1:-$(uname -s | tr '[:upper:]' '[:lower:]')}"
EXT=""; case "$OS" in mingw*|msys*|windows*) OS=windows; EXT=".exe";; darwin*) OS=macos;; esac
VERSION="$(grep -m1 '^version' crates/zeng-editor/Cargo.toml | cut -d'"' -f2)"
TARGET_DIR="${CARGO_TARGET_DIR:-target}"
cargo build --release -p zeng-player -p zeng-editor
rm -rf "dist/ZengEngine-$OS"; mkdir -p dist
if [ "$OS" = macos ]; then
  APP="dist/ZengEngine-$OS/ZengEngine.app"
  mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
  cp ${TARGET_DIR}/release/zeng-editor ${TARGET_DIR}/release/zeng-player "$APP/Contents/MacOS/"
  cp -r templates "$APP/Contents/Resources/templates"
  cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>ZengEngine</string>
<key>CFBundleExecutable</key><string>zeng-editor</string>
<key>CFBundleIdentifier</key><string>app.zengengine.editor</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>$VERSION</string>
<key>CFBundleVersion</key><string>$VERSION</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
  codesign --force --deep --sign - "$APP" 2>/dev/null || true
  cp README.md NOTICE LICENSE-MIT LICENSE-APACHE "dist/ZengEngine-$OS/"
  (cd dist && ditto -c -k --keepParent "ZengEngine-$OS" "ZengEngine-$VERSION-macos.zip")
else
  D="dist/ZengEngine-$OS"
  mkdir -p "$D"
  cp "${TARGET_DIR}/release/zeng-editor$EXT" "${TARGET_DIR}/release/zeng-player$EXT" "$D/"
  cp -r templates docs "$D/"
  cp README.md NOTICE LICENSE-MIT LICENSE-APACHE "$D/"
  if [ "$OS" = windows ]; then (cd dist && 7z a -tzip "ZengEngine-$VERSION-windows.zip" "ZengEngine-$OS" >/dev/null || powershell -Command "Compress-Archive -Force ZengEngine-$OS ZengEngine-$VERSION-windows.zip")
  else (cd dist && tar czf "ZengEngine-$VERSION-linux.tar.gz" "ZengEngine-$OS"); fi
fi
echo "Built dist/ for $OS $VERSION"
