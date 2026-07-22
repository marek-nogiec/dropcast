#!/bin/sh
set -eu

version=6.1.6
source_sha256=d4fcb164028dd3beee5d92c0ac72e46aac6973c75ea12dc14de07bf8f407370a
source_url="https://ffmpeg.org/releases/ffmpeg-${version}.tar.xz"
project_dir=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)

case "$(uname -s):$(uname -m)" in
  Darwin:arm64) asset=ffmpeg-darwin-arm64.gz ;;
  Darwin:x86_64) asset=ffmpeg-darwin-x64.gz ;;
  Linux:aarch64) asset=ffmpeg-linux-arm64.gz ;;
  Linux:x86_64) asset=ffmpeg-linux-x64.gz ;;
  *)
    echo "Unsupported FFmpeg build host: $(uname -s) $(uname -m)" >&2
    exit 1
    ;;
esac

output=${1:-"target/ffmpeg-bundles/${asset}"}
case "$output" in
  /*) output_path=$output ;;
  *) output_path="$project_dir/$output" ;;
esac
work_dir=$(mktemp -d "${TMPDIR:-/tmp}/dropcast-ffmpeg-build.XXXXXX")
trap 'rm -rf "$work_dir"' EXIT HUP INT TERM

archive="$work_dir/ffmpeg-${version}.tar.xz"
curl --fail --location --retry 3 --output "$archive" "$source_url"

actual_sha256=$(shasum -a 256 "$archive" | awk '{print $1}')
if test "$actual_sha256" != "$source_sha256"; then
  echo "FFmpeg source checksum mismatch" >&2
  exit 1
fi

tar -xJf "$archive" -C "$work_dir"
cd "$work_dir/ffmpeg-${version}"

./configure \
  --disable-autodetect \
  --disable-avdevice \
  --disable-debug \
  --disable-doc \
  --disable-ffplay \
  --disable-ffprobe \
  --disable-network \
  --disable-shared \
  --enable-small \
  --enable-static

build_jobs=$(sysctl -n hw.logicalcpu 2>/dev/null || getconf _NPROCESSORS_ONLN 2>/dev/null || echo 2)
make -j "$build_jobs" ffmpeg

configuration=$(./ffmpeg -version | sed -n 's/^configuration: //p')
case " $configuration " in
  *" --enable-gpl "*|*" --enable-nonfree "*)
    echo "Refusing to package an FFmpeg build with restricted flags: $configuration" >&2
    exit 1
    ;;
esac

mkdir -p "$(dirname "$output_path")"
gzip -9 -n -c ffmpeg > "$output_path.tmp"
mv "$output_path.tmp" "$output_path"
printf 'Built %s with configuration: %s\n' "$output_path" "$configuration"
