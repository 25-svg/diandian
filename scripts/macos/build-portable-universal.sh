#!/usr/bin/env bash
set -euo pipefail

# Creates an unsigned, portable macOS folder. It deliberately does not create
# an .app, .dmg, .pkg, signature, or notarization record.

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SRC_TAURI="$ROOT/src-tauri"
OUTPUT_DIR="${OUTPUT_DIR:-$SRC_TAURI/target/portable-macos}"
PACKAGE_DIR="$OUTPUT_DIR/典典直播切片-mac"
WORK_DIR="${TMPDIR:-/tmp}/bili-shadowreplay-portable-macos"
FFMPEG_VERSION="7.1.1"
FFMPEG_ARCHIVE="$WORK_DIR/ffmpeg-$FFMPEG_VERSION.tar.xz"
FFMPEG_SOURCE="$WORK_DIR/ffmpeg-$FFMPEG_VERSION"

require_command() {
  command -v "$1" >/dev/null 2>&1 || {
    echo "Missing required command: $1" >&2
    exit 1
  }
}

[[ "$(uname -s)" == "Darwin" ]] || {
  echo "Run this script on macOS." >&2
  exit 1
}

require_command xcodebuild
require_command xcrun
require_command curl
require_command tar
require_command lipo
require_command npm
require_command rustup
require_command make

xcodebuild -version >/dev/null
rustup target add aarch64-apple-darwin x86_64-apple-darwin

mkdir -p "$WORK_DIR"
if [[ ! -d "$FFMPEG_SOURCE" ]]; then
  curl --fail --location --output "$FFMPEG_ARCHIVE" \
    "https://ffmpeg.org/releases/ffmpeg-$FFMPEG_VERSION.tar.xz"
  tar -C "$WORK_DIR" -xf "$FFMPEG_ARCHIVE"
fi

build_ffmpeg() {
  local arch="$1"
  local prefix="$WORK_DIR/install-$arch"
  local build="$WORK_DIR/build-$arch"

  rm -rf "$build" "$prefix"
  mkdir -p "$build" "$prefix"
  pushd "$build" >/dev/null
  "$FFMPEG_SOURCE/configure" \
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

rm -rf "$PACKAGE_DIR"
mkdir -p "$PACKAGE_DIR/bin"
lipo -create "$WORK_DIR/install-arm64/bin/ffmpeg" "$WORK_DIR/install-x86_64/bin/ffmpeg" \
  -output "$PACKAGE_DIR/bin/ffmpeg"
lipo -create "$WORK_DIR/install-arm64/bin/ffprobe" "$WORK_DIR/install-x86_64/bin/ffprobe" \
  -output "$PACKAGE_DIR/bin/ffprobe"
chmod 755 "$PACKAGE_DIR/bin/ffmpeg" "$PACKAGE_DIR/bin/ffprobe"

pushd "$ROOT" >/dev/null
npm ci
npx tauri build --bundles none --config src-tauri/tauri.macos.conf.json \
  --target universal-apple-darwin
popd >/dev/null

APP_BINARY="$SRC_TAURI/target/universal-apple-darwin/release/bili-shadowreplay"
[[ -f "$APP_BINARY" ]] || {
  echo "Tauri binary was not produced at: $APP_BINARY" >&2
  exit 1
}
cp "$APP_BINARY" "$PACKAGE_DIR/bin/bili-shadowreplay"
chmod 755 "$PACKAGE_DIR/bin/bili-shadowreplay"

cat > "$PACKAGE_DIR/启动.command" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")" && pwd)"
export PATH="$ROOT/bin:$PATH"
cd "$ROOT"
exec "$ROOT/bin/bili-shadowreplay"
EOF
chmod 755 "$PACKAGE_DIR/启动.command"

cat > "$PACKAGE_DIR/README.txt" <<'EOF'
典典直播切片（macOS 便携版）

使用方法：
1. 保持整个“典典直播切片-mac”文件夹完整。
2. 双击“启动.command”。
3. 不需要安装 Node.js、Rust、Python 或 FFmpeg。

首次从浏览器下载 ZIP 后，若 macOS 提示文件受隔离标记影响，请在终端执行：
xattr -dr com.apple.quarantine "/完整路径/典典直播切片-mac"

本版本未签名、未公证，支持 macOS 13 及更高版本的 Apple 芯片和 Intel Mac。
EOF

file "$PACKAGE_DIR/bin/bili-shadowreplay"
lipo -archs "$PACKAGE_DIR/bin/bili-shadowreplay"
lipo -archs "$PACKAGE_DIR/bin/ffmpeg"
lipo -archs "$PACKAGE_DIR/bin/ffprobe"

echo
echo "Portable package created: $PACKAGE_DIR"
echo "Send the whole folder, or zip it with:"
echo "  cd \"$OUTPUT_DIR\" && ditto -c -k --sequesterRsrc --keepParent \"典典直播切片-mac\" \"典典直播切片-mac.zip\""
