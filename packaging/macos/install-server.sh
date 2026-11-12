#!/bin/sh
# Build and install bbd as a LaunchAgent on this Mac.
set -eu
cd "$(dirname "$0")/../.."
cargo build --release -p bb-server
mkdir -p "$HOME/Applications/BlackBubbles"
BIN="$HOME/Applications/BlackBubbles/bbd"
cp target/release/bbd "$BIN"
# A stable (ad-hoc) signature lets macOS remember the permissions you grant.
codesign --force --sign - --identifier com.craftyderpysmall.bbd "$BIN"

PLIST="$HOME/Library/LaunchAgents/com.craftyderpysmall.bbd.plist"
mkdir -p "$HOME/Library/LaunchAgents" "$HOME/Library/Logs"
sed -e "s|__BIN__|$BIN|g" -e "s|__HOME__|$HOME|g" packaging/macos/com.craftyderpysmall.bbd.plist > "$PLIST"
launchctl bootout "gui/$(id -u)" "$PLIST" 2>/dev/null || true
launchctl bootstrap "gui/$(id -u)" "$PLIST"

cat <<MSG

bbd is installed and running ($BIN).

Next:
  1. System Settings → Privacy & Security → Full Disk Access → add $BIN
     (open "$HOME/Applications/BlackBubbles" in Finder and drag bbd in), then run:
       launchctl kickstart -k gui/$(id -u)/com.craftyderpysmall.bbd
  2. Pair a computer:   $BIN pair --host <this Mac's name or Tailscale IP>
  3. Find My:           $BIN findmy-login you@icloud.com
Logs: ~/Library/Logs/bbd.log
MSG
