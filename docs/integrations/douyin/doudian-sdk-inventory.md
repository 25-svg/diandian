# 抖店 SDK vs 抖音罗盘 — 排查结论

> 排查对象：`d:\Desktop\doudian-sdk-python-1.1.0-20260724091610`  
> 日期：2026-07-27  

## 结论（重要）

**这个包是「抖店开放平台 SDK」，不是「抖音罗盘 SDK」。**

全包检索结果：

| 关键词 | 匹配数 |
|--------|--------|
| 罗盘 / compass / 直播大屏 | **0** |
| live_id / room_id（直播场次） | **0**（除 buyin 的 author_ids） |
| 分钟趋势 / GMV 曲线 / 讲解事件 | **0** |

SDK 版本：`doudian-sdk-python-1.1.0`（约 **309** 个 API 模块）

## 这个 SDK 里有什么（模块分类）

| 模块前缀 | 用途 | 与罗盘关系 |
|----------|------|------------|
| `order/*` | 订单查询、发货、结算 | 可**间接**用于成交验证 |
| `product/*` | 商品上下架、库存 | 商品信息补全 |
| `afterSale/*` | 售后 | 弱相关 |
| `logistics/*` | 物流 | 无关 |
| `buyin/*` | 精选联盟、达人、自播作者 | 主播列表 |
| `shop/*` | 店铺体验分等 | 弱相关 |
| `material/*` | 素材中心 | 无关 |
| `coupons/*` | 优惠券 | 无关 |
| `instantShopping/*` | 即时零售 | 无关 |

**没有**：内容分析、直播间明细、直播大屏、综合趋势、讲解/投放/主播事件轨。

## 罗盘大屏需要的数据 → 不在本 SDK

对照你截图「直播大屏·专业版」：

| 罗盘 UI | 本 SDK 是否有 |
|---------|---------------|
| 直播间成交金额 / GPM / 投放消耗 | ❌ |
| 综合趋势 · 成交金额分钟曲线 | ❌ |
| 讲解 / 投放 / 主播 事件轨 | ❌ |
| 流量分析 / 违规 Tab | ❌ |
| 评论流 | ❌（用本地 danmu.txt） |

## 本 SDK 里仍能用的接口（辅助，不能替代罗盘）

### 1. 订单 — 验证片段窗内是否成交

```
/order/searchList     按 create_time_start / create_time_end 拉一场直播时段订单
/order/orderDetail    单笔订单详情（SKU、金额、支付时间）
```

用法：用开播~关播时间窗拉订单 → 按分钟聚合 GMV → **近似**罗盘成交曲线（缺投放、在线、讲解段）。

### 2. 自播作者 — 对应「主播轨」

```
/buyin/queryShopSelfAuthors   店铺自播达人列表
```

### 3. 鉴权（所有抖店 API 共用）

```python
from doudian.core.AccessTokenBuilder import AccessTokenBuilder
from doudian.core.DoudianOpConfig import GlobalConfig

GlobalConfig.appKey = "..."
GlobalConfig.appSecret = "..."
token = AccessTokenBuilder.buildTokenByShopId(shop_id)
```

## 罗盘数据可能在哪里

需要向公司确认是否还有**另一套**文档/SDK：

| 来源 | 说明 | 能否覆盖罗盘大屏 |
|------|------|------------------|
| **A. 抖店开放平台 · 数据/内容分析类 API** | 可能在 open.douyin.com 单独申请，未打进此 SDK | 部分（看申请权限） |
| **B. 抖音开放平台 · 直播数据** | `open.douyin.com/room/data/*`，T+1，偏达人公开直播 | 部分，**无电商 GMV 分钟曲线** |
| **C. 罗盘/电商数据 ISV 专用网关** | 公司内部转发罗盘后端 | ✅ 最可能 |
| **D. 千川/广告 API** | 单独体系 | 仅投放轨 |

## 下一步

1. 问公司：**「直播大屏的数据接口是否在另一个 SDK 或文档？」**
2. 若只有本 SDK：Phase 1 先用 `order/searchList` 做**订单时间轴**，罗盘分钟曲线等公司补文档
3. 若拿到罗盘专用 OpenAPI：按 `2026-07-27-douyin-compass-integration-codex-handoff.md` 实施

## 快速验证命令（本地）

在 SDK 目录搜索确认：

```powershell
# 应全部为 0
Select-String -Path "sdk-python\doudian\api\**\*.py" -Pattern "compass|罗盘|live_room|直播大屏" -Recurse
```
