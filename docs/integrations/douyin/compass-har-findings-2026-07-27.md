# 抖音罗盘商品讲解数据抓取结论

> 来源：用户在已登录的抖音罗盘直播大屏中导出的 HAR。
> 本文只记录接口路径、业务参数和字段结构，不保存 Cookie、Token、签名、浏览器指纹或账号标识。

## 结论

项目需要的“商品讲解时间节点 + 经营结果”来自罗盘网页内部接口，不是开放平台的
`/compass/getProductSaleData` 汇总接口。

核心接口：

```text
GET /compass_api/shop/live/live_screen/product_explain_analysis
```

业务参数：

| 参数 | 含义 |
| --- | --- |
| `room_id` | 罗盘直播场次/房间标识 |
| `product_id` | 商品标识 |
| `role` | 当前抓取为 `shop` |
| `page_no` | 页码 |
| `page_size` | 每页条数 |

不要持久化或手工构造 `verifyFp`、`fp`、`msToken`、`a_bogus` 等浏览器会话参数。

## 讲解效果字段

响应的 `data.data_result[]` 每条记录对应一次商品讲解：

| 字段 | 含义 | 单位/转换 |
| --- | --- | --- |
| `explain_start_ts` | 讲解开始时间 | Unix 秒 |
| `explain_end_ts` | 讲解结束时间 | Unix 秒 |
| `explain_time` | 页面展示的起止时间 | 文本 |
| `explain_duration.value` | 讲解时长 | 秒 |
| `product_click_cnt.value` | 商品点击次数 | 次 |
| `product_pay_amt.value` | 商品用户支付金额 | 分，除以 100 显示为元 |
| `watch_cnt.value` | 直播间进入次数（推荐流） | 次 |
| `avg_online_ucnt.value` | 平均在线人数 | 人 |

抓取样本中，一条 `12:08-12:11` 的讲解记录返回：

```json
{
  "explain_duration": 188,
  "product_click_cnt": 8,
  "product_pay_amt_fen": 711500,
  "product_pay_amt_yuan": 7115,
  "watch_cnt": 22,
  "avg_online_ucnt": 30
}
```

这与罗盘页面显示的 `¥7,115` 一致，确认金额字段以“分”为单位。

## 相关接口

### 商品讲解详情

```text
GET /compass_api/shop/live/live_screen/product_explain_detail
```

业务参数：

- `room_id`
- `product_id`
- `show_feature=true`
- `without_explain_duration=false`

返回商品信息、整场经营汇总，以及按分钟排列的点击等趋势。

### 商品整体趋势

```text
GET /compass_api/shop/live/live_screen/product_overall_trend
```

业务参数：

- `room_id`
- `product_id`

响应包含 `explain_list[]`，可一次取得该商品全部讲解开始/结束时间：

- `explain_start_ts`
- `explain_end_ts`
- `explain_start_time`
- `explain_end_time`
- `product_id`

### SKU 明细

```text
GET /compass_api/shop/live/live_screen/product_sku_detail
```

业务参数：

- `room_id`
- `product_id`
- `role=shop`
- `page_no`
- `page_size`

返回 SKU 到手价、支付金额、支付件数、未支付件数等数据。

## 接入项目的正确方式

1. 用 `product_overall_trend.explain_list[]` 建立完整讲解时间窗。
2. 分页读取 `product_explain_analysis.data_result[]`，为每个时间窗补充点击、支付金额、进房和在线人数。
3. 使用 `explain_start_ts`、`explain_end_ts` 与本地录播开播时间对齐，换算为视频相对秒数。
4. 从逐字稿截取对应时间窗，并前后扩展 30-90 秒，保留客户问题、主播回答和结果上下文。
5. 把经营数据当作“结果证据”，把逐字稿和视频当作“话术证据”。
6. 先发现候选片段，再按企业母稿评分；不能因为该时段有支付金额就直接判定某一句话促成成交。

## 安全与稳定性边界

这些路径是罗盘网页内部接口，不是已确认可供第三方长期调用的开放 API：

- 不把浏览器 Cookie、Token、签名或指纹打包进客户端。
- 不在多台员工电脑上复制同一账号会话。
- 生产环境优先让公司申请正式数据接口或自建服务端网关。
- 第一版可支持“导入脱敏 HAR/官方导出文件”，用于验证时间轴和分析效果。
- 若使用内部网关，由网关统一鉴权、限流、审计和刷新会话，桌面端只接收规范化业务数据。

