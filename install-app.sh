#!/bin/zsh
set -euo pipefail
SOURCE_DIR="${0:A:h}"
cd "$SOURCE_DIR"
DEST="$HOME/Applications/RadioKing.app"
BACKUP="$HOME/Applications/RadioKing (previous alpha).app"
case "${1:-}" in
  "")
    if /usr/sbin/lsof -nP -iTCP:8765 -sTCP:LISTEN >/dev/null 2>&1; then
      echo "Port 8765 is in use. Quit RadioKing or an older player before installing." >&2
      exit 1
    fi
    if [[ -e "$DEST" && -e "$BACKUP" ]]; then
      echo "Backup already exists: $BACKUP. Move it aside before installing." >&2
      exit 1
    fi
    ;;
  --package) ;;
  *) echo "Usage: $0 [--package]" >&2; exit 2 ;;
esac

cargo build --release --locked
STAGING=$(/usr/bin/mktemp -d "$SOURCE_DIR/target/radioking.XXXXXX")
trap 'rm -rf "$STAGING"' EXIT
APP="$STAGING/RadioKing.app"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
/usr/bin/swiftc -module-cache-path "$SOURCE_DIR/target/swift-module-cache" \
  -target "$(uname -m)-apple-macos13.0" -O -framework AppKit RadioKingLauncher.swift \
  -o "$APP/Contents/MacOS/RadioKing"
cp target/release/radio-king "$APP/Contents/Resources/RadioKingServer"
cp RadioKing.icns "$APP/Contents/Resources/RadioKing.icns"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>RadioKing</string>
<key>CFBundleDisplayName</key><string>RadioKing</string>
<key>CFBundleIdentifier</key><string>com.peteoliver.radioking.live</string>
<key>CFBundleExecutable</key><string>RadioKing</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.4.0</string>
<key>CFBundleVersion</key><string>0.4.0a11</string>
<key>CFBundleIconFile</key><string>RadioKing</string>
<key>LSMinimumSystemVersion</key><string>13.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
/usr/bin/plutil -lint "$APP/Contents/Info.plist"
/usr/bin/codesign --force --sign - "$APP"
/usr/bin/codesign --verify --deep --strict "$APP"

if [[ "${1:-}" == --package ]]; then
  ZIP="$SOURCE_DIR/target/RadioKing-v0.4.0-alpha.11-macOS-$(uname -m).zip"
  /usr/bin/ditto -c -k --sequesterRsrc --keepParent "$APP" "$STAGING/release.zip"
  mv -f "$STAGING/release.zip" "$ZIP"
  echo "Created: $ZIP"
else
  mkdir -p "$HOME/Applications"
  if [[ -e "$DEST" ]]; then
    mv "$DEST" "$BACKUP"
    echo "Preserved old app: $BACKUP"
  fi
  mv "$APP" "$DEST"
  echo "Created: $DEST"
  echo 'Launch with: open "$HOME/Applications/RadioKing.app"'
fi
