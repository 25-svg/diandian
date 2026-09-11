# 典典私密授权与更新后台

这套 Cloudflare Worker 只服务 6–20 台已授权电脑。R2 桶保持私有；管理员后台和设备更新接口使用不同 HTTPS 域名。软件不从公开 GitHub 下载更新。

## 本地验证

在仓库根目录运行：

```powershell
npm --prefix distribution/control-plane install
npm --prefix distribution/control-plane test
npm --prefix distribution/control-plane run build
```

`build` 只做本地 bundle 和 Wrangler dry-run，不会部署。

## Cloudflare 配置

1. 创建一个私有 R2 bucket 和一个 D1 database，把 `wrangler.admin.jsonc`、`wrangler.update.jsonc` 中的占位名称和 database id 换成实际资源。
2. 管理后台单独绑定一个 Cloudflare Access 应用；更新 Worker 绑定另一个 HTTPS 域名。两个 origin 不能相同。
3. 管理 Worker 配置普通变量：`UPDATE_API_BASE_URL`、`ADMIN_ACCESS_ISSUER`、`ADMIN_ACCESS_AUDIENCE`、`ADMIN_ACCESS_JWKS_URL`、`PUBLISHER_ACCESS_ISSUER`、`PUBLISHER_ACCESS_AUDIENCE`、`PUBLISHER_ACCESS_JWKS_URL`。
4. 更新 Worker 用 Wrangler secret 保存 `LEASE_PRIVATE_JWK`。它是许可证签名私钥，不得写进 JSON、README、命令历史或客户端。
5. D1/R2 只通过 `DB`、`ARTIFACTS` bindings 访问。发布器使用单桶权限的 R2 S3 key 和只允许 publisher Access 应用的 service token。

后台两级权限：`owner` 可管理管理员和发布状态，`operator` 只做日常设备、激活码和下载链接操作。至少保留一个 owner。授权 lease 允许离线 7 天；撤销、过期或时钟回拨都不能继续离线使用。

发布顺序固定为：上传后生成 draft → 指定测试机验证 testing → owner 人工提升为 production。不得跳过测试组，也不要在 Task 12 生产验收前执行远程迁移或 deploy。

## 无凭据冒烟检查

部署后可发一组无凭据、无副作用的负向请求：管理员外部访问、无 Bearer 更新元数据、续租、伪造下载票据都必须被拒绝；公开激活入口只发送空 JSON，并要求它不能成功。

```powershell
node distribution/control-plane/scripts/smoke.mjs --admin-origin https://admin.example.com/ --update-origin https://updates.example.com/
```

脚本只输出探针名、URL 和 HTTP 状态，不解析或打印响应体、响应头，不接受 token、私钥或其他 secret 参数。任何意外 2xx 或不在安全 allowlist 中的状态都会以 exit 1 失败。
