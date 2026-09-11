# 典典直播切片

![icon](docs/public/images/header.png)

![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/xinrea/bili-shadowreplay/main.yml?label=Application%20Build)
![GitHub Actions Workflow Status](https://img.shields.io/github/actions/workflow/status/Xinrea/bili-shadowreplay/package.yml?label=Docker%20Build)
![GitHub Release](https://img.shields.io/github/v/release/xinrea/bili-shadowreplay)
![GitHub Downloads (all assets, all releases)](https://img.shields.io/github/downloads/xinrea/bili-shadowreplay/total)
[![Ask DeepWiki](https://deepwiki.com/badge.svg)](https://deepwiki.com/Xinrea/bili-shadowreplay)

典典直播切片是一个缓存直播并进行实时编辑投稿的工具。通过划定时间区间，并编辑简单的必需信息，即可完成直播切片以及投稿，将整个流程压缩到分钟级。同时，也支持对缓存的历史直播进行回放，以及相同的切片编辑投稿处理流程。

目前仅支持 B 站和抖音平台的直播。

<a href="https://www.star-history.com/?repos=Xinrea%2Fbili-shadowreplay&type=date&legend=top-left">
 <picture>
   <source media="(prefers-color-scheme: dark)" srcset="https://api.star-history.com/svg?repos=Xinrea/bili-shadowreplay&type=date&theme=dark&legend=top-left" />
   <source media="(prefers-color-scheme: light)" srcset="https://api.star-history.com/svg?repos=Xinrea/bili-shadowreplay&type=date&legend=top-left" />
   <img alt="Star History Chart" src="https://api.star-history.com/svg?repos=Xinrea/bili-shadowreplay&type=date&legend=top-left" />
 </picture>
</a>

## 安装和使用

![rooms](docs/public/images/summary.png)

前往网站查看说明：[典典直播切片](https://bsr.xinrea.cn/)

## 参与开发

可以通过 [DeepWiki](https://deepwiki.com/Xinrea/bili-shadowreplay) 了解本项目。

贡献指南：[Contributing](.github/CONTRIBUTING.md)

## 私密授权与自动更新

公司分发使用 Cloudflare 免费额度内的 D1、私有 R2、Workers 和 Access。安装包不公开放在 GitHub；管理员生成一次性下载链接，主播首次安装激活后，软件在空闲时自动检查、下载和安装更新。

- 授权适合 6–20 台电脑，离线最长 7 天；设备被撤销、授权过期或电脑时钟异常时会停止使用。
- 后台有 owner、operator 两级权限。owner 管管理员和版本放行，operator 管设备、激活码和下载链接。
- 更新先进入测试组，再由 owner 放到 production；软件连续空闲 2 分钟后安装，录制、任务或视频预览期间不安装。
- 私密后台说明见 [distribution/control-plane/README.md](distribution/control-plane/README.md)，Windows 发布器说明见 [distribution/publisher/README.md](distribution/publisher/README.md)。

开发者可运行 `npm run test:private-distribution` 验证授权、后台、发布器和桌面更新逻辑。正式构建前必须通过环境变量提供 `DIANDIAN_UPDATE_ENDPOINT` 与 `DIANDIAN_LICENSE_PUBLIC_KEY`；这里只记录变量名，不记录值。`npm run build:private-distribution` 会先校验两项，再构建两个 Worker、发布器和前端，并扫描客户端产物中的敏感配置名。

## 赞助

<!-- markdownlint-disable MD033 -->
<img src="docs/public/images/donate.png" alt="donate" width="300">
