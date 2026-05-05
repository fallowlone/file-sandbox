#!/usr/bin/env bash
set -e
cd "$(dirname "$0")"

# ── Icon ──────────────────────────────────────────────────────────────────────
if [ ! -f "AppIcon.icns" ]; then
  echo "Generating AppIcon.icns..."
  swift create-icon.swift
fi

# ── Swift build ───────────────────────────────────────────────────────────────
echo "Building FileSandboxMenuBar..."
swift build -c release

BINARY=".build/release/FileSandboxMenuBar"
APP="FileSandboxMenuBar.app"
MACOS_DIR="$APP/Contents/MacOS"
RESOURCES_DIR="$APP/Contents/Resources"

rm -rf "$APP"
mkdir -p "$MACOS_DIR" "$RESOURCES_DIR"
cp "$BINARY" "$MACOS_DIR/FileSandboxMenuBar"
cp "AppIcon.icns" "$RESOURCES_DIR/AppIcon.icns"

# ── Localizations ────────────────────────────────────────────────────────────
# Copy .lproj folders from the SwiftPM resource bundle into Contents/Resources/
# so Bundle.main resolves Text(LocalizedStringKey) at runtime.
SPM_BUNDLE=".build/release/FileSandboxMenuBar_FileSandboxMenuBar.bundle"
if [ -d "$SPM_BUNDLE" ]; then
  for lproj in "$SPM_BUNDLE"/*.lproj; do
    [ -d "$lproj" ] && cp -R "$lproj" "$RESOURCES_DIR/"
  done
fi

cat > "$APP/Contents/Info.plist" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>FileSandboxMenuBar</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundleIdentifier</key>
    <string>dev.artemmac.filesandbox-menubar</string>
    <key>CFBundleName</key>
    <string>FileSandboxMenuBar</string>
    <key>CFBundleVersion</key>
    <string>1.0</string>
    <key>LSMinimumSystemVersion</key>
    <string>13.0</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>NSPrincipalClass</key>
    <string>NSApplication</string>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleLocalizations</key>
    <array>
        <string>en</string>
        <string>ru</string>
    </array>
</dict>
</plist>
EOF

echo "Ad-hoc codesigning with virtualization entitlement..."
codesign --force --sign - \
         --entitlements sandbox.entitlements \
         --options runtime \
         "$APP"
codesign --verify --verbose=2 "$APP"

# ── Stage sandbox-base artifacts ─────────────────────────────────────────────
SANDBOX_IMG_DIR="$(cd .. && pwd)/sandbox-image/build"
SANDBOX_BASE_DIR="$HOME/Library/Application Support/FileSandbox/sandbox-base/current"
if [ -f "$SANDBOX_IMG_DIR/base.img" ] && [ -f "$SANDBOX_IMG_DIR/SHA256SUMS" ]; then
  echo "Staging sandbox base image to: $SANDBOX_BASE_DIR"
  mkdir -p "$SANDBOX_BASE_DIR"
  cp "$SANDBOX_IMG_DIR/base.img"    "$SANDBOX_BASE_DIR/"
  cp "$SANDBOX_IMG_DIR/vmlinuz"     "$SANDBOX_BASE_DIR/"
  cp "$SANDBOX_IMG_DIR/initrd.img"  "$SANDBOX_BASE_DIR/"
  ( cd "$SANDBOX_BASE_DIR" && shasum -a 256 -c "$SANDBOX_IMG_DIR/SHA256SUMS" ) \
    || { echo "WARNING: SHA-256 mismatch; sandbox will refuse to launch." >&2; }
else
  echo "Note: sandbox-image/build/base.img not found — run 'yarn sandbox:build' first to enable the sandbox feature."
fi

echo "Done: $(pwd)/$APP"
echo "Run: open \"$(pwd)/$APP\""
