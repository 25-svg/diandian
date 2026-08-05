# Codex 任务：录播身份展示 + 直播大屏跳转

> **背景：** 用户希望像罗盘直播大屏一样，**一眼知道这场是谁、哪场**（如 `金典拍拍相机专卖店 · 2026/07/28 08:15 · 罗雨欣`），并在录播/分析页一键打开对应 **直播数据大屏**。  
> **前置：** Task 1–3（绑定表 + resolve + ArchiveAnalysis KPI 条）已完成，见 `2026-07-29-live-dashboard-record-binding-codex-handoff.md`。  
> **样本场次：** `account_key=20296833869`，`shop_name=金典拍拍相机专卖店`，`started_at=2026-07-28T08:15:49`，GMV ≈ ¥236,762，51 件成交。

---

## Goal

1. **录播列表（Archive）**：绑定大屏后，主标题显示店铺/账号身份（类似罗盘顶栏），副标题显示抖音号、开播时间、GMV；未绑定则 fallback 到 `anchor_name` / 标题。
2. **录播分析（ArchiveAnalysis）**：顶部增加 **「直播大屏」** 按钮，跳转到 `LiveDataDashboard` 并预选已绑定场次。
3. **录播列表行操作**：已绑定抖音录播增加 **「大屏」** 按钮，同上跳转。
4. **直播数据大屏（LiveDataDashboard）**：支持从外部传入 `sessionId` 预选场次（反向跳转留 Task 5 可选）。

---

## 非目标（本期不做）

- 罗盘里的 **讲解 / 投放 / 主播时间轨**（XLSX 无源，见 `live-dashboard-xlsx-field-map.md`）。
- 分钟级 GMV 曲线、订单 pay_time 轴（Phase 2）。
- 修改已有绑定匹配规则（15 分钟窗、歧义不自动绑）。

---

## 身份展示规则（「一眼知道是谁」）

| 优先级 | 主标题 `primary` | 副标题 `secondary` |
|--------|------------------|-------------------|
| 已绑定大屏 | `shop_name`（罗盘达人昵称） | `account_key · started_at · GMV · N件成交 · 主播 anchor_name`（主播名与 shop 不同时追加） |
| 未绑定 | `anchor_name` 或 `title` | 「未绑定直播数据大屏」或「可在录播分析页绑定」 |

**说明：**

- `anchor_name` 来自本地 AI 识别（如 **罗雨欣**），XLSX **没有**主播轨字段；有则与 shop 并列展示，不能伪造。
- 列表页 **禁止** 对每条录播单独 `resolve`（N+1）；应 **批量查绑定** + 可选 **后台 auto-resolve 最多 15 条** 未绑定抖音录播。

---

## Global Constraints

- 仅 `platform=douyin` 显示大屏按钮 / 参与 auto-resolve。
- KPI、GMV 必须来自 `live_dashboard_sessions`，禁止推算。
- 跳转用统一事件 `bsr:open-live-dashboard`，payload `{ sessionId: number }`。
- 复用已有 `dashboardMetricCards`、`resolve_live_dashboard_for_record`、`bind_live_dashboard_session`。

---

## 可能存在的 WIP（Codex 请先 diff 再决定保留或重写）

上一轮对话可能已部分改动以下文件，**以本 handoff 为准验收**，冲突处按 spec 整理：

- `src/lib/liveDashboard.ts` — `formatArchiveDashboardIdentity` / `openLiveDashboard`
- `src/page/Archive.svelte` — 身份列、大屏按钮、批量 binding
- `src/page/ArchiveAnalysis.svelte` — 头部「直播大屏」
- `src/page/LiveDataDashboard.svelte` — `initialSessionId` prop
- `src/App.svelte` — 事件监听
- `src-tauri/.../live_dashboard_binding.rs` — `list_live_dashboard_bindings_for_live_ids`

若 WIP 不完整或与 spec 不符，Codex 应整理为最小可 review 的 PR。

---

### Task 1: 后端 — 批量查绑定

**Files:**

- Modify: `src-tauri/src/database/live_dashboard_binding.rs`
- Modify: `src-tauri/src/handlers/live_dashboard_binding.rs`
- Modify: `src-tauri/src/main.rs`
- Test: DB test — 给定 2 个 `live_id`，只返回已绑定行

**Command:**

```rust
list_live_dashboard_bindings_for_live_ids(live_ids: Vec<String>)
  -> Vec<LiveDashboardBindingSummaryRow>
```

**Row 字段（camelCase 序列化）：**

`liveId`, `sessionId`, `matchMethod`, `accountKey`, `shopName`, `startedAt`, `paymentAmountFen`, `dealItemCount`

**SQL：** `bindings JOIN sessions WHERE live_id IN (...)`

- [ ] Step 1: Failing test
- [ ] Step 2: Implement QueryBuilder IN 查询
- [ ] Step 3: Register tauri command
- [ ] Step 4: `cargo test` green

---

### Task 2: 前端工具 — 身份格式化 + 跳转

**Files:**

- Modify: `src/lib/liveDashboard.ts`
- Create/Modify: `src/lib/liveDashboard.test.ts`

**Exports:**

```typescript
formatArchiveDashboardIdentity(archive, binding?) -> { primary, secondary, hasDashboard }
formatDashboardSessionTime(startedAt) -> locale string
openLiveDashboard(sessionId: number)  // dispatch bsr:open-live-dashboard
```

- [ ] 测试：`金典拍拍相机专卖店` + `罗雨欣` + fixture KPI → primary/secondary 断言
- [ ] Commit: `feat: format archive identity from live dashboard binding`

---

### Task 3: App 路由 — 大屏预选

**Files:**

- Modify: `src/App.svelte`
- Modify: `src/page/LiveDataDashboard.svelte`

**行为：**

1. `App` 监听 `bsr:open-live-dashboard` → `active = "直播数据大屏"`，`liveDashboardSessionId = detail.sessionId`
2. `<LiveDataDashboard initialSessionId={liveDashboardSessionId} />`
3. `LiveDataDashboard` 在 `initialSessionId` 变化时调用 `refresh(sessionId)`，下拉框同步选中

- [ ] 手动：从 ArchiveAnalysis 点按钮 → 大屏页显示 7/28 金典场次 KPI

---

### Task 4: Archive 列表 — 身份 + 大屏按钮

**Files:**

- Modify: `src/page/Archive.svelte`

**UI：**

1. 列名 **「账号 / 标题」**：主行 `primary` + 徽章「已绑大屏」；副行 `secondary`；原 `title` 与 primary 不同时第三行小字展示。
2. 操作列：**「大屏」** 按钮（`BarChart3`），仅 `liveDashboardBindings.has(live_id)` 时可点。
3. `loadArchives` 完成后：
   - `list_live_dashboard_bindings_for_live_ids` 一次批量加载
   - 可选：`autoResolveLiveDashboardBindings()` — 最多 15 条未绑定 douyin 调 `resolve`（后台，不阻塞 loading）

**禁止：** 每条录播 sync 调 resolve；删除后全量 `loadArchives` 仅因 binding 更新（删除录播已有本地 remove，binding map 同步删即可）。

- [ ] 手动：导入 7/28 XLSX 后，列表出现「金典拍拍相机专卖店」+ GMV，无需先进分析页

---

### Task 5: ArchiveAnalysis — 直播大屏按钮

**Files:**

- Modify: `src/page/ArchiveAnalysis.svelte`

**UI：**

1. **Header actions**：`platform=douyin` 且已绑定 → 按钮「直播大屏」→ `openLiveDashboard(session.id)`
2. **KPI 条标题**：`shopName` 作主标题（替代泛化「已绑定直播数据大屏」）；右侧链接「打开直播大屏」

- [ ] 与 Task 3 联调通过

---

### Task 6（可选）: LiveDataDashboard 反向跳转

**Files:**

- Modify: `src-tauri/src/database/live_dashboard_binding.rs` — `get_live_id_for_session(session_id)`
- Modify: `src/page/LiveDataDashboard.svelte` — 有绑定时显示「打开录播分析」

---

### Task 7: 验收

- [ ] `cargo test` + `npx tsx src/lib/liveDashboard.test.ts`
- [ ] 导入 `valid-live-dashboard.xlsx` 或桌面 7/28 明细
- [ ] Archive 列表：身份行与罗盘顶栏信息一致（店铺名、时间、GMV）
- [ ] ArchiveAnalysis + Archive 行「大屏」→ LiveDataDashboard 同场次 KPI
- [ ] 未绑定录播：按钮 disabled，副标题提示去分析页绑定
- [ ] 更新 design spec `## Verification` 一节（无密钥）

---

## 复制给 Codex 的提示词

```markdown
请实现「录播身份展示 + 直播大屏跳转」。

阅读：
- docs/superpowers/plans/2026-07-29-live-dashboard-identity-and-navigation-codex-handoff.md
- docs/superpowers/specs/2026-07-29-live-dashboard-record-binding-design.md
- docs/integrations/douyin/live-dashboard-xlsx-field-map.md

按 Task 1→5（Task 6 可选）：
1. list_live_dashboard_bindings_for_live_ids 批量 API
2. liveDashboard.ts 身份格式化 + openLiveDashboard 事件
3. App + LiveDataDashboard 支持 sessionId 预选
4. Archive 列表展示 shop/account/GMV + 「大屏」按钮
5. ArchiveAnalysis 头部「直播大屏」

样本：20296833869 / 金典拍拍相机专卖店 / 2026-07-28 08:15:49

若仓库里已有部分 WIP diff，以 handoff 验收标准为准整理，不要重复造轮子。
禁止伪造 XLSX 没有的指标或讲解/投放时间轨。
完成后列出改动文件与手动验收步骤。
```
