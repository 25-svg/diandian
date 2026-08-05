# 抖店 order.searchList — 按支付时间对齐直播场次

> 目标：拿到 `{pay_time, pay_amount, product_name}`，计算 `offset_sec = pay_time - live_started_at`，对接录播 SRT / LLM 证据包。  
> 脚本：`scripts/doudian_fetch_orders.py` → `scripts/doudian_order_events.py`

---

## 1. 官方 API 限制（必读）

`order.searchList` **没有** `pay_time_start` / `pay_time_end` 参数。

SDK `OrderSearchListParam` 仅支持：

| 参数 | 含义 |
|------|------|
| `create_time_start` / `create_time_end` | **下单时间**（创建时间） |
| `update_time_start` / `update_time_end` | 订单更新时间 |
| `combine_status.order_status` | 状态组合，如 `"2"` = 已支付待发货 |
| `page` / `size` | 分页，`size` ≤ 100 |

响应里每条订单/SKU 才有 **`pay_time`**（秒级 Unix 时间戳）。

因此正确做法是 **两步**：

```
① API 用 create_time 拉「宽窗口」
② 本地按 pay_time 筛「直播窗口」
```

---

## 2. 为什么不能只用 create_time = 直播时段？

| 情况 | create_time 在直播内？ | pay_time 在直播内？ |
|------|------------------------|---------------------|
| 直播中下单一并支付 | ✅ | ✅ |
| 直播前加购、直播中支付 | ❌ 可能更早 | ✅ |
| 直播中下单、直播后支付 | ✅ | ❌ 可能更晚 |

你桌面那份 `order-searchList-20260729-135146.json` 用 **14 天下单时间** 拉单，再按 7/28 直播窗筛 pay_time，相机店只剩 **5 笔**——不是 API 坏了，而是 **大部分订单的 pay_time 不在那场直播里**。

要对齐 XLSX 整场 GMV（如 7/28 ¥236,552），必须 **针对该场直播的 pay_time 窗口重新拉单**。

---

## 3. 推荐参数（按 pay_time 窗口）

### 3.1 时间从哪里来

| 来源 | 字段 | 示例 |
|------|------|------|
| 罗盘 XLSX · 基本信息 | 开播～关播 | `2026/07/28 08:15:49-2026/07/28 15:44:39` |
| 录播绑定 | `started_at` | `2026-07-28T08:15:49` |
| 本软件 DB | `live_dashboard_sessions.started_at` / `ended_at` | 同上 |

**pay_time 过滤窗口** = XLSX 开播 ~ 关播（或关播 + 少量缓冲，如 30 分钟）。

### 3.2 create_time 宽窗口（默认）

脚本自动计算：

```
create_time_start = pay_time_start - 3天   (--create-buffer-before, 默认 259200)
create_time_end   = pay_time_end   + 1天   (--create-buffer-after,  默认 86400)
```

若仍漏单，可加大 `--create-buffer-before`（例如 7 天 = `604800`）。

### 3.3 combine_status

| 场景 | 建议 |
|------|------|
| **按下单时间对齐直播（推荐）** | `--placed-in-live`，自动 `order_status=2,3,5` |
| 只拉待发货 | `--order-status 2` |
| 按 pay_time 对齐 SRT | 默认宽 create_time + 本地 pay_time 筛 |

旧版桌面 JSON 用了 **`order_status=2`  alone**，7/29 拉单时已发货(3)/已完成(5) 的单会被 API 排除，容易漏单。重拉时用 **`--placed-in-live`** 即可。

---

## 3.4 按下单时间重拉（对齐罗盘场次）

罗盘「成交件数」更接近 **直播时段内下单** 的订单，而不是 `pay_time` 落在关播前。

```powershell
.\scripts\doudian_fetch_orders.ps1 `
  -LiveStartedAt "2026/07/28 08:15:49" `
  -LiveEndedAt "2026/07/28 15:44:39" `
  -PlacedInLive `
  -ShopId "212709966" `
  -Output "d:\Desktop\order-searchList-camera-20260728-placed.json"
```

脚本行为：

- API `create_time` = 开播 ~ 关播 + 1 天缓冲（`--create-buffer-after`）
- `combine_status.order_status = "2,3,5"`（已支付 / 已发货 / 已完成）
- 本地再筛 `create_time` 在 `[开播, 关播]` 内的订单（可用 `--no-create-window-filter` 关闭）

---

## 4. 一键命令

### 前置：Token

```powershell
python scripts\doudian_get_token.py --mode code --code "<授权code>"
# 或 --mode self --shop-id "212709966"   # 金典拍拍相机专卖店
# 输出: d:\Desktop\doudian_token.env
```

**每个店铺单独授权、单独拉单。** 7/28 XLSX 是「相机专卖店」，不要用科创店 token。

### 4.1 拉单（7/28 相机场）

```powershell
python scripts\doudian_fetch_orders.py `
  --token-file "d:\Desktop\doudian_token.env" `
  --live-started-at "2026/07/28 08:15:49" `
  --live-ended-at "2026/07/28 15:44:39" `
  --output "d:\Desktop\order-searchList-camera-20260728.json"
```

等价 create_time 窗口（默认 buffer）：

| | 本地时间 |
|--|----------|
| pay_time 筛 | 2026-07-28 08:15:49 ~ 15:44:39 |
| create_time 拉 | 2026-07-25 08:15:49 ~ 2026-07-29 15:44:39 |

输出：

- `order-searchList-camera-20260728.json` — 全量（已按 pay_time 过滤）
- `order-searchList-camera-20260728-summary.json` — 摘要

### 4.2 转成支付事件

```powershell
python scripts\doudian_order_events.py `
  --orders "d:\Desktop\order-searchList-camera-20260728.json" `
  --shop-name "金典拍拍相机专卖店" `
  --started-at "2026/07/28 08:15:49" `
  --pay-time-start "2026/07/28 08:15:49" `
  --pay-time-end "2026/07/28 15:44:39" `
  --output "d:\Desktop\payment-events-camera-20260728-v2.json"
```

### 4.3 7/29 场次

```powershell
python scripts\doudian_fetch_orders.py `
  --live-started-at "2026-07-29 08:00:00" `
  --live-ended-at "2026-07-29 15:00:00" `
  --output "d:\Desktop\order-searchList-camera-20260729.json"
```

关播时间按实际 XLSX/录播填写；上面仅为示例。

---

## 5. 参数对照表

### doudian_fetch_orders.py

| 参数 | 说明 |
|------|------|
| `--live-started-at` | pay_time 下限（兼作 create 窗口基准） |
| `--live-ended-at` | pay_time 上限 |
| `--pay-time-start/end` | 与 live-* 相同，可单独指定 |
| `--create-time-start/end` | 完全手动 create 窗口（覆盖自动计算） |
| `--create-buffer-before` | pay 开始前缓冲秒数，默认 259200（3 天） |
| `--create-buffer-after` | pay 结束后缓冲秒数，默认 86400（1 天） |
| `--order-status` | 默认 `2` |
| `--keep-unpaid` | 不按 pay_time 过滤，保留 create 查询全部结果 |
| `--page-size` | 默认 50，最大 100 |

### doudian_order_events.py

| 参数 | 说明 |
|------|------|
| `--started-at` | 计算 `offset_sec` 的直播开播时间 |
| `--pay-time-start/end` | 可选，再次按 pay_time 绝对时间过滤 |
| `--in-window N` | 可选，`0 <= offset_sec <= N` |
| `--shop-name` / `--shop-id` | 多店 JSON 时按店过滤 |

---

## 6. 与 XLSX KPI 核对

拉单并生成 events 后，对比：

| KPI | XLSX 来源 | 订单 API 算法 |
|-----|-----------|---------------|
| 用户支付金额 | `基本信息.用户支付金额` | `sum(pay_amount_yuan)`，直播窗内 SKU |
| 成交件数 | `基本信息.成交件数`（优先） | SKU 行数或 `item_num` 之和 |
| GPM | `基本信息.千次观看用户支付金额` | **用 XLSX 原值**，不要用订单重算 |

若订单合计仍远低于 XLSX：

1. 确认 token 是 **相机店** 不是科创店  
2. 加大 `--create-buffer-before`  
3. 检查是否有合并支付、子单在 `sku_order_list` 多行  
4. 部分成交可能走其它店铺主体（多店 ERP 需分 shop 拉）

---

## 7. 安全与存储

- 勿提交：`.env`、`doudian_token.env`、订单 JSON（含 PII）
- Token 过期用 `doudian_get_token.py --mode refresh`
- 文档中心：https://op.jinritemai.com/docs/center → 搜 `order.searchList`

---

## 8. 后续接入 Tauri（Phase 2 概要）

1. 录播/XLSX 绑定 → 得到 `started_at` / `ended_at` / `shop_id`  
2. 后台调 `order.searchList`（Rust HTTP 或 sidecar Python）  
3. 写 `payment_events` 表  
4. ArchiveAnalysis：支付时间轴 + SRT 截取 + LLM 证据包  

Parser 与 `doudian_order_events.py` 字段保持一致即可复用。
