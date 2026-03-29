#!/usr/bin/env bash
#
# AMUX Build Script — Linux & macOS
#
# Usage:
#   ./scripts/build.sh              # Build for current platform
#   ./scripts/build.sh --release    # Release build (optimized + stripped)
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
for arg in "$@"; do
    case "$arg" in
        --release) RELEASE="1";;
        --package) RELEASE="1"; PACKAGE="1";;
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
    BINARY="target/release/amux-desktop"
else
    cargo build -p amux-desktop --features "$FEATURES" 2>&1 | grep -E "Compiling amux|Finished|error" || true
    BINARY="target/debug/amux-desktop"
fi

if [ ! -f "$BINARY" ]; then
    echo "Build failed!"
    exit 1
fi

echo ""
echo "Binary: $BINARY ($(du -h "$BINARY" | cut -f1))"

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
        mkdir -p "$APP_DIR/Contents/MacOS"
        mkdir -p "$APP_DIR/Contents/Resources"
        mv "$STAGE_DIR/amux" "$APP_DIR/Contents/MacOS/amux"
        cp "$PROJECT_DIR/assets/icons/amux-icon.svg" "$APP_DIR/Contents/Resources/"
        cat > "$APP_DIR/Contents/Info.plist" << PLIST_EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key><string>amux</string>
    <key>CFBundleIdentifier</key><string>com.amux.terminal</string>
    <key>CFBundleName</key><string>AMUX</string>
    <key>CFBundleVersion</key><string>${VERSION}</string>
    <key>CFBundleShortVersionString</key><string>${VERSION}</string>
    <key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
PLIST_EOF
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
