#!/bin/sh
set -eu

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$project_dir"

if [ "$(uname -s)" != "Darwin" ] || [ "$(uname -m)" != "arm64" ]; then
    echo "This packager currently builds Dropcast.app on an Apple-silicon Mac." >&2
    exit 1
fi

version=$(sed -n 's/^version = "\([^"]*\)"/\1/p' Cargo.toml | head -n 1)
bundle_dir="$project_dir/target/macos/Dropcast.app"
contents_dir="$bundle_dir/Contents"
resources_dir="$contents_dir/Resources"
macos_dir="$contents_dir/MacOS"
temporary_dir=$(mktemp -d "${TMPDIR:-/tmp}/dropcast-package.XXXXXX")
trap 'rm -rf "$temporary_dir"' EXIT HUP INT TERM

cargo build --release --locked --bin dropcast-app

rm -rf "$bundle_dir"
mkdir -p "$resources_dir" "$macos_dir"
cp "$project_dir/target/release/dropcast-app" "$macos_dir/Dropcast"

iconset_dir="$temporary_dir/AppIcon.iconset"
mkdir -p "$iconset_dir"
qlmanage -t -s 1024 -o "$temporary_dir" "$project_dir/assets/dropcast-icon.svg" >/dev/null
source_icon="$temporary_dir/dropcast-icon.svg.png"

make_icon() {
    pixels=$1
    name=$2
    sips -z "$pixels" "$pixels" "$source_icon" --out "$iconset_dir/$name" >/dev/null
}

make_icon 16 icon_16x16.png
make_icon 32 icon_16x16@2x.png
make_icon 32 icon_32x32.png
make_icon 64 icon_32x32@2x.png
make_icon 128 icon_128x128.png
make_icon 256 icon_128x128@2x.png
make_icon 256 icon_256x256.png
make_icon 512 icon_256x256@2x.png
make_icon 512 icon_512x512.png
make_icon 1024 icon_512x512@2x.png
iconutil -c icns "$iconset_dir" -o "$resources_dir/AppIcon.icns"

cat >"$contents_dir/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleDisplayName</key>
    <string>Dropcast</string>
    <key>CFBundleExecutable</key>
    <string>Dropcast</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon.icns</string>
    <key>CFBundleIdentifier</key>
    <string>dev.mareknogiec.dropcast</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>Dropcast</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>$version</string>
    <key>CFBundleVersion</key>
    <string>$version</string>
    <key>LSApplicationCategoryType</key>
    <string>public.app-category.video</string>
    <key>LSMinimumSystemVersion</key>
    <string>12.0</string>
    <key>NSBonjourServices</key>
    <array>
        <string>_googlecast._tcp</string>
    </array>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSLocalNetworkUsageDescription</key>
    <string>Dropcast finds Cast-enabled TVs and streams your selected movie over your local network.</string>
</dict>
</plist>
EOF

plutil -lint "$contents_dir/Info.plist" >/dev/null
xattr -cr "$bundle_dir"
codesign --force --deep --sign "${CODE_SIGN_IDENTITY:--}" "$bundle_dir"
xattr -cr "$bundle_dir"
codesign --verify --deep --strict "$bundle_dir"

echo "Created $bundle_dir"
