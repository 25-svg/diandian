# 直播数据大屏 XLSX 自动导入 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** 导入抖店官方整场数据 XLSX，并在桌面端“直播数据大屏”自动显示场次、渠道、商品和短视频引流数据。

**Architecture:** Rust 后端用 `calamine` 解析工作簿并规范化为领域模型；SQLite 以“账号标识 + 开播时间”幂等保存场次及明细。Tauri 命令共享同一导入服务，目录轮询器和前端手动选文件都调用该服务；Svelte 页面只消费查询接口。

**Tech Stack:** Rust, Tauri 2, SQLx/SQLite, calamine, Svelte 3, TypeScript。

## Global Constraints

- 仅接收官方整场下载 XLSX；不得自动登录、抓 Cookie 或网页爬取。
- 金额、比例和时长必须规范化，不能把“直播间用户支付金额”命名为“直播间成交金额”。
- 导入必须幂等；唯一键为账号标识和开播开始时间。
- 自动监听失败时，手动导入必须继续可用。
- 不保存 SKU 子行；只保存商品汇总行。

---

### Task 1: XLSX 领域模型与解析器

**Files:**
- Modify: `src-tauri/Cargo.toml`
- Create: `src-tauri/src/live_data_import.rs`
- Modify: `src-tauri/src/main.rs`
- Test: `src-tauri/src/live_data_import.rs`

**Interfaces:**
- Produces `parse_live_dashboard_xlsx(path: &Path) -> Result<LiveDashboardImport, LiveDashboardImportError>`.
- `LiveDashboardImport` contains `session`, `channels`, `short_videos`, and `products`.

- [ ] **Step 1: Write failing parser tests with a temporary workbook fixture**

```rust
#[test]
fn parses_payment_amount_percentage_and_duration() {
    let import = parse_fixture("valid-live-dashboard.xlsx").unwrap();
    assert_eq!(import.session.payment_amount_fen, 23_655_200);
    assert_eq!(import.session.average_online, 22);
    assert_eq!(import.session.viewer_conversion_rate, Some(0.0068));
    assert_eq!(import.session.average_watch_seconds, Some(65));
}

#[test]
fn rejects_a_workbook_without_the_required_dashboard_sheets() {
    assert!(matches!(parse_fixture("missing-dashboard-sheet.xlsx"), Err(LiveDashboardImportError::MissingSheet(_))));
}
```

- [ ] **Step 2: Run the parser tests to verify they fail**

Run: `cargo test --bin bili-shadowreplay live_data_import::tests`

Expected: FAIL because `live_data_import` and parser API do not exist.

- [ ] **Step 3: Add `calamine` and implement the smallest parser**

```rust
pub fn parse_live_dashboard_xlsx(path: &Path) -> Result<LiveDashboardImport, LiveDashboardImportError> {
    let mut workbook = calamine::open_workbook_auto(path)?;
    let basic = required_sheet(&mut workbook, "基本信息")?;
    let board = required_sheet(&mut workbook, "整体看板")?;
    Ok(LiveDashboardImport {
        session: parse_session(&basic, &board)?,
        channels: parse_channels(&required_sheet(&mut workbook, "流量分析-渠道分析")?)?,
        short_videos: parse_short_videos(&required_sheet(&mut workbook, "流量分析-短视频引流")?)?,
        products: parse_product_summary(&required_sheet(&mut workbook, "商品分析-商品明细")?)?,
    })
}
```

- [ ] **Step 4: Add parser edge-case tests and verify green**

```rust
#[test]
fn ignores_sku_rows_between_repeated_product_headers() {
    let import = parse_fixture("products-with-sku-rows.xlsx").unwrap();
    assert_eq!(import.products.len(), 2);
    assert!(import.products.iter().all(|product| !product.product_id.is_empty()));
}
```

Run: `cargo test --bin bili-shadowreplay live_data_import::tests`

Expected: PASS.

- [ ] **Step 5: Commit parser work**

```bash
git add src-tauri/Cargo.toml src-tauri/Cargo.lock src-tauri/src/live_data_import.rs src-tauri/src/main.rs
git commit -m "feat: parse live dashboard xlsx exports"
```

### Task 2: 直播数据 SQLite 存储与幂等导入

**Files:**
- Create: `src-tauri/src/database/live_dashboard.rs`
- Modify: `src-tauri/src/database/mod.rs`
- Modify: `src-tauri/src/main.rs`
- Test: `src-tauri/src/database/live_dashboard.rs`

**Interfaces:**
- Consumes `LiveDashboardImport` from Task 1.
- Produces `Database::upsert_live_dashboard(import, source_file) -> Result<LiveDashboardSessionRow, DatabaseError>` and list/detail query methods.

- [ ] **Step 1: Write failing database tests**

```rust
#[tokio::test]
async fn repeated_import_replaces_details_without_creating_a_second_session() {
    let db = test_database().await;
    db.upsert_live_dashboard(import_for("2026-07-28T08:15:49", 2), "first.xlsx").await.unwrap();
    db.upsert_live_dashboard(import_for("2026-07-28T08:15:49", 3), "second.xlsx").await.unwrap();
    assert_eq!(db.list_live_dashboard_sessions().await.unwrap().len(), 1);
    assert_eq!(db.list_live_dashboard_products(1).await.unwrap().len(), 3);
}
```

- [ ] **Step 2: Run database tests to verify they fail**

Run: `cargo test --bin bili-shadowreplay database::live_dashboard::tests`

Expected: FAIL because migration SQL and database methods do not exist.

- [ ] **Step 3: Implement tables and transactional upsert**

```sql
CREATE TABLE live_dashboard_sessions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  account_key TEXT NOT NULL,
  started_at TEXT NOT NULL,
  shop_name TEXT NOT NULL DEFAULT '',
  payment_amount_fen INTEGER NOT NULL DEFAULT 0,
  source_file TEXT NOT NULL,
  imported_at TEXT NOT NULL,
  UNIQUE(account_key, started_at)
);
```

Use one SQLx transaction: upsert the session, delete old channel/short-video/product rows by `session_id`, insert parsed rows, then commit.

- [ ] **Step 4: Verify database tests pass**

Run: `cargo test --bin bili-shadowreplay database::live_dashboard::tests`

Expected: PASS.

- [ ] **Step 5: Commit storage work**

```bash
git add src-tauri/src/database/live_dashboard.rs src-tauri/src/database/mod.rs src-tauri/src/main.rs
git commit -m "feat: persist imported live dashboard data"
```

### Task 3: Tauri commands、下载目录设置与自动导入轮询

**Files:**
- Modify: `src-tauri/src/config.rs`
- Create: `src-tauri/src/handlers/live_dashboard.rs`
- Modify: `src-tauri/src/handlers/mod.rs`
- Modify: `src-tauri/src/main.rs`
- Test: `src-tauri/src/handlers/live_dashboard.rs`

**Interfaces:**
- Produces commands `import_live_dashboard_xlsx`, `list_live_dashboard_sessions`, `get_live_dashboard_detail`, `get_live_dashboard_settings`, and `set_live_dashboard_download_dir`.
- Directory poller calls the same `import_live_dashboard_xlsx_path` service as the manual command.

- [ ] **Step 1: Write failing command/service tests**

```rust
#[tokio::test]
async fn manual_and_watched_import_share_the_same_idempotent_service() {
    let state = test_state().await;
    import_live_dashboard_xlsx_path(&state, fixture_path()).await.unwrap();
    import_live_dashboard_xlsx_path(&state, fixture_path()).await.unwrap();
    assert_eq!(state.db.list_live_dashboard_sessions().await.unwrap().len(), 1);
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test --bin bili-shadowreplay handlers::live_dashboard::tests`

Expected: FAIL because the handler module and import service do not exist.

- [ ] **Step 3: Implement config and poller**

```rust
pub async fn import_live_dashboard_xlsx_path(
    state: &AppState,
    path: &Path,
) -> Result<LiveDashboardSessionRow, String> {
    let parsed = parse_live_dashboard_xlsx(path).map_err(|error| error.to_string())?;
    state.db.upsert_live_dashboard(parsed, &path.to_string_lossy()).await.map_err(Into::into)
}
```

Persist `live_dashboard_download_dir` in `Conf.toml`; default it to the platform Downloads directory. Poll every five seconds, ignore `~$` files, and only parse an `.xlsx` after the file size is unchanged in two consecutive polls. Store source file fingerprints in the import record so unchanged files are not reprocessed.

- [ ] **Step 4: Verify command/service tests pass**

Run: `cargo test --bin bili-shadowreplay handlers::live_dashboard::tests`

Expected: PASS.

- [ ] **Step 5: Commit command and watcher work**

```bash
git add src-tauri/src/config.rs src-tauri/src/handlers/live_dashboard.rs src-tauri/src/handlers/mod.rs src-tauri/src/main.rs
git commit -m "feat: auto import live dashboard downloads"
```

### Task 4: 前端直播数据大屏与手动导入兜底

**Files:**
- Create: `src/lib/liveDashboard.ts`
- Create: `src/lib/liveDashboard.test.ts`
- Create: `src/page/LiveDataDashboard.svelte`
- Modify: `src/App.svelte`
- Modify: `src/lib/components/BSidebar.svelte`

**Interfaces:**
- Consumes Task 3 Tauri commands through `invoke` and `tauri-plugin-dialog` file picker.
- Produces a route named `直播数据大屏`.

- [ ] **Step 1: Write failing view-model tests**

```ts
assert.deepEqual(
  dashboardMetricCards({ payment_amount_fen: 23655200, viewer_conversion_rate: 0.0068 }),
  [
    { label: "直播间用户支付金额", value: "¥236,552" },
    { label: "直播间观看-成交率", value: "0.68%" },
  ],
);
```

- [ ] **Step 2: Run the view-model test to verify it fails**

Run: `node --loader ts-node/esm src/lib/liveDashboard.test.ts`

Expected: FAIL because `dashboardMetricCards` does not exist.

- [ ] **Step 3: Implement the route and empty/error states**

```ts
async function importWorkbook(): Promise<void> {
  const selected = await open({ multiple: false, filters: [{ name: "Excel", extensions: ["xlsx"] }] });
  if (!selected || Array.isArray(selected)) return;
  await invoke("import_live_dashboard_xlsx", { path: selected });
  await refresh();
}
```

Show source file and imported time. Display `—` plus “官方导出未提供” for absent metrics; do not synthesize transaction amount labels.

- [ ] **Step 4: Verify front-end tests and production build**

Run: `node --loader ts-node/esm src/lib/liveDashboard.test.ts && npm run build`

Expected: PASS; Vite produces `dist/`.

- [ ] **Step 5: Commit UI work**

```bash
git add src/lib/liveDashboard.ts src/lib/liveDashboard.test.ts src/page/LiveDataDashboard.svelte src/App.svelte src/lib/components/BSidebar.svelte
git commit -m "feat: add live data dashboard"
```

### Task 6: 官方大屏 KPI 第二行对齐（UI 增强）

**Files:**
- Modify: `src-tauri/src/live_data_import.rs`（若 session 缺字段则补解析）
- Modify: `src-tauri/src/database/live_dashboard.rs`
- Modify: `src/lib/liveDashboard.ts`
- Modify: `src/lib/liveDashboard.test.ts`
- Modify: `src/page/LiveDataDashboard.svelte`

**必读：** `docs/integrations/douyin/live-dashboard-xlsx-field-map.md` 与 design spec「官方大屏 KPI 对齐」节。

**Interfaces:**
- `dashboardMetricCards()` 返回两行共 11 张 KPI（或第一行 6 + 第二行 5 分开导出）。
- 缺失 XLSX 源的数据（如「直播间成交金额」）显示 `— / 官方导出未提供`。

- [ ] **Step 1: 补解析与 DB 字段**

从 XLSX 写入 session：`buyer_count`（成交人数）、`item_count`（商品汇总行成交件数之和）、`product_click_conversion_rate`、`exposure_view_rate`、`qianchuan_spend_fen`（渠道「整体」千川消耗）。

- [ ] **Step 2: 更新 `dashboardMetricCards` 测试**

断言 fixture 场次：用户支付 ¥236,552、成交人数 46、成交件数 51、千川消耗 ¥2,201.51、商品点击-成交率 3.08%。

- [ ] **Step 3: LiveDataDashboard 两行卡片布局**

第一行 6 张 + 第二行 5 张；**不要**添加「直播间成交金额」卡片（XLSX 无源）。

- [ ] **Step 4: 跑测试**

```powershell
$env:CARGO_TARGET_DIR = "...\src-tauri\target"
cargo test --bin bili-shadowreplay live_data_import::tests database::live_dashboard::tests
node --loader ts-node/esm src/lib/liveDashboard.test.ts
```

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/live_data_import.rs src-tauri/src/database/live_dashboard.rs src/lib/liveDashboard.ts src/lib/liveDashboard.test.ts src/page/LiveDataDashboard.svelte
git commit -m "feat: align live dashboard KPIs with official compass export"
```

### Task 5: 端到端验证与回归

**Files:**
- Modify: `docs/superpowers/specs/2026-07-29-live-data-dashboard-import-design.md`

**Interfaces:**
- Consumes complete Tasks 1–4.
- Produces verified import behavior using the official supplied workbook.

- [ ] **Step 1: Run all focused Rust tests**

Run: `cargo test --bin bili-shadowreplay live_data_import::tests database::live_dashboard::tests handlers::live_dashboard::tests`

Expected: PASS.

- [ ] **Step 2: Run frontend verification**

Run: `node --loader ts-node/esm src/lib/liveDashboard.test.ts && npm run build`

Expected: PASS.

- [ ] **Step 3: Manual acceptance with the supplied official XLSX**

Run the app, set Downloads as the watched directory, copy the supplied workbook into it, and verify that one session appears. Copy it again and verify session count stays one while import status says updated.

- [ ] **Step 4: Record test result in the design document**

Add a `## Verification` section containing the executed commands, date, and result. Do not include secrets or source file contents.

- [ ] **Step 5: Commit verification record**

```bash
git add docs/superpowers/specs/2026-07-29-live-data-dashboard-import-design.md
git commit -m "docs: verify live dashboard import"
```
