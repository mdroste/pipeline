#!/usr/bin/env bash
# Fail a release whose disk image would open as a bare, unarranged Finder window.
# Finder writes the layout into .DS_Store, and Tauri skips that step without any
# error when it believes it is running in CI.
set -euo pipefail

dmg_path="${1:?Supply the disk image to check}"
mount_point="$(mktemp -d)"

cleanup() {
  hdiutil detach "$mount_point" -quiet || hdiutil detach "$mount_point" -force -quiet || true
  rmdir "$mount_point" 2>/dev/null || true
}

hdiutil attach "$dmg_path" -nobrowse -readonly -noverify -noautoopen -quiet -mountpoint "$mount_point"
trap cleanup EXIT

fail() {
  echo "Disk image layout check failed: $1" >&2
  exit 1
}

[[ -d "$mount_point/Pipeline.app" ]] || fail "Pipeline.app is missing"
[[ -L "$mount_point/Applications" ]] || fail "the Applications link is missing"
[[ -f "$mount_point/.background/dmg-background.tiff" ]] || fail "the background picture is missing"
[[ -f "$mount_point/.DS_Store" ]] || fail "Finder did not save a layout"
# icvp holds the background and icon view; Iloc holds each icon's position.
grep -aq 'icvp' "$mount_point/.DS_Store" || fail "the layout has no icon view settings"
grep -aq 'Iloc' "$mount_point/.DS_Store" || fail "the layout has no icon positions"

echo "Disk image layout verified: $dmg_path"
