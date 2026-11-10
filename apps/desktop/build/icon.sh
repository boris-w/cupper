#!/bin/sh
# Makes icon.icns here, the app's icon, from the latte master of
# docs/meetings/2026-11-10-app-icon/: the four electron-builder configs and
# the host bundle (`cargo xtask host-bundle`) take it (design 5.10.1).
# From the other masters, the Dock icons of the other colours in app-icons/,
# and every colour's small icon for the settings page (appIcon, T252).
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
masters=$here/../../../docs/meetings/2026-11-10-app-icon/icon
small=$here/../../../packages/ui-core/src/settings/icons
mkdir -p "$here/app-icons" "$small"
for colour in latte mocha sky matcha ink night; do
    # Latte is the bundle's icon: the Dock goes back to it.
    [ "$colour" = latte ] || cp "$masters/cupper-$colour.png" "$here/app-icons/$colour.png"
    # Shown at 40 points, twice that in pixels.
    sips -z 80 80 "$masters/cupper-$colour.png" --out "$small/$colour.png" >/dev/null
done
