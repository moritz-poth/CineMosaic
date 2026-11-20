#!/bin/bash
# Builds "dist/CineMosaic.app" as a universal binary (Apple Silicon + Intel) and "dist/CineMosaic-<version>.zip".
# Usage: scripts/build-mac-app.sh        Requires: rustup target add x86_64-apple-darwin
# FFmpeg: static ffmpeg + ffprobe for both architectures in vendor/ffmpeg/arm64/ and vendor/ffmpeg/x86_64/ are put
# next to the app binary (frames::tool looks there first). Without them the app uses an installed ffmpeg – fine on
# this Mac, not releasable.
set -euo pipefail
cd "$(dirname "$0")/.."
NAME="CineMosaic"
RELEASE=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)   # e.g. 0.1.0-beta.1
VERSION=${RELEASE%%-*}                                                  # numbers only for macOS
BUILD=1                                                                 # bump with every release
APP="dist/$NAME.app"
TARGET="${CARGO_TARGET_DIR:-target}"

cargo build --release --bin cinemosaic-app --target aarch64-apple-darwin
cargo build --release --bin cinemosaic-app --target x86_64-apple-darwin
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
lipo -create "$TARGET/aarch64-apple-darwin/release/cinemosaic-app" "$TARGET/x86_64-apple-darwin/release/cinemosaic-app" \
  -output "$APP/Contents/MacOS/$NAME"

FFMPEG="bundled"
for tool in ffmpeg ffprobe; do
  if [ -x "vendor/ffmpeg/arm64/$tool" ] && [ -x "vendor/ffmpeg/x86_64/$tool" ]; then
    lipo -create "vendor/ffmpeg/arm64/$tool" "vendor/ffmpeg/x86_64/$tool" -output "$APP/Contents/MacOS/$tool"
  else
    FFMPEG="missing"
  fi
done

iconutil -c icns assets/AppIcon.iconset -o "$APP/Contents/Resources/AppIcon.icns"
cp LICENSE "$APP/Contents/Resources/LICENSE.txt"
cp ui/fonts/OFL.txt "$APP/Contents/Resources/OFL.txt"
cp THIRD-PARTY.md "$APP/Contents/Resources/THIRD-PARTY.md"
if [ -d docs/help/images ]; then
  mkdir -p "$APP/Contents/Resources/help"
  cp -R docs/help/images "$APP/Contents/Resources/help/"
fi

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$NAME</string>
  <key>CFBundleDisplayName</key><string>$NAME</string>
  <key>CFBundleExecutable</key><string>$NAME</string>
  <key>CFBundleIdentifier</key><string>io.github.moritz-poth.cinemosaic</string>
  <key>CFBundleVersion</key><string>$BUILD</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CineMosaicRelease</key><string>$RELEASE</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>NSHumanReadableCopyright</key><string>© 2026 Moritz Poth. GPL-3.0-or-later.</string>
</dict>
</plist>
PLIST
# ad-hoc signed (no Apple Developer account): macOS needs "Open Anyway" once, see README
codesign --force --deep --sign - "$APP"
ZIP="dist/$NAME-$RELEASE.zip"
rm -f "$ZIP"
ditto -c -k --keepParent "$APP" "$ZIP"
# update signature: installed apps only accept zips signed with the key in this Mac's Keychain
rm -f "$ZIP.sig"
if PUB=$(swift scripts/update-key.swift public 2>/dev/null) && grep -q "\"$PUB\"" src/lib.rs \
   && swift scripts/update-key.swift sign "$ZIP"; then
  SIGNED="signed (+ .sig – upload both)"
else
  SIGNED="NOT SIGNED (no update key, or it differs from UPDATE_APP.public_key)"
fi
echo "Done: $APP, $ZIP – $SIGNED"
if [ "$FFMPEG" = "missing" ]; then
  echo "NO FFMPEG BUNDLED (vendor/ffmpeg/{arm64,x86_64}/{ffmpeg,ffprobe} missing): the app uses an installed ffmpeg – not releasable" >&2
fi
