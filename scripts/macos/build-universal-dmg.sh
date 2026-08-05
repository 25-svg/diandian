#!/usr/bin/env bash
set -euo pipefail

# Build a self-contained Universal macOS package on a Mac with Xcode command
# line tools. The script builds an arm64 and x86_64 FFmpeg, combines them with
# lipo, then asks Tauri to create a universal .app and .dmg.

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SRC_TAURI="$ROOT/src-tauri"
RESOURCE_DIR="$SRC_TAURI/resources/macos/universal"
WORK_DIR="${TMPDIR:-/tmp}/bili-shadowreplay-macos"
FFMPEG_VERSION="7.1.1"
FFMPEG_TAR="$WORK_DIR/ffmpeg-$FFMPEG_VERSION.tar.xz"
FFMPEG_SRC="$WORK_DIR/ffmpeg-$FFMPEG_VERSION"

command -v xcodebuild >/dev/null || { echo "Xcode Command Line Tools are required." >&2; exit 1; }
command -v curl >/dev/null || { echo "curl is required." >&2; exit 1; }
command -v npm >/dev/null || { echo "Node.js/npm is required." >&2; exit 1; }

mkdir -p "$WORK_DIR" "$RESOURCE_DIR"
if [[ ! -d "$FFMPEG_SRC" ]]; then
  curl --fail --location --output "$FFMPEG_TAR" "https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz"
  tar -C "$WORK_DIR" -xf "$FFMPEG_TAR"
fi

build_ffmpeg() {
  local arch="$1"
  local prefix="$WORK_DIR/install-$arch"
  local build="$WORK_DIR/build-$arch"
  rm -rf "$build" "$prefix"
  mkdir -p "$build" "$prefix"
  pushd "$build" >/dev/null
  "$FFMPEG_SRC/configure" \
    --prefix="$prefix" \
    --arch="$arch" \
    --target-os=darwin \
    --cc="xcrun --sdk macosx clang -arch $arch" \
    --cxx="xcrun --sdk macosx clang++ -arch $arch" \
    --extra-cflags="-arch $arch -mmacosx-version-min=13.0" \
    --extra-ldflags="-arch $arch -mmacosx-version-min=13.0" \
    --disable-debug \
    --disable-doc \
    --disable-ffplay \
    --enable-gpl \
    --enable-version3 \
    --enable-static \
    --disable-shared
  make -j"$(sysctl -n hw.ncpu)"
  make install
  popd >/dev/null
}

build_ffmpeg arm64
build_ffmpeg x86_64
lipo -create "$WORK_DIR/install-arm64/bin/ffmpeg" "$WORK_DIR/install-x86_64/bin/ffmpeg" -output "$RESOURCE_DIR/ffmpeg"
lipo -create "$WORK_DIR/install-arm64/bin/ffprobe" "$WORK_DIR/install-x86_64/bin/ffprobe" -output "$RESOURCE_DIR/ffprobe"
chmod +x "$RESOURCE_DIR/ffmpeg" "$RESOURCE_DIR/ffprobe"
lipo -archs "$RESOURCE_DIR/ffmpeg"
lipo -archs "$RESOURCE_DIR/ffprobe"

pushd "$ROOT" >/dev/null
npm ci
npx tauri build --config src-tauri/tauri.macos.conf.json --target universal-apple-darwin --bundles app,dmg
popd >/dev/null

echo "Packages: $SRC_TAURI/target/universal-apple-darwin/release/bundle/dmg"
