# 录播场次 ↔ 直播数据大屏 绑定设计

## 目标

当用户打开某场 **录播**（Archive / ArchiveAnalysis）时，自动或手动关联已导入的 **直播数据大屏** 场次（来自官方 XLSX），使：

- 同一场直播的 **视频/逐字稿** 与 **经营 KPI、渠道、商品榜** 在同一上下文展示；
- 为 Phase 2「按订单支付时间在视频中找成交话术」提供 **场次级时间原点**（`started_at`）。

日常流程：罗盘点下载 XLSX → 软件自动导入大屏 → 打开对应录播分析 → **无需再选手动文件** 即可看到该场大屏数据。

## 范围

本期新增：

- 录播 `live_id` 与 `live_dashboard_sessions.id` 的绑定表及 CRUD。
- 按 **开播时间 + 账号** 的自动匹配服务（带歧义时不自动绑）。
- Tauri 命令：`resolve_live_dashboard_for_record`、`bind_live_dashboard_session`。
- ArchiveAnalysis 顶部只读 KPI 条（数据来自已绑定大屏，非伪造）。
- 录播列表或分析页「手动绑定大屏场次」下拉兜底。
- 直播数据大屏页「查看对应录播」反向跳转（有绑定时）。

本期不做：

- 分钟级成交曲线、讲解/投放事件轨（XLSX 无源）。
- 订单 API 拉取与支付时间轴（见 Phase 2 文档规划）。
- 修改录播主键 `live_id` 或 XLSX 导入幂等键。

## 两套场次标识

| 维度 | 录播（`records`） | 大屏（`live_dashboard_sessions`） |
|------|-------------------|-----------------------------------|
| 平台 | `platform`（如 `douyin`） | 隐含抖音（XLSX 来源） |
| 房间/账号 | `room_id`（字符串） | `account_key`（`基本信息.抖音号or火山号`） |
| 内部场次 ID | `live_id` | `id`（SQLite 自增） |
| 开播时间 | `created_at`（录制开始，ISO/local） | `started_at`（XLSX 直播时间起点，`YYYY-MM-DDTHH:MM:SS`） |
| 关播/时长 | `length`（秒） | XLSX `直播时间` 终点或 `直播时长` |
| 店铺展示名 | `title` 等 | `shop_name`（`达人昵称`） |

**绑定键（业务）：** 同一场直播 = 时间接近 + 账号/店铺一致。

## 匹配策略

按顺序尝试；**歧义时必须停止自动绑定**。

### 策略 A — 时间 + 账号（优先自动）

条件同时满足：

1. `platform = 'douyin'`（本期仅抖音；其他平台返回空）。
2. `|parse(record.created_at) - parse(session.started_at)| <= 15 分钟`。
3. `record.room_id == session.account_key`（字符串相等，去空格）。

唯一命中 → `match_method = auto_account` → 写入绑定表。

### 策略 B — 时间 + 店铺名（账号不一致时）

当 A 的账号不等但时间窗内仅 **1 场** 大屏且：

- `shop_name` 与 `record.title` 互相包含（忽略空格），或
- 用户曾在同 `room_id` 上绑定过同 `shop_name` 的历史场次（可选增强），

→ `match_method = auto_shop`。**若时间窗内多场 → 不自动绑。**

### 策略 C — 人工绑定（兜底）

UI 展示时间窗 ±2 小时内的候选 `live_dashboard_sessions`，用户选择 → `match_method = manual`。

### 不匹配的情况

- 无导入 XLSX / 无候选场次 → 提示「请先导入官方整场数据或手动绑定」。
- 时间差 > 15 分钟且无手动选择 → 不绑定。

## 数据模型

```sql
CREATE TABLE live_dashboard_bindings (
  live_id TEXT NOT NULL PRIMARY KEY,
  session_id INTEGER NOT NULL REFERENCES live_dashboard_sessions(id) ON DELETE CASCADE,
  match_method TEXT NOT NULL CHECK(match_method IN ('auto_account','auto_shop','manual')),
  bound_at TEXT NOT NULL
);

CREATE INDEX idx_live_dashboard_bindings_session ON live_dashboard_bindings(session_id);
```

规则：

- 一个 `live_id` 最多绑定一个大屏场次。
- 重新手动绑定 → `UPDATE` 覆盖；自动匹配 **不覆盖** 已有 `manual` 绑定。
- 删除大屏场次 → 绑定 CASCADE 删除。

## 服务接口（Rust）

```rust
pub struct LiveDashboardMatchCandidate {
    pub session: LiveDashboardSessionRow,
    pub time_delta_seconds: i64,
    pub account_match: bool,
    pub shop_name_match: bool,
}

pub struct LiveDashboardResolveResult {
    pub bound: Option<LiveDashboardSessionRow>,
    pub candidates: Vec<LiveDashboardMatchCandidate>,
    pub auto_bind_attempted: bool,
}

// 查询：有 binding 直接返回；否则跑 A/B，唯一则写入 binding
resolve_live_dashboard_for_record(platform, room_id, live_id) -> LiveDashboardResolveResult

// 强制绑定（分析页下拉）
bind_live_dashboard_session(live_id, session_id) -> LiveDashboardSessionRow

// 可选：解除绑定
unbind_live_dashboard_session(live_id)
```

时间窗常量：`AUTO_MATCH_WINDOW_SECS = 15 * 60`；候选列表窗：`CANDIDATE_WINDOW_SECS = 2 * 3600`。

## 前端

### ArchiveAnalysis

- 进入页时 `invoke('resolve_live_dashboard_for_record', { platform, roomId, liveId })`。
- **已绑定：** 顶部展示 `dashboardMetricCards`（与大屏页同源），副文案「数据来自官方 XLSX · {started_at} · {source_file}」。
- **未绑定有多候选：** 下拉「绑定直播数据大屏场次」+ 各候选的时间差。
- **无候选：** 提示导入 XLSX + 链接设置下载目录。

### Archive 录播列表（可选 P1）

- 已绑定：角标「已关联大屏」。

### LiveDataDashboard

- 场次详情增加「打开对应录播分析」（有 `live_dashboard_bindings` 反向查 `live_id` 时）。

## 与 Phase 2（订单支付时间）的衔接

绑定成功后，分析页可用：

```
video_offset_sec = order_pay_time - session.started_at
```

订单 API 拉取窗口：`session.started_at` ~ XLSX 直播结束时间。  
**本设计只保证 `started_at` 与 `live_id` 对齐；订单接入为下一 spec。**

## 错误处理

- `created_at` / `started_at` 解析失败 → 不自动绑，仅 manual。
- 同一 15 分钟窗内多场且账号相同 → 不自动绑，列候选。
- 大屏未导入 → 空结果，不报错阻塞分析页其他功能。

## 测试与验收

- 单元：`match_sessions(record, sessions)` — 时间差、账号、歧义。
- 集成：fixture 大屏 `2026-07-28T08:15:49` + mock record `08:16` 同 room_id → 自动绑定。
- 集成：两场同时落在 15 分钟窗 → 0 自动绑定，2 candidates。
- 手动：下拉绑定后 persist，刷新分析页 KPI 仍在。
- 手动：7/28 真实录播 + 已导入 XLSX → 分析页顶部 KPI 与 LiveDataDashboard 一致。

## 相关文档

- XLSX 字段与大屏 KPI：[`docs/integrations/douyin/live-dashboard-xlsx-field-map.md`](../../integrations/douyin/live-dashboard-xlsx-field-map.md)
- 大屏导入设计：[`2026-07-29-live-data-dashboard-import-design.md`](2026-07-29-live-data-dashboard-import-design.md)
- 实施 handoff：[`../plans/2026-07-29-live-dashboard-record-binding-codex-handoff.md`](../plans/2026-07-29-live-dashboard-record-binding-codex-handoff.md)
