# 典典私密授权与更新后台

这套 Cloudflare Worker 只服务 6–20 台已授权电脑。安装包存放在私有 GitHub Releases；管理员后台和设备更新接口使用不同 HTTPS 域名。软件不从公开 GitHub 下载更新。

## 本地验证

在仓库根目录运行：

```powershell
npm --prefix distribution/control-plane install
npm --prefix distribution/control-plane test
npm --prefix distribution/control-plane run build
```

`build` 只做本地 bundle 和 Wrangler dry-run，不会部署。

## Cloudflare 配置

1. 创建一个 D1 database，把两个 Wrangler 配置中的 database id 换成实际资源；依次应用 `0001`、`0002`、`0003` 迁移。
2. 更新 Worker 与管理 Worker 使用不同 HTTPS 域名。管理后台由应用内账号和 12 小时会话保护，不需要 Cloudflare Zero Trust，也不要求绑定银行卡。
3. 管理 Worker 配置普通变量 `UPDATE_API_BASE_URL`，并用 Wrangler secret 保存 `GITHUB_RELEASES_TOKEN`、`PUBLISHER_API_TOKEN`。发布流水线使用同一 `PUBLISHER_API_TOKEN` 作为 Bearer 凭证。
4. 更新 Worker 用 Wrangler secret 保存 `LEASE_PRIVATE_JWK` 和只读 `GITHUB_RELEASES_TOKEN`。私钥和令牌不得写进 JSON、README、命令历史或客户端。
5. 首位 owner 的一次性密码由部署脚本随机生成并写入 D1，首次登录必须改密；不得发布固定默认密码。

后台两级权限：`owner` 可管理管理员和发布状态，`operator` 只做日常设备、激活码和下载链接操作。至少保留一个 owner。授权 lease 允许离线 7 天；撤销、过期或时钟回拨都不能继续离线使用。

直播切片业务账号使用独立的 `app_users` 和 `app_user_sessions`，不得与上述发行后台管理员混用。业务登录接口位于 `/v1/app-auth/*`；发行后台 owner 通过 `/api/admin/app-users` 创建、停用或重置业务账号。所有新账号只能使用一次性初始口令进入强制改密页，完成改密前不能进入工作台。桌面构建必须设置 `VITE_APP_AUTH_API_BASE_URL` 为更新 Worker 的 HTTPS origin。

发布顺序固定为：上传后生成 draft → 指定测试机验证 testing → owner 人工提升为 production。不得跳过测试组，也不要在 Task 12 生产验收前执行远程迁移或 deploy。

## 无凭据冒烟检查

部署后可发一组无凭据、无副作用的负向请求：管理员外部访问、无 Bearer 更新元数据、续租、伪造下载票据都必须被拒绝；公开激活入口只发送空 JSON，并要求它不能成功。

```powershell
node distribution/control-plane/scripts/smoke.mjs --admin-origin https://admin.example.com/ --update-origin https://updates.example.com/
```

脚本只输出探针名、URL 和 HTTP 状态，不解析或打印响应体、响应头，不接受 token、私钥或其他 secret 参数。任何意外 2xx 或不在安全 allowlist 中的状态都会以 exit 1 失败。
