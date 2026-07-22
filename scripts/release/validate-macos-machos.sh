#!/usr/bin/env bash

set -euo pipefail

root=""
expected_arch=""
max_minos=""
poppler_layout=false
require_developer_id=false

while (($#)); do
  case "$1" in
    --root) root="$2"; shift 2 ;;
    --expected-arch) expected_arch="$2"; shift 2 ;;
    --max-minos) max_minos="$2"; shift 2 ;;
    --poppler-layout) poppler_layout=true; shift ;;
    --require-developer-id) require_developer_id=true; shift ;;
    *) echo "Unknown argument: $1" >&2; exit 2 ;;
  esac
done

if [[ ! -d "$root" || -z "$expected_arch" || -z "$max_minos" ]]; then
  echo "usage: validate-macos-machos.sh --root DIR --expected-arch ARCH --max-minos VERSION [--poppler-layout] [--require-developer-id]" >&2
  exit 2
fi

version_at_most() {
  awk -v actual="$1" -v maximum="$2" 'BEGIN {
    split(actual, a, "."); split(maximum, m, ".");
    for (i = 1; i <= 4; i++) {
      av = (a[i] == "" ? 0 : a[i]) + 0;
      mv = (m[i] == "" ? 0 : m[i]) + 0;
      if (av < mv) exit 0;
      if (av > mv) exit 1;
    }
    exit 0;
  }'
}

minimum_macos() {
  otool -l "$1" | awk '
    /LC_BUILD_VERSION/ { build = 1; legacy = 0; next }
    build && /minos/ { print $2; exit }
    /LC_VERSION_MIN_MACOSX/ { legacy = 1; build = 0; next }
    legacy && /version/ { print $2; exit }
  '
}

resolve_poppler_dependency() {
  local binary="$1"
  local dependency="$2"
  case "$dependency" in
    @executable_path/*)
      printf '%s/%s\n' "$root" "${dependency#@executable_path/}"
      ;;
    @loader_path/*)
      printf '%s/%s\n' "$(dirname "$binary")" "${dependency#@loader_path/}"
      ;;
    *)
      return 1
      ;;
  esac
}

count=0
while IFS= read -r -d '' binary; do
  if ! file "$binary" | grep -q 'Mach-O'; then
    continue
  fi
  count=$((count + 1))

  archs="$(lipo -archs "$binary")"
  if [[ "$archs" != "$expected_arch" ]]; then
    echo "Expected $expected_arch, found $archs: $binary" >&2
    exit 1
  fi

  minos="$(minimum_macos "$binary")"
  if [[ -z "$minos" ]]; then
    echo "No minimum macOS load command: $binary" >&2
    exit 1
  fi
  if ! version_at_most "$minos" "$max_minos"; then
    echo "$binary requires macOS $minos, above declared $max_minos" >&2
    exit 1
  fi

  codesign --verify --strict --verbose=2 "$binary"
  if $require_developer_id && ! codesign -dv --verbose=4 "$binary" 2>&1 | grep -q '^Authority=Developer ID Application:'; then
    echo "Mach-O lacks a Developer ID Application signature: $binary" >&2
    exit 1
  fi

  self_id="$(otool -D "$binary" 2>/dev/null | sed -n '2p' || true)"
  while IFS= read -r dependency; do
    [[ -z "$dependency" || "$dependency" == "$self_id" ]] && continue
    case "$dependency" in
      /System/Library/*|/usr/lib/*) ;;
      /opt/homebrew/*|/usr/local/*|*/Cellar/*)
        echo "Non-relocatable dependency in $binary: $dependency" >&2
        exit 1
        ;;
      @rpath/*)
        if $poppler_layout; then
          echo "Unresolved @rpath dependency in bundled Poppler $binary: $dependency" >&2
          exit 1
        fi
        ;;
      @executable_path/*|@loader_path/*)
        if $poppler_layout; then
          resolved="$(resolve_poppler_dependency "$binary" "$dependency")"
          if [[ ! -f "$resolved" ]]; then
            echo "Missing relocated dependency for $binary: $dependency -> $resolved" >&2
            exit 1
          fi
        fi
        ;;
      /*)
        echo "Unexpected absolute dependency in $binary: $dependency" >&2
        exit 1
        ;;
    esac
  done < <(otool -L "$binary" | tail -n +2 | awk '{print $1}')
done < <(find "$root" -type f -print0)

if ((count == 0)); then
  echo "No Mach-O files found under $root" >&2
  exit 1
fi

echo "Validated $count Mach-O files under $root (arch $expected_arch, minimum macOS <= $max_minos)"

