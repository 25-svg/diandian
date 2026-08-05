# 抖音罗盘（直播大屏）— 接口接入准备

> 密钥勿提交 git。本文档对应罗盘页面：**直播大屏 · 专业版**

## 相关文档

- **整场 XLSX 导入（当前主路径）：** [`live-dashboard-xlsx-field-map.md`](live-dashboard-xlsx-field-map.md) — 官方大屏 KPI ↔ Excel 字段对照
- **订单 API · 按 pay_time 对齐直播：** [`order-searchList-pay-time-guide.md`](order-searchList-pay-time-guide.md)
- SDK 排查：[`doudian-sdk-inventory.md`](doudian-sdk-inventory.md)
- **录播 ↔ 大屏绑定：** [`../../superpowers/specs/2026-07-29-live-dashboard-record-binding-design.md`](../../superpowers/specs/2026-07-29-live-dashboard-record-binding-design.md)

- [ ] 抖店/罗盘 **官方 OpenAPI**
- [ ] 公司 **自建网关**（转发罗盘后端）
- [ ] 其他：________

| 项 | 值 |
|----|-----|
| Base URL | |
| 鉴权 | AppKey+Secret / OAuth / Cookie |
| 店铺 shop_id | |
| 是否需要 VPN | |

## 2. 罗盘页面 → 接口对照（请填实际路径）

| 罗盘 UI 区域 | 接口路径/方法 | 样例文件 |
|--------------|---------------|----------|
| 顶部 KPI（成交金额、GPM、投放消耗…） | | `sample-response/compass-session-summary.json` |
| 综合趋势 · 成交金额曲线 | | `sample-response/compass-minute-gmv.json` |
| 综合趋势 · 在线人数（如有） | | |
| **讲解** 事件轨（黄点） | | `sample-response/compass-explain-events.json` |
| **投放** 事件轨 | | `sample-response/compass-ad-events.json` |
| **主播** 事件轨 | | `sample-response/compass-host-events.json` |
| 流量分析 Tab | | |
| 违规情况 Tab | | |
| 评论列表（若 API 有） | | |

## 3. 场次标识（绑定本地录播用）

截图示例场次：

- 店铺：金典拍拍相机专卖店
- 开播：2026-03-27 08:26
- 时长：04:39:26
- 主播：罗雨欣

请填写接口中的对应字段：

| 字段含义 | API 字段名 | 示例值 |
|----------|------------|--------|
| 罗盘场次 ID | | |
| 直播间 ID | | |
| 开播时间 | | |
| 关播时间 / 时长 | | |

本地系统字段（勿改）：

- `room_id` — 录制时抖音房间号
- `live_id` — 本软件内部场次 ID

## 4. 分钟趋势字段映射

| 罗盘展示 | API 字段 | 单位 |
|----------|----------|------|
| 成交金额 | | 元 |
| 在线人数 | | 人 |
| 时间戳 | | Unix / ISO / 相对开播秒 |

## 5. 讲解事件字段映射（重要）

| 需要 | API 字段 | 说明 |
|------|----------|------|
| 开始时间 | | |
| 结束时间 | | |
| 商品 ID | | |
| 商品名称 | | |
| 链接号（如 56 号） | | |

## 6. curl 测试（Secret 打码）

```bash
# 粘贴公司提供的测试命令
```
