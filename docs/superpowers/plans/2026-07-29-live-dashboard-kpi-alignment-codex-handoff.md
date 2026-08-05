# Codex 任务：直播数据大屏 KPI 对齐官方罗盘

> **前置：** Task 1–5 已完成（XLSX 导入 + 大屏基础页）  
> **设计：** `docs/superpowers/specs/2026-07-29-live-data-dashboard-import-design.md`  
> **字段对照：** `docs/integrations/douyin/live-dashboard-xlsx-field-map.md`  
> **Fixture：** `src-tauri/tests/fixtures/live-dashboard/valid-live-dashboard.xlsx`

---

## 目标

把「直播数据大屏」顶部 KPI 对齐官方罗盘直播大屏（图二）**在 XLSX 能提供的范围内**补全第二行指标；不伪造 XLSX 没有的「直播间成交金额」和分钟曲线。

---

## 硬性约束

1. **禁止**将 `直播间用户支付金额` 显示为 `直播间成交金额`。
2. **禁止**伪造 `直播间成交金额`、分钟成交曲线、讲解/投放事件轨。
3. XLSX 无源指标 → `— / 官方导出未提供`。
4. `成交件数` = 商品汇总行 `成交件数` 之和（**不含** SKU 子行）。
5. `千川消耗` = `流量分析-渠道分析` 中 `渠道名称=整体` 行的 `千川消耗`。
6. KPI 标签优先用 **XLSX 原字段名**（可去掉括号内「人数」仅当 UI 太挤时，但测试应用全名）。

---

## KPI 布局

### 第一行（6 张 — 保持/整理现有）

| 标签 | XLSX |
|------|------|
| 直播间用户支付金额 | 基本信息 · `直播间用户支付金额` |
| 千次观看用户支付金额 | 基本信息 · `千次观看用户支付金额` |
| 直播间观看人数 | 整体看板 · `直播间观看人数` |
| 平均在线人数 | 整体看板 · `平均在线人数` |
| 人均观看时长 | 整体看板 · `人均观看时长` |
| 直播间观看-成交率 | 整体看板 · `直播间观看-成交率(人数)` |

### 第二行（5 张 — 新增）

| 标签 | XLSX |
|------|------|
| 成交人数 | 流量分析-流量转化 · `成交人数` |
| 成交件数 | 商品明细汇总行 · Σ`成交件数` |
| 直播间商品点击-成交率 | 整体看板 · `直播间商品点击-成交率(人数)` |
| 直播间曝光-观看率(人数) | 整体看板 · `直播间曝光-观看率(人数)` |
| 千川消耗 | 渠道分析 · 整体行 · `千川消耗` |

### 不添加

- 直播间成交金额（XLSX 无列）

---

## Fixture 期望值（验收）

| 指标 | 期望值 |
|------|--------|
| 直播间用户支付金额 | ¥236,552 |
| 千次观看用户支付金额 | ¥26,439.25 |
| 直播间观看人数 | 6782 |
| 平均在线人数 | 22 |
| 人均观看时长 | 1分5秒 |
| 直播间观看-成交率 | 0.68% |
| 成交人数 | 46 |
| 成交件数 | 51 |
| 直播间商品点击-成交率 | 3.08% |
| 直播间曝光-观看率(人数) | 10.12% |
| 千川消耗 | ¥2,201.51 |

---

## 建议改动文件

- `src-tauri/src/live_data_import.rs` — 补 session 字段解析
- `src-tauri/src/database/live_dashboard.rs` — migration + 列
- `src/lib/liveDashboard.ts` — `dashboardMetricCards` 两行 11 指标
- `src/lib/liveDashboard.test.ts` — 用上表断言
- `src/page/LiveDataDashboard.svelte` — 两行 grid，去掉硬编码重复卡片

---

## 测试

```powershell
$env:CARGO_TARGET_DIR = "d:\git_work\bili-shadowreplay-worktrees\obsidian-vault-read-sync\src-tauri\target"
cargo test --bin bili-shadowreplay live_data_import::tests database::live_dashboard::tests
node --loader ts-node/esm src/lib/liveDashboard.test.ts
npm run build
```

---

## 复制给 Codex 的提示词

```markdown
请执行 Task 6：直播数据大屏 KPI 对齐官方罗盘。

阅读：
- docs/superpowers/plans/2026-07-29-live-dashboard-kpi-alignment-codex-handoff.md
- docs/integrations/douyin/live-dashboard-xlsx-field-map.md
- docs/superpowers/specs/2026-07-29-live-data-dashboard-import-design.md（「官方大屏 KPI 对齐」节）

在现有导入与大屏基础上：
1. 补解析/DB：成交人数、成交件数(汇总求和)、商品点击-成交率、曝光-观看率、千川消耗
2. dashboardMetricCards 第一行 6 + 第二行 5 共 11 张 KPI
3. 不要添加「直播间成交金额」；无 XLSX 源显示「— / 官方导出未提供」
4. 用 fixture valid-live-dashboard.xlsx 期望值写测试并跑绿

完成后列出改动文件与 manual 验收步骤。
```
