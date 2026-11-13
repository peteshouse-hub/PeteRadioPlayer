#!/bin/sh
set -eu
cd "$(dirname "$0")/../.."

case "${1:-}" in
    amd64) target=x86_64-unknown-linux-musl ;;
    arm64) target=aarch64-unknown-linux-musl ;;
    *) echo "Usage: $0 amd64|arm64" >&2; exit 2 ;;
esac
arch=$1
version=$(awk -F '"' '/^version = "/ { print $2; exit }' Cargo.toml)
version=$(printf '%s' "$version" | sed 's/-/~/')
linker_var=$(printf 'CARGO_TARGET_%s_LINKER' "$target" | tr '[:lower:]-' '[:upper:]_')
env "$linker_var=rust-lld" cargo build --release --locked --target "$target"

stage=$(mktemp -d "$PWD/target/debian.XXXXXX")
trap 'rm -rf "$stage"' 0
mkdir -p "$stage/control" "$stage/data/usr/bin" \
    "$stage/data/usr/share/applications" "$stage/data/usr/share/pixmaps" \
    "$stage/data/usr/share/doc/radioking"
install -m 755 "target/$target/release/radio-king" "$stage/data/usr/bin/radio-king"
install -m 644 packaging/fedora/radioking.desktop "$stage/data/usr/share/applications/radioking.desktop"
install -m 644 RadioKing.png "$stage/data/usr/share/pixmaps/radioking.png"
install -m 644 LICENSE "$stage/data/usr/share/doc/radioking/copyright"
cat >"$stage/control/control" <<CONTROL
Package: radioking
Version: $version-1
Architecture: $arch
Maintainer: RadioKing contributors
Section: sound
Priority: optional
Depends: ffmpeg, curl, ca-certificates
Recommends: xdg-utils
Homepage: https://github.com/peteshouse-hub/PeteRadioPlayer
Description: BBC and worldwide radio with adjustable BBC time shift
 RadioKing streams radio through the default Linux audio output.
CONTROL
printf '2.0\n' >"$stage/debian-binary"
# Both GNU tar and macOS bsdtar can write these portable archives.
COPYFILE_DISABLE=1 tar --format ustar --uid 0 --gid 0 -czf "$stage/control.tar.gz" -C "$stage/control" . 2>/dev/null ||
    tar --format=ustar --owner=0 --group=0 --numeric-owner -czf "$stage/control.tar.gz" -C "$stage/control" .
COPYFILE_DISABLE=1 tar --format ustar --uid 0 --gid 0 -czf "$stage/data.tar.gz" -C "$stage/data" . 2>/dev/null ||
    tar --format=ustar --owner=0 --group=0 --numeric-owner -czf "$stage/data.tar.gz" -C "$stage/data" .
dest="$PWD/target/radioking_${version}-1_${arch}.deb"
# Use short System V ar headers. macOS ar adds a Mach-O symbol table.
python3 - "$stage" <<'PYTHON'
from pathlib import Path
import sys

stage = Path(sys.argv[1])
with (stage / "package.deb").open("wb") as package:
    package.write(b"!<arch>\n")
    for name in ("debian-binary", "control.tar.gz", "data.tar.gz"):
        data = (stage / name).read_bytes()
        header = f"{name:16}{0:<12}{0:<6}{0:<6}{'100644':8}{len(data):<10}`\n"
        package.write(header.encode("ascii"))
        package.write(data)
        if len(data) % 2:
            package.write(b"\n")
PYTHON
mv "$stage/package.deb" "$dest"
printf '%s\n' "$dest"
