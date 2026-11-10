#!/bin/sh
# Makes icon.icns here, the app's icon, from the latte master of
# docs/meetings/2026-11-10-app-icon/: the four electron-builder configs and
# the host bundle (`cargo xtask host-bundle`) take it (design 5.10.1).
#   build/icon.sh [<master, 1024 x 1024 PNG>]
set -eu
here=$(cd "$(dirname "$0")" && pwd)
master=${1:-$here/../../../docs/meetings/2026-11-10-app-icon/icon/cupper-latte.png}
work=$(mktemp -d)
mkdir "$work/icon.iconset"
for size in 16 32 128 256 512; do
    sips -z "$size" "$size" "$master" --out "$work/icon.iconset/icon_${size}x${size}.png" >/dev/null
    double=$((size * 2))
    sips -z "$double" "$double" "$master" --out "$work/icon.iconset/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$work/icon.iconset" -o "$here/icon.icns"
rm -r "$work"
