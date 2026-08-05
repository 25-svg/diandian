# macOS Universal 打包说明

## 目标

产出同时支持 Apple 芯片和 Intel Mac 的 `.dmg`。安装后自带 FFmpeg 与 FFprobe；不要求最终用户安装 Node.js、Rust、Python 或 FFmpeg。

## 构建机要求

- 一台 macOS 13 或更高版本的 Mac。
- Xcode Command Line Tools：`xcode-select --install`。
- Node.js/npm 与网络连接。
- 可选 Apple Developer 证书。没有证书也能构建，但目标用户首次打开需右键“打开”。

## 构建

在仓库根目录执行：

```bash
chmod +x scripts/macos/build-universal-dmg.sh
scripts/macos/build-universal-dmg.sh
```

脚本会：

1. 分别构建 arm64 与 x86_64 FFmpeg/FFprobe。
2. 用 `lipo` 合成 Universal 二进制，并放入 `.app/Contents/Resources`。
3. 用 Tauri 构建 Universal `.app` 和 `.dmg`。

输出目录：`src-tauri/target/universal-apple-darwin/release/bundle/dmg/`。

## 验收

```bash
file "典典直播切片.app/Contents/MacOS/典典直播切片"
lipo -archs "典典直播切片.app/Contents/Resources/ffmpeg"
lipo -archs "典典直播切片.app/Contents/Resources/ffprobe"
```

三个命令都应包含 `arm64 x86_64`。随后在一台 Apple 芯片 Mac 和一台 Intel Mac 上分别双击启动，并在应用内导入一段本地视频验证探测、缩略图与转码。

## 签名与公证

若有 Apple Developer 账号，构建完成后使用 Developer ID Application 证书签名，再用 `notarytool` 公证 DMG。没有签名时，不得声称可直接绕过 macOS Gatekeeper；用户首次打开需右键应用并选择“打开”。
