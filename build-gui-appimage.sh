#!/bin/bash
set -e

# Configuration
APP_NAME="topgrade-gui"
BIN_NAME="topgrade-gui"
SVG_ICON_PATH="doc/topgrade_transparent-quadrado.svg"
ICON_PATH="doc/topgrade_fixed_256.png" # Generated temporary icon
OUTPUT_DIR="dist"

echo "Building $APP_NAME..."

# Ensure we are in the project root
if [ ! -f "Cargo.toml" ]; then
    echo "Error: Cargo.toml not found. Please run this script from the project root."
    exit 1
fi

# 0. Generate Icon
echo "Generating standard 256x256 icon from SVG..."
if command -v magick >/dev/null 2>&1; then
    magick -background none "$SVG_ICON_PATH" -resize 256x256 "$ICON_PATH"
elif command -v convert >/dev/null 2>&1; then
    convert -background none "$SVG_ICON_PATH" -resize 256x256 "$ICON_PATH"
elif command -v rsvg-convert >/dev/null 2>&1; then
    rsvg-convert -w 256 -h 256 -f png -o "$ICON_PATH" "$SVG_ICON_PATH"
elif command -v inkscape >/dev/null 2>&1; then
    inkscape -o "$ICON_PATH" -w 256 -h 256 "$SVG_ICON_PATH"
else
    echo "Error: Missing SVG conversion tool."
    echo "To fix this, please install 'librsvg2-bin' or 'imagemagick':"
    echo "  sudo apt install librsvg2-bin"
    exit 1
fi

# 1. Build the binary
echo "Compiling release binary..."
cargo build --release --bin topgrade
cargo build --release --bin "$BIN_NAME" --features gui

# 2. Prepare AppDir
echo "Setting up AppDir..."
rm -rf AppDir
mkdir -p AppDir/usr/bin
mkdir -p AppDir/usr/share/applications
mkdir -p AppDir/usr/share/icons/hicolor/256x256/apps
mkdir -p AppDir/usr/share/icons/hicolor/scalable/apps

# Copy binary
cp "target/release/topgrade" AppDir/usr/bin/
cp "target/release/$BIN_NAME" AppDir/usr/bin/

# Copy icons
cp "$ICON_PATH" "AppDir/usr/share/icons/hicolor/256x256/apps/$APP_NAME.png"
cp "$SVG_ICON_PATH" "AppDir/usr/share/icons/hicolor/scalable/apps/$APP_NAME.svg"

# Create .desktop file
cat > AppDir/usr/share/applications/"$APP_NAME".desktop <<EOF
[Desktop Entry]
Name=Topgrade GUI
Exec=$BIN_NAME
Icon=$APP_NAME
Type=Application
Categories=System;Utility;
Terminal=false
Comment=Upgrade everything in your system
EOF

# 3. Download LinuxDeploy
echo "Downloading linuxdeploy..."
if [ ! -f "linuxdeploy-x86_64.AppImage" ]; then
    wget -q "https://github.com/linuxdeploy/linuxdeploy/releases/download/continuous/linuxdeploy-x86_64.AppImage"
    chmod +x linuxdeploy-x86_64.AppImage
    # Extract if necessary or run directly. Usually run directly.
fi

# Initialize OUTPUT_DIR
mkdir -p "$OUTPUT_DIR"

# 4. Generate AppImage
echo "Generating AppImage..."

# We need to set VERSION environment variable for linuxdeploy to name the AppImage correctly
export VERSION=$(grep '^version =' Cargo.toml | head -n1 | cut -d '"' -f 2)
echo "Detected Version: $VERSION"

./linuxdeploy-x86_64.AppImage \
    --appdir AppDir \
    --output appimage \
    --icon-file "$ICON_PATH" \
    --desktop-file "AppDir/usr/share/applications/$APP_NAME.desktop"

# Move to dist
mv Topgrade_GUI-*.AppImage "$OUTPUT_DIR/"

echo "Success! AppImage is available in $OUTPUT_DIR/"
