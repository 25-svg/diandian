# 典典私密更新发布器

发布器把 Windows 安装包写入私有 R2，并向后台登记一个 `draft`。它不会自动切到测试或生产，也不会把安装包放到 GitHub Release。

## 首次配置

在 Windows 当前用户下运行：

```powershell
powershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File .\distribution\publisher\Set-PublisherCredential.ps1
```

按提示输入单桶 R2 S3 endpoint、bucket、region、access key，以及只允许 publisher Access 应用的 service token 和 publisher API origin。值通过 Windows DPAPI 加密保存在当前用户目录；不要把值写进仓库、截图、聊天或命令行参数。换凭据时必须先明确移除旧的加密文件，再重新录入。

## 本地验证与构建

```powershell
npm --prefix distribution/publisher install
npm --prefix distribution/publisher test
npm --prefix distribution/publisher run build
```

## 发布流程

准备同一版本的安装器 `.exe`、Tauri `.sig` 和 UTF-8 发布说明 `.txt`，然后双击 `发布点点更新.cmd`。脚本交互读取文件路径，不接受 secret 命令行参数。

成功只代表：对象上传完成、后台 draft 建立。之后在管理后台按 `draft → testing → production` 操作；先让测试设备完成激活、下载、空闲安装和重启验证，再由 owner 全量放行。失败时先检查后台 draft 列表，避免网络响应丢失后重复发布同一版本。
