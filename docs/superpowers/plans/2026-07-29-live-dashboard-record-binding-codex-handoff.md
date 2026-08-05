# Codex 任务：录播 ↔ 直播数据大屏 时间绑定

> **设计：** `docs/superpowers/specs/2026-07-29-live-dashboard-record-binding-design.md`  
> **前置：** 直播数据大屏 XLSX 导入（Task 1–5）已可用  
> **Fixture 大屏：** `src-tauri/tests/fixtures/live-dashboard/valid-live-dashboard.xlsx`  
> **样本：** `started_at=2026-07-28T08:15:49`，`account_key=20296833869`，`shop_name=金典拍拍相机专卖店`

---

## Goal

打开 ArchiveAnalysis 时，按 **录播开始时间 + 抖音号** 自动关联对应 `live_dashboard_session`；歧义时手动下拉绑定；顶部展示该场 XLSX KPI（只读）。

---

## Global Constraints

- 仅 `platform=douyin` 参与自动匹配。
- 自动匹配窗：**15 分钟**；候选列表窗：**±2 小时**。
- **歧义不自动绑**（同窗多场）。
- 已有 `manual` 绑定不被 auto 覆盖。
- KPI 来自 XLSX；禁止伪造「直播间成交金额」。
- 不修改 `live_id` / XLSX 幂等键 `account_key+started_at`。

---

### Task 1: 绑定表与匹配领域逻辑

**Files:**
- Create: `src-tauri/src/live_dashboard_binding.rs`（或 `src-tauri/src/database/live_dashboard_binding.rs`）
- Modify: `src-tauri/src/database/live_dashboard.rs`
- Modify: `src-tauri/src/database/mod.rs`
- Modify: `src-tauri/src/main.rs`
- Test: `live_dashboard_binding` / `database` tests

**Interfaces:**
- `match_dashboard_sessions(record, sessions) -> (Option<session_id>, Vec<Candidate>)`
- `AUTO_MATCH_WINDOW_SECS = 900`

- [ ] **Step 1: Failing tests**

```rust
#[test]
fn auto_binds_when_time_and_account_match() {
    let record = record_at("20296833869", "2026-07-28T08:16:00");
    let sessions = vec![session_at("20296833869", "2026-07-28T08:15:49")];
    let (bound, candidates) = match_dashboard_sessions(&record, &sessions);
    assert_eq!(bound.map(|s| s.started_at), Some("2026-07-28T08:15:49".into()));
    assert!(candidates.is_empty());
}

#[test]
fn does_not_auto_bind_when_multiple_sessions_in_window() {
    let record = record_at("20296833869", "2026-07-28T08:16:00");
    let sessions = vec![
        session_at("20296833869", "2026-07-28T08:15:49"),
        session_at("20296833869", "2026-07-28T08:20:00"),
    ];
    assert!(match_dashboard_sessions(&record, &sessions).0.is_none());
}
```

- [ ] **Step 2–4:** Implement migration `live_dashboard_bindings`, matching, tests green.

- [ ] **Step 5: Commit** `feat: add live dashboard session matching`

---

### Task 2: Tauri commands + resolve 服务

**Files:**
- Create: `src-tauri/src/handlers/live_dashboard_binding.rs`
- Modify: `src-tauri/src/handlers/mod.rs`, `main.rs`

**Commands:**
- `resolve_live_dashboard_for_record(platform, room_id, live_id)`
- `bind_live_dashboard_session(live_id, session_id)`
- `unbind_live_dashboard_session(live_id)`（可选）

- [ ] **Step 1:** Failing handler test — existing binding returns without re-match.
- [ ] **Step 2:** Implement resolve: read binding → else list sessions in candidate window → auto if unique → else return candidates.
- [ ] **Step 3:** `bind` upserts with `match_method=manual`.
- [ ] **Step 4:** Tests green; commit `feat: expose live dashboard binding commands`

---

### Task 3: ArchiveAnalysis UI

**Files:**
- Modify: `src/page/ArchiveAnalysis.svelte`
- Modify: `src/lib/liveDashboard.ts`（复用 `dashboardMetricCards`）
- Optional: `src/lib/liveDashboardBinding.ts` + test

- [ ] On mount / when `liveId` set: call `resolve_live_dashboard_for_record`.
- [ ] **Bound:** render KPI row + link「在直播数据大屏中打开」.
- [ ] **Candidates:** `<select>` + button「绑定此场次」→ `bind_live_dashboard_session` → refresh.
- [ ] **Empty:** 提示「导入官方整场 XLSX 后可自动关联」.
- [ ] Commit `feat: show bound live dashboard KPIs in archive analysis`

---

### Task 4: 反向跳转（可选）

**Files:**
- Modify: `src/page/LiveDataDashboard.svelte`

- [ ] If binding exists for selected session, show「打开录播分析」→ dispatch/navigate with `live_id`.

---

### Task 5: 验收

- [ ] `cargo test` binding + handler tests
- [ ] Manual: import fixture XLSX, open 7/28 douyin record analysis, KPI matches LiveDataDashboard
- [ ] Record result in design spec `## Verification`（无密钥）

---

## 复制给 Codex 的提示词

```markdown
请实现「录播 ↔ 直播数据大屏」时间绑定。

阅读：
- docs/superpowers/specs/2026-07-29-live-dashboard-record-binding-design.md
- docs/superpowers/plans/2026-07-29-live-dashboard-record-binding-codex-handoff.md
- docs/integrations/douyin/live-dashboard-xlsx-field-map.md

按 Task 1→5：
1. live_dashboard_bindings 表 + 15分钟自动匹配（账号优先，歧义不自动绑）
2. resolve_live_dashboard_for_record / bind_live_dashboard_session
3. ArchiveAnalysis 顶部展示已绑定场次的 dashboardMetricCards；未绑定显示候选下拉
4. 测试：2026-07-28 08:15:49 大屏 + 08:16 录播同 room_id → 自动绑定

禁止伪造 XLSX 没有的指标。完成后列出改动文件与手动验收步骤。
```
