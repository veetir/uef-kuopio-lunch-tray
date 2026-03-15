#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
SRC_LIGHT="${SCRIPT_DIR}/icon-square-light.svg"
SRC_DARK="${SCRIPT_DIR}/icon-square-dark.svg"
OUT_LIGHT="${SCRIPT_DIR}/icon-light.ico"
OUT_DARK="${SCRIPT_DIR}/icon-dark.ico"
OUT_DEFAULT="${SCRIPT_DIR}/icon.ico"

if ! command -v rsvg-convert >/dev/null 2>&1; then
  echo "error: rsvg-convert not found" >&2
  exit 1
fi

if ! command -v convert >/dev/null 2>&1; then
  echo "error: ImageMagick convert not found" >&2
  exit 1
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

build_ico() {
  local src_svg="$1"
  local out_ico="$2"
  for size in 16 20 24 32; do
    rsvg-convert -w "${size}" -h "${size}" "${src_svg}" -o "${tmp_dir}/icon-${size}.png"
  done
  convert \
    "${tmp_dir}/icon-16.png" \
    "${tmp_dir}/icon-20.png" \
    "${tmp_dir}/icon-24.png" \
    "${tmp_dir}/icon-32.png" \
    "${out_ico}"
}

build_ico "${SRC_LIGHT}" "${OUT_LIGHT}"
build_ico "${SRC_DARK}" "${OUT_DARK}"
cp "${OUT_LIGHT}" "${OUT_DEFAULT}"

echo "updated ${OUT_LIGHT}"
echo "updated ${OUT_DARK}"
echo "updated ${OUT_DEFAULT}"
