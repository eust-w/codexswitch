#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_PATH="$ROOT_DIR/src-tauri/target/release/bundle/macos/codexswitch.app"
DMG_DIR="$ROOT_DIR/src-tauri/target/release/bundle/dmg"
ARCH="$(uname -m)"
VERSION="$(ROOT_DIR="$ROOT_DIR" node -p "require(process.env.ROOT_DIR + '/src-tauri/tauri.conf.json').version")"
OUT_DMG="$DMG_DIR/codexswitch_${VERSION}_${ARCH}.dmg"
TMP_DIR="$(mktemp -d)"

cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

export PATH="$HOME/.cargo/bin:/opt/homebrew/opt/rustup/bin:$PATH"

npm run build:mac

mkdir -p "$DMG_DIR"
rm -f "$OUT_DMG"
mkdir -p "$TMP_DIR/codexswitch"
cp -R "$APP_PATH" "$TMP_DIR/codexswitch/"
ln -s /Applications "$TMP_DIR/codexswitch/Applications"

hdiutil create \
  -volname codexswitch \
  -srcfolder "$TMP_DIR/codexswitch" \
  -ov \
  -format UDZO \
  "$OUT_DMG"

echo "Built DMG: $OUT_DMG"
