#!/usr/bin/env bash
set -e

LABEL="dev.artemmac.filesandbox"
PLIST="$HOME/Library/LaunchAgents/$LABEL.plist"
PROJECT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
LOG_DIR="$PROJECT_DIR/logs"
DAEMON_DIR="$PROJECT_DIR/daemon"
BINARY="$DAEMON_DIR/target/release/file-sandbox-daemon"

# ── Binary ───────────────────────────────────────────────────────────────────
# Build the release binary if it is not already present.
if [ ! -x "$BINARY" ]; then
  echo "Release binary not found — building..."
  if ! command -v cargo >/dev/null 2>&1; then
    echo "Error: cargo not found. Install Rust (https://rustup.rs) or build the binary manually:"
    echo "  cd $DAEMON_DIR && cargo build --release"
    exit 1
  fi
  ( cd "$DAEMON_DIR" && cargo build --release )
fi

if [ ! -x "$BINARY" ]; then
  echo "Error: build did not produce $BINARY."
  exit 1
fi

echo "Using binary: $BINARY"

# ── Preflight ────────────────────────────────────────────────────────────────
if [ ! -f "$PROJECT_DIR/config.json" ]; then
  echo "Warning: $PROJECT_DIR/config.json not found — daemon will fail to start without it."
fi

mkdir -p "$LOG_DIR"

# ── Write plist ──────────────────────────────────────────────────────────────
cat > "$PLIST" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>$LABEL</string>
    <key>ProgramArguments</key>
    <array>
        <string>$BINARY</string>
    </array>
    <key>WorkingDirectory</key>
    <string>$PROJECT_DIR</string>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <dict>
        <!-- Restart on crash, but not after clean launchctl stop -->
        <key>SuccessfulExit</key>
        <false/>
    </dict>
    <key>StandardOutPath</key>
    <string>$LOG_DIR/filesandbox.log</string>
    <key>StandardErrorPath</key>
    <string>$LOG_DIR/filesandbox.error.log</string>
    <key>ThrottleInterval</key>
    <integer>10</integer>
</dict>
</plist>
EOF

# ── Load ─────────────────────────────────────────────────────────────────────
# Unload first in case an old version is registered
launchctl bootout "gui/$(id -u)/$LABEL" 2>/dev/null || \
  launchctl unload "$PLIST" 2>/dev/null || true

if launchctl bootstrap "gui/$(id -u)" "$PLIST" 2>/dev/null; then
  echo "Loaded via bootstrap (macOS 12+)"
  launchctl kickstart "gui/$(id -u)/$LABEL" 2>/dev/null || true
elif launchctl load "$PLIST"; then
  echo "Loaded via launchctl load"
else
  echo "Warning: could not auto-load. Run manually:"
  echo "  launchctl load $PLIST"
fi

echo ""
echo "✓ Installed: $PLIST"
echo "  Binary:    $BINARY"
echo "  Logs:      $LOG_DIR/filesandbox.log"
echo "  Status:    launchctl list | grep $LABEL"
echo "  Stop:      launchctl stop $LABEL"
echo "  Remove:    bash scripts/uninstall-launchagent.sh"
