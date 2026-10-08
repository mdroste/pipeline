#!/usr/bin/env bash
set -euo pipefail

platform="${1:?usage: bundle-poppler.sh <macos-arm64|macos-x64|windows|linux>}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
destination="$repo_root/gui/src-tauri/resources/poppler"
lock="$repo_root/scripts/release/poppler-lock.json"
case "$destination" in
  */gui/src-tauri/resources/poppler) ;;
  *) echo "refusing unsafe Poppler destination: $destination" >&2; exit 2 ;;
esac
rm -rf "$destination"
mkdir -p "$destination"

case "$platform" in
  macos-arm64|macos-x64)
    export HOMEBREW_NO_AUTO_UPDATE=1
    brew list --versions poppler >/dev/null 2>&1 || brew install poppler
    brew list --versions dylibbundler >/dev/null 2>&1 || brew install dylibbundler

    work="$(mktemp -d)"
    trap 'rm -rf "$work"' EXIT
    poppler_prefix="$(brew --prefix poppler)"
    cp "$poppler_prefix/bin/pdftoppm" "$poppler_prefix/bin/pdftotext" "$work/"
    search_args=()
    for lib_dir in "$(brew --prefix)"/opt/*/lib; do
      [ -d "$lib_dir" ] && search_args+=(-s "$lib_dir")
    done
    (
      cd "$work"
      dylibbundler -od -b -x ./pdftoppm -x ./pdftotext \
        -d ./lib -p '@executable_path/lib/' "${search_args[@]}" </dev/null
    )
    mkdir -p "$destination/lib" "$destination/share" "$destination/licenses/poppler"
    cp "$work/pdftoppm" "$work/pdftotext" "$destination/"
    cp "$work/lib/"*.dylib "$destination/lib/"
    cp -RL "$poppler_prefix/share/poppler" "$destination/share/poppler"
    cp "$poppler_prefix/COPYING" "$destination/licenses/poppler/"
    ;;

  linux)
    mkdir -p "$destination/lib" "$destination/share" "$destination/licenses/poppler"
    cp "$(command -v pdftoppm)" "$(command -v pdftotext)" "$destination/"
    ldd "$(command -v pdftoppm)" "$(command -v pdftotext)" \
      | awk '/=> \// {print $3}' | sort -u \
      | while IFS= read -r library; do
          name="$(basename "$library")"
          case "$name" in
            ld-linux-*|libc.so.*|libdl.so.*|libpthread.so.*|libm.so.*|librt.so.*|libresolv.so.*|libnsl.so.*|libutil.so.*) continue ;;
          esac
          cp -L "$library" "$destination/lib/$name"
        done
    patchelf --set-rpath '$ORIGIN/lib:$ORIGIN' "$destination/pdftoppm"
    patchelf --set-rpath '$ORIGIN/lib:$ORIGIN' "$destination/pdftotext"
    while IFS= read -r library; do
      file "$library" | grep -q ELF && patchelf --set-rpath '$ORIGIN' "$library"
    done < <(find "$destination/lib" -type f | sort)
    cp -RL /usr/share/poppler "$destination/share/poppler"
    cp /usr/share/common-licenses/GPL-2 "$destination/licenses/poppler/GPL-2.txt"
    ;;

  windows)
    distribution_version="$(jq -r '.windows.distributionVersion' "$lock")"
    poppler_version="$(jq -r '.windows.version' "$lock")"
    archive="$RUNNER_TEMP/poppler.zip"
    unpacked="$RUNNER_TEMP/poppler-unpacked"
    curl --fail --location --retry 3 \
      --output "$archive" "$(jq -r '.windows.archive.url' "$lock")"
    echo "$(jq -r '.windows.archive.sha256' "$lock")  $archive" | sha256sum --check --strict
    unzip -q "$archive" -d "$unpacked"
    root="$unpacked/poppler-$poppler_version"
    cp "$root/Library/bin/pdftoppm.exe" "$root/Library/bin/pdftotext.exe" "$destination/"
    for dll in "$root/Library/bin/"*.dll; do
      case "$(basename "$dll")" in
        Qt5*|Qt6*|poppler-cpp.dll|poppler-glib.dll) continue ;;
      esac
      cp "$dll" "$destination/"
    done
    mkdir -p "$destination/share" "$destination/licenses/poppler"
    cp -R "$root/share/poppler" "$destination/share/poppler"
    cp "$root/share/poppler/COPYING"* "$destination/licenses/poppler/"
    printf '%s\n' "$distribution_version" > "$destination/DISTRIBUTION_VERSION"
    ;;

  *)
    echo "unsupported platform: $platform" >&2
    exit 2
    ;;
esac

case "$platform" in
  windows) "$destination/pdftotext.exe" -v ;;
  *) "$destination/pdftotext" -v ;;
esac

node "$repo_root/scripts/release/poppler-inventory.mjs" "$platform"
