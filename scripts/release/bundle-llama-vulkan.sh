#!/usr/bin/env bash
# Fill the Windows-only llama.cpp Vulkan bundle. The release job signs these
# files, so graphics-card recognition runs with Smart App Control on.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
destination="$repo_root/gui/src-tauri/resources/llama-vulkan"
lock="$repo_root/scripts/release/llama-vulkan-lock.json"
case "$destination" in
  */gui/src-tauri/resources/llama-vulkan) ;;
  *) echo "refusing unsafe llama.cpp destination: $destination" >&2; exit 2 ;;
esac
rm -rf "$destination"
mkdir -p "$destination/licenses"

work="${RUNNER_TEMP:-$(mktemp -d)}/llama-vulkan"
rm -rf "$work"
mkdir -p "$work"
curl --fail --location --retry 3 \
  --output "$work/llama.zip" "$(jq -r '.archive.url' "$lock")"
echo "$(jq -r '.archive.sha256' "$lock")  $work/llama.zip" | sha256sum --check --strict
curl --fail --location --retry 3 \
  --output "$work/LICENSE" "$(jq -r '.licenseText.url' "$lock")"
echo "$(jq -r '.licenseText.sha256' "$lock")  $work/LICENSE" | sha256sum --check --strict

unzip -q "$work/llama.zip" -d "$work/unpacked"
cp "$work/unpacked/llama-server.exe" "$destination/"
for dll in "$work/unpacked/"*.dll; do
  case "$(basename "$dll")" in
    ggml-rpc.dll) continue ;;
  esac
  cp "$dll" "$destination/"
done
cp "$work/LICENSE" "$destination/licenses/LICENSE-llama.cpp.txt"

if [ "${OS:-}" = "Windows_NT" ]; then
  "$destination/llama-server.exe" --version
fi
