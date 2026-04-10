#!/usr/bin/env bash
#
# AMUX Build Script — Linux & macOS
#
# Usage:
#   ./scripts/build.sh              # Build for current platform
#   ./scripts/build.sh --release    # Release build (optimized + stripped)
#   ./scripts/build.sh --app        # Release build + macOS .app bundle
#   ./scripts/build.sh --package    # Build + create distributable archive
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
VERSION=$(grep '^version' "$PROJECT_DIR/Cargo.toml" | head -1 | sed 's/.*"\(.*\)"/\1/')

# Detect platform
OS="$(uname -s)"
ARCH="$(uname -m)"
case "$OS" in
    Linux*)  PLATFORM="linux";;
    Darwin*) PLATFORM="macos";;
    *)       echo "Unsupported OS: $OS"; exit 1;;
esac

# Parse args
RELEASE=""
PACKAGE=""
APP=""
for arg in "$@"; do
    case "$arg" in
        --release) RELEASE="1";;
        --package) RELEASE="1"; PACKAGE="1";;
        --app)     RELEASE="1"; APP="1";;
    esac
done

# Select GPUI features based on platform
if [ "$PLATFORM" = "linux" ]; then
    FEATURES="gpui-linux"
else
    FEATURES="gpui"
fi

echo "=== AMUX Build ==="
echo "Platform: $PLATFORM ($ARCH)"
echo "Version:  $VERSION"
echo "Features: $FEATURES"
echo ""

# Check dependencies
echo "--- Checking dependencies ---"
if [ "$PLATFORM" = "linux" ]; then
    MISSING=""
    for lib in libxcb.so libxkbcommon.so; do
        if ! ldconfig -p 2>/dev/null | grep -q "$lib"; then
            MISSING="$MISSING $lib"
        fi
    done
    if [ -n "$MISSING" ]; then
        echo "Missing libraries:$MISSING"
        echo "Install: sudo apt install libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev"
        exit 1
    fi
    echo "Linux dependencies: OK"
elif [ "$PLATFORM" = "macos" ]; then
    if ! xcode-select -p &>/dev/null; then
        echo "Xcode Command Line Tools required: xcode-select --install"
        exit 1
    fi
    echo "macOS dependencies: OK"
fi

# Build
echo ""
echo "--- Building ---"
cd "$PROJECT_DIR"

if [ -n "$RELEASE" ]; then
    cargo build -p amux-desktop --features "$FEATURES" --release 2>&1 | grep -E "Compiling amux|Finished|error" || true
    BINARY="target/release/Amux"
else
    cargo build -p amux-desktop --features "$FEATURES" 2>&1 | grep -E "Compiling amux|Finished|error" || true
    BINARY="target/debug/Amux"
fi

if [ ! -f "$BINARY" ]; then
    echo "Build failed!"
    exit 1
fi

echo ""
echo "Binary: $BINARY ($(du -h "$BINARY" | cut -f1))"

# Helper: generate macOS .icns from amux.jpg using sips + iconutil
generate_icns() {
    local SRC_JPG="$1"
    local OUT_ICNS="$2"
    local TMP_DIR
    TMP_DIR="$(mktemp -d)"
    local ICONSET_DIR="$TMP_DIR/amux.iconset"
    mkdir -p "$ICONSET_DIR"

    # Convert source to a 1024x1024 PNG first
    local SRC_PNG="$TMP_DIR/source.png"
    sips -s format png -z 1024 1024 "$SRC_JPG" --out "$SRC_PNG" >/dev/null 2>&1

    # Generate all required icon sizes from the PNG
    for size in 16 32 128 256 512; do
        sips -z $size $size "$SRC_PNG" --out "$ICONSET_DIR/icon_${size}x${size}.png" >/dev/null 2>&1
        local double=$((size * 2))
        sips -z $double $double "$SRC_PNG" --out "$ICONSET_DIR/icon_${size}x${size}@2x.png" >/dev/null 2>&1
    done

    iconutil -c icns "$ICONSET_DIR" -o "$OUT_ICNS"
    rm -rf "$TMP_DIR"
    echo "Generated: $OUT_ICNS"
}

# Helper: create macOS .app bundle
create_app_bundle() {
    local BINARY_SRC="$1"
    local APP_DIR="$2"
    local ICON_JPG="$PROJECT_DIR/assets/icons/amux.jpg"

    mkdir -p "$APP_DIR/Contents/MacOS"
    mkdir -p "$APP_DIR/Contents/Resources"

    cp "$BINARY_SRC" "$APP_DIR/Contents/MacOS/amux"

    # Generate .icns for the bundle icon
    generate_icns "$ICON_JPG" "$APP_DIR/Contents/Resources/amux.icns"

    cat > "$APP_DIR/Contents/Info.plist" << PLIST_EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>amux</string>
    <key>CFBundleIdentifier</key><string>com.amux.terminal</string>
    <key>CFBundleName</key><string>AMUX</string>
    <key>CFBundleDisplayName</key><string>AMUX</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleVersion</key><string>${VERSION}</string>
    <key>CFBundleShortVersionString</key><string>${VERSION}</string>
    <key>CFBundleIconFile</key><string>amux</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
</dict>
</plist>
PLIST_EOF
}

# --app: Quick .app bundle for dev/testing (macOS only)
if [ -n "$APP" ] && [ "$PLATFORM" = "macos" ]; then
    echo ""
    echo "--- Creating .app bundle ---"
    APP_DIR="$PROJECT_DIR/target/release/AMUX.app"
    rm -rf "$APP_DIR"
    create_app_bundle "$BINARY" "$APP_DIR"
    echo ""
    echo "=== .app bundle ready ==="
    echo "$APP_DIR"
    echo "Run: open $APP_DIR"
fi

# Package
if [ -n "$PACKAGE" ]; then
    echo ""
    echo "--- Packaging ---"

    # Strip debug symbols
    strip "$BINARY"
    echo "Stripped: $(du -h "$BINARY" | cut -f1)"

    # Create dist directory
    DIST_DIR="$PROJECT_DIR/dist"
    DIST_NAME="amux-${VERSION}-${PLATFORM}-${ARCH}"
    STAGE_DIR="$DIST_DIR/$DIST_NAME"
    rm -rf "$STAGE_DIR"
    mkdir -p "$STAGE_DIR"

    # Copy files
    cp "$BINARY" "$STAGE_DIR/amux"
    cp -r "$PROJECT_DIR/assets/icons" "$STAGE_DIR/"
    if [ "$PLATFORM" = "linux" ]; then
        cp "$PROJECT_DIR/assets/amux.desktop" "$STAGE_DIR/"
        # Create install script
        cat > "$STAGE_DIR/install.sh" << 'INSTALL_EOF'
#!/usr/bin/env bash
set -e
PREFIX="${1:-$HOME/.local}"
echo "Installing AMUX to $PREFIX ..."
mkdir -p "$PREFIX/bin"
cp amux "$PREFIX/bin/"
chmod +x "$PREFIX/bin/amux"
# Desktop entry & icon (Linux only)
if [ -f amux.desktop ]; then
    mkdir -p "$HOME/.local/share/applications"
    mkdir -p "$HOME/.local/share/icons/hicolor/scalable/apps"
    cp amux.desktop "$HOME/.local/share/applications/"
    cp icons/amux-icon.svg "$HOME/.local/share/icons/hicolor/scalable/apps/amux.svg"
fi
echo "Done! Run: amux"
INSTALL_EOF
        chmod +x "$STAGE_DIR/install.sh"
    fi

    # Create archive
    cd "$DIST_DIR"
    if [ "$PLATFORM" = "macos" ]; then
        # Create .app bundle for macOS
        APP_DIR="$STAGE_DIR/AMUX.app"
        rm -rf "$APP_DIR"
        create_app_bundle "$STAGE_DIR/amux" "$APP_DIR"
        rm "$STAGE_DIR/amux"  # binary is now inside .app
        tar -czf "${DIST_NAME}.tar.gz" "$DIST_NAME"
    else
        tar -czf "${DIST_NAME}.tar.gz" "$DIST_NAME"
    fi

    echo ""
    echo "=== Package ready ==="
    echo "$DIST_DIR/${DIST_NAME}.tar.gz"
    ls -lh "$DIST_DIR/${DIST_NAME}.tar.gz"
fi

echo ""
echo "=== Done ==="
