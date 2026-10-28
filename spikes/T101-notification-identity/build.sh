#!/bin/sh
# Builds $SP10_OUT/SP10.app (default ./out) with Contents/Helpers/SP10Host.app, signed ad hoc
# with the hardened runtime as the self-use build (design 5.7.9). build.sh [own|bare]: the helper
# with a display name and icon of its own, or with CFBundleName only, as T005 writes the host's.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
out=${SP10_OUT:-$here/out}
variant=${1:-own}
app=$out/SP10.app
host=$app/Contents/Helpers/SP10Host.app
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources" "$host/Contents/MacOS" "$host/Contents/Resources"
swiftc -O -swift-version 5 -target arm64-apple-macos14 -module-name sp10 \
    -o "$app/Contents/MacOS/sp10" "$here/Common.swift" "$here/app/main.swift"
swiftc -O -swift-version 5 -target arm64-apple-macos14 -module-name sp10host \
    -o "$host/Contents/MacOS/sp10-host" "$here/Common.swift" "$here/host/main.swift"

icon() { # letter, r,g,b, the .icns to write
    mkdir -p "$out/$1.iconset"
    swift "$here/icon.swift" "$1" "$2" "$out/$1.iconset/icon_512x512@2x.png"
    iconutil -c icns "$out/$1.iconset" -o "$3" && rm -r "$out/$1.iconset"
}

plist() { # file, identifier, CFBundleName, executable, more keys
    cat >"$1" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleExecutable</key><string>$4</string>
<key>CFBundleIdentifier</key><string>$2</string>
<key>CFBundleName</key><string>$3</string>
<key>CFBundlePackageType</key><string>APPL</string>
<key>CFBundleShortVersionString</key><string>0.1.0</string><key>CFBundleVersion</key><string>0.1.0</string>
<key>LSMinimumSystemVersion</key><string>14.0</string>
$5
</dict></plist>
EOF
}

icon M 0.15,0.40,0.90 "$app/Contents/Resources/AppIcon.icns"
plist "$app/Contents/Info.plist" dev.cupper.spike.sp10 "SP10 Spike" sp10 \
    "<key>CFBundleIconFile</key><string>AppIcon</string>
<key>CFBundleURLTypes</key><array><dict><key>CFBundleURLName</key><string>dev.cupper.spike.sp10</string>
<key>CFBundleURLSchemes</key><array><string>sp10-spike</string></array></dict></array>"
case $variant in
own)
    icon H 0.95,0.50,0.10 "$host/Contents/Resources/AppIcon.icns"
    plist "$host/Contents/Info.plist" dev.cupper.spike.sp10.host SP10Host sp10-host \
        "<key>CFBundleDisplayName</key><string>SP10 Notifier</string>
<key>CFBundleIconFile</key><string>AppIcon</string><key>LSUIElement</key><true/>"
    ;;
bare) plist "$host/Contents/Info.plist" dev.cupper.spike.sp10.host SP10Host sp10-host \
    "<key>LSUIElement</key><true/>" ;;
*) echo "unknown variant: $variant" >&2 && exit 2 ;;
esac
# Inside out, as the product signs the host before the app.
codesign --force --options runtime --sign - "$host"
codesign --force --options runtime --sign - "$app"
codesign --verify --strict --deep "$app"
echo "$app ($variant)"
