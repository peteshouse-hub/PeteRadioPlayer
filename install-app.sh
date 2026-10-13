#!/bin/zsh
set -euo pipefail
SOURCE_DIR="${0:A:h}"
APP="$HOME/Applications/RadioKing Live.app"
BACKUP="$HOME/Applications/RadioKing Live (alpha.3 backup).app"
cd "$SOURCE_DIR"
if /usr/sbin/lsof -nP -iTCP:8765 -sTCP:LISTEN 2>/dev/null | /usr/bin/grep -q LISTEN; then
  echo "Port 8765 is in use. Quit RadioKing Live (and any old player) before installing." >&2
  exit 1
fi
if [[ -e "$APP" && -e "$BACKUP" ]]; then
  echo "Backup already exists: $BACKUP. Move it aside first; no files changed." >&2
  exit 1
fi
cargo build --release
[[ -x target/release/radio-king-live ]] || exit 1
/usr/bin/swiftc -O -framework AppKit "$SOURCE_DIR/RadioKingLauncher.swift" -o "$SOURCE_DIR/RadioKingLive-launcher"
mkdir -p "$HOME/Applications"
if [[ -e "$APP" ]]; then
  mv "$APP" "$BACKUP"
  echo "Preserved old app: $BACKUP"
fi
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$SOURCE_DIR/RadioKingLive-launcher" "$APP/Contents/MacOS/RadioKingLive"
cp target/release/radio-king-live "$APP/Contents/Resources/RadioKingServer"
chmod 755 "$APP/Contents/MacOS/RadioKingLive" "$APP/Contents/Resources/RadioKingServer"
cp "$SOURCE_DIR/RadioKingLive.icns" "$APP/Contents/Resources/RadioKingLive.icns"
cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleName</key><string>RadioKing Live</string>
<key>CFBundleDisplayName</key><string>RadioKing Live</string>
<key>CFBundleIdentifier</key><string>com.peteoliver.radioking.live</string>
<key>CFBundleExecutable</key><string>RadioKingLive</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.4.0</string>
<key>CFBundleVersion</key><string>0.4.0a4</string>
<key>CFBundleIconFile</key><string>RadioKingLive</string>
<key>LSMinimumSystemVersion</key><string>13.0</string>
<key>NSHighResolutionCapable</key><true/>
</dict></plist>
PLIST
/usr/bin/plutil -lint "$APP/Contents/Info.plist"
touch "$APP"
echo "Created: $APP"
echo 'Launch with: open "$HOME/Applications/RadioKing Live.app"'
