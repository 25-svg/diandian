# Codex 任务：爱复盘模块地图 → 典典直播切片产品路线图

> **来源：** Cursor 产品对照分析 + 公司策略确认  
> **日期：** 2026-07-27  
> **分支：** `obsidian-vault-read-sync`  
> **Cursor 职责：** 审 diff、验收  
> **Codex 职责：** 按阶段实施、跑测试  

---

## 产品定位（必读）

| 对标 | 定位 |
|------|------|
| **爱复盘** | 直播 AI 运营大脑：快、全、SaaS、偏「诊断 + 可复制话术」 |
| **典典直播切片** | **企业直播教研与培养系统**：深、准、母稿驱动、本地可控 |

### 公司原则（不可违背）

1. **AI 只给建议，不能直接当「照着念的标准稿」**
2. **进企业母稿 / 培训标准必须人工审定**（候选辅稿队列）
3. **证据优先**：片段必须有原话、时间、入选理由
4. **数据可留本地**（NAS / 本机），不强制 SaaS

### 不做清单

- ❌ 「一键导出标准口播、新人背稿上播」
- ❌ AI 高分自动写入母稿（跳过人审）
- ❌ 132 行业通用话术库（用 Obsidian + 企业母稿即可）
- ❌ 先做竞品自动录播 SaaS（二期以后再说）

---

## 五层架构（目标态）

```
第1层 采集    录播 + 逐字稿 + 校稿 artifact +（P2）弹幕/平台分钟数据
第2层 拆解    话术分段 + 功能分类 + 证据链
第3层 诊断    内容评分 + 结果信号 + 合规/事实风险
第4层 计划    优化清单 + 母稿 diff + 下场行动项
第5层 培养    审定母稿 + 练习包 + 再播对比
```

### 现状对照

| 层 | 已有 | 缺失 |
|----|------|------|
| 采集 | `recorder_manager`、ASR、NAS、`transcript_artifacts` | 平台分钟数据 API |
| 拆解 | 12 类 V2 发现、`ArchiveAnalysis` | 运营向 10 类标签映射、总监摘要 |
| 诊断 | 母稿六维分、`gates`、`summarizeSessionReview` | 一页纸总诊断、文本侧结果信号 |
| 计划 | 复盘 `improvements`、upgrade review | 结构化「下场优化计划单」导出 |
| 培养 | 母稿、候选队列、培训讨论点 | 培养包 UI、再播对比闭环 |

---

## 模块对照表（爱复盘 → 本系统）

| 爱复盘模块 | 本系统命名（对用户） | 核心产出 | 主要代码位置 |
|------------|----------------------|----------|--------------|
| 话术拆解 | **成交话术拆解** | 证据片段列表 + 功能标签 | `archiveAnalysis.ts`、`ArchiveAnalysis.svelte` |
| 话术仿写 | **话术发展建议**（非仿写/非口播） | `[建议稿]` + 母稿 diff | `prompts.ts`、复盘官、`MasterComparisonPanel` |
| 数据诊断 | **场次诊断** | 总监摘要 + 片段分 | `summarizeSessionReview`、`comparison.rs` |
| 优化计划 | **下场优化计划** | Markdown/PDF 行动清单 | 新建 `sessionOptimizationPlan.ts` + 导出 UI |
| 主播培养 | **母稿培养包** | 范例 + 讨论题 + 练习记录 | 母稿队列 + Obsidian + 新页面或 Tab |

---

## P0 — 话术拆解体验 + 总监摘要（1～2 周）

### Task P0-1：运营向功能标签（10 类映射）

**改什么**

- 新建 `src/lib/scriptTaxonomy.ts`（或扩 `archiveAnalysis.ts`）
- 将 V2 十二类映射为运营熟悉的标签，例如：

| 运营标签 | 可映射的 V2 类型 |
|----------|------------------|
| 迎新留人 | 留人钩子、信任建立 |
| 需求判断 | 需求判断 |
| 塑品讲解 | 产品讲解、产品推荐 |
| 异议处理 | 异议处理、售后与风险消除 |
| 报价链接 | 价格、优惠或链接承接 |
| 逼单成交 | 催单与成交确认、完整成交链路 |
| 金句示范 | 高质量金句 |
| 反面案例 | 需要改进的反面案例 |

- `ArchiveAnalysis.svelte` 片段卡片展示 **双标签**：运营标签 + 原始类型

**为什么这么改** | 运营熟悉爱复盘/行业话术分类，降低认知成本  
**针对谁** | 日常做复盘的运营  
**有什么用** | 不用记 12 类英文式名称，列表一眼能筛  

**验证**

- `archiveAnalysis.test.ts` 增加映射用例
- UI 手动：卡片显示「塑品讲解 · 产品讲解」

---

### Task P0-2：单场「总监级诊断摘要」（5 分钟体验）

**改什么**

- 新建 `src/lib/sessionDiagnosis.ts`
  - 输入：`candidates[]`、`masterComparisons`、`auditBundle`、`sessionReviewSummary`
  - 输出：

```typescript
type SessionDiagnosis = {
  headline: string;           // 一句话结论，≤45 字
  highlights: string[];     // 最多 3 条
  issues: string[];           // 最多 3 条
  confirmations: string[];    // 待确认事实，含校稿 pending
  nextActions: string[];    // 下场 3 条动作
  stats: SessionReviewSummary;
};
```

- 实现方式（二选一，优先 A）：
  - **A.** 规则聚合已有 `reviews` + `masterComparisons` + `pendingCriticalCount`（无额外 LLM）
  - **B.** 可选调用 `minimax_chat` 生成 headline（仅当规则结果为空时）

- `ArchiveAnalysis.svelte`：发现完成后 / 校稿区上方展示 **「本场诊断摘要」** 折叠卡片，不阻塞片段列表

**为什么这么改** | 对齐爱复盘「先给结论再下钻」，避免一进来就 27 段 + 锁 UI  
**针对谁** | 运营总监、培训负责人  
**有什么用** | 5 分钟内知道「这场值不值得深挖、先改什么」  

**验证**

- `sessionDiagnosis.test.ts`
- 手动：发现完成后摘要可见；无需等每个片段复盘完

---

### Task P0-3：UI 锁死修复（前置依赖）

**文档：** `docs/superpowers/plans/2026-07-27-analysis-ui-lock-codex-handoff.md`

**必须先做**，否则 P0-2 摘要页也会被「整场复盘 1/27 + 复盘官 spinner」挡住。

要点：

- 整场复盘后台跑，不抢 `selectedCandidateId`
- 增加「取消整场复盘 / 取消当前复盘」
- 换片段时清 `reviewingId`

---

### Task P0-4：「话术发展建议」定稿（原话术仿写）

**改什么**

- `src/lib/agent/prompts.ts` — `COMMERCE_REVIEW_PROMPT`
- `ArchiveAnalysis.svelte` — 复盘 systemPrompt
- 全文替换：
  - ❌ 可直接口播 / 照着念 / 口播版本
  - ✅ **话术发展建议（非标准稿）** / **培训讨论用改写**
- `spoken_script` 字段 UI 标签固定为：**「话术发展建议（不可直接照念，须培训负责人审定）」**
- 导出 Markdown 模板头部加免责声明

**为什么这么改** | 公司不认可 AI 标准稿；爱复盘「仿写」在本系统 = 建议稿  
**针对谁** | 培训负责人审定、主播只看审定后母稿  
**有什么用** | 满足「仿写」需求但不越界  

**验证**

- 全局 grep 无「可直接口播」
- 复盘结果 UI 文案检查

---

## P1 — 优化计划 + 培养包（2～4 周）

### Task P1-1：下场优化计划单（导出）

**改什么**

- 新建 `src/lib/optimizationPlan.ts`

```typescript
type OptimizationPlan = {
  sessionTitle: string;
  diagnosedAt: string;
  masterScriptVersion: string;
  summary: SessionDiagnosis;
  masterSectionUpdates: Array<{
    sectionTitle: string;
    reason: string;
    candidateSegmentId?: string;
    score?: number;
  }>;
  practiceSegments: Array<{
    id: string;
    label: string;
    scene: string;
    discussionPoints: string[];
  }>;
  factConfirmations: string[];
};
```

- `buildOptimizationPlan(...)` 从当前场次状态组装
- `formatOptimizationPlanMarkdown(plan)` → 可复制 / 保存 `.md`
- `ArchiveAnalysis.svelte` 增加按钮：**「导出下场优化计划」**

**为什么这么改** | 对齐爱复盘「优化计划」；运营需要可转发给主播的文档  
**针对谁** | 运营写计划、培训负责人排课  
**有什么用** | 复盘不止于看 UI，能进企业微信 / Obsidian  

**验证**

- 单元测试 markdown 结构
- 手动导出含：摘要 + 待确认 + 建议练的 2～3 段

---

### Task P1-2：母稿培养包（最小版）

**改什么**

- 新建 `src/lib/trainingPack.ts` + 组件 `TrainingPackPanel.svelte`（或挂在 `ArchiveAnalysis` / `Setting` 下）
- 输入：已审定母稿章节 + 高分候选片段（`admission === candidate_queue` 或人审通过）
- 输出培养包：

```
# 培养包：{章节名}
## 标准范例（企业母稿原文）
## 金牌片段（本场/历史，带视频时间）
## 讨论题（来自 training_checklist / 培训讨论点）
## 练习要求（不对着念，说明改进方向）
## 再播检查项（下场对照母稿分 ≥ X）
```

- 数据可从 `getMasterBaseline` + `masterComparisons` + `reviews` 读取
- 首期 **只读导出 Markdown**，不做 AI 陪练

**为什么这么改** | 对齐爱复盘「主播培养」；闭环在母稿而非通用 SOP  
**针对谁** | 培训负责人、新主播  
**有什么用** | 审定后的素材直接变成培训教材  

**验证**

- 有母稿 + 至少 1 个 scored 片段时可导出
- 包内无「请照着念」表述

---

### Task P1-3：候选辅稿 → 培养包联动

**改什么**

- `SupportCandidateQueue.svelte`：审定通过后提示 **「生成培养包」**
- 可选写入 Obsidian `04-话术模块/` 或 `06-辅稿/`（若 `knowledge_writer` 已有接口则复用）

**为什么这么改** | 人审通过的内容不应停在 SQLite，要进培训资产  
**针对谁** | 培训负责人  
**有什么用** | 爱复盘「资产沉淀」的本地版  

---

## P2 — 数据诊断增强（按数据接入节奏）

### Task P2-1：文本侧「结果信号」代理（无平台 API）

**改什么**

- 新建 `src/lib/transcriptSignals.ts`
- 在片段 `start/end` 窗口统计：
  - 问价词、链接词、成交确认词、转品词
  - 可选：弹幕 JSON 若存在则计密度
- 片段卡片增加一行：**「结果信号：问价×2 / 无成交确认」**
- 总监摘要 `issues` 可引用信号（如「多段有问价无成交确认」）

**为什么这么改** | 在无抖音分钟数据前，部分对齐爱复盘「话术↔结果」  
**针对谁** | 运营判断片段价值  
**有什么用** | 比纯文本抽象分更接近业务真实  

---

### Task P2-2：平台分钟数据接入（可选，需商务/API）

**前置：** 公司提供抖音/快手数据接口或导出 CSV  

**改什么**

- 导入 CSV 或 API → 与 `transcriptEntries` 时间轴对齐
- 片段详情展示：该段前后 2 分钟在线人数变化（简图或 ↑↓）

**暂不在 P0/P1 实施**，仅在架构预留 `PlaybackMetrics` 类型。

---

### Task P2-3：合规/事实风险（二手相机场景）

**改什么**

- 规则库：`src/lib/complianceRules.ts`
  - 极限词、虚假承诺、成色未确认表述等
- 扫描 `correctedSrt` 或片段文本 → 总监摘要 `confirmations` / 风险计数
- 不对接爱复盘 10 万词库，先做 **金典拍拍专用短规则表**

**为什么这么改** | 二手相机合规敏感；爱复盘「违规助手」的垂直版  
**针对谁** | 品控、运营  
**有什么用** | 降低封号与客诉风险  

---

## P2 — 组织级（管理层，较大）

### Task P2-4：多账号 / 多主播汇总看板

**改什么**

- 新页面 `src/page/TrainingDashboard.svelte`（或扩展 `Summary.svelte`）
- 聚合：每场诊断 headline、均分、待确认数、培养包生成数
- 依赖 P0-2 摘要结构统一

**针对谁** | 管理层  
**有什么用** | 爱复盘「管理驾驶舱」的轻量本地版  

---

## 文件索引（实施时优先读）

| 文件 | 用途 |
|------|------|
| `src/page/ArchiveAnalysis.svelte` | 主分析 UI |
| `src/lib/archiveAnalysis.ts` | 发现/片段领域 |
| `src/lib/agent/prompts.ts` | 复盘官 Prompt |
| `src/lib/masterScript.ts` | 母稿对比类型 |
| `src/lib/components/analysis/MasterComparisonPanel.svelte` | 母稿分展示 |
| `src/lib/components/master/SupportCandidateQueue.svelte` | 候选辅稿人审 |
| `src-tauri/src/master_script/comparison.rs` | 后端评分 |
| `src-tauri/crates/master-comparison/src/lib.rs` | 模型 JSON 解析 |
| `src-tauri/crates/knowledge/` | Obsidian 知识库 |

---

## 测试命令

```powershell
cd d:\git_work\bili-shadowreplay-worktrees\obsidian-vault-read-sync

npm run test:transcript-review
npm run test:master-script
node --loader ts-node/esm src/lib/archiveAnalysis.test.ts
cargo test -p master-comparison
```

新增模块各自加 `*.test.ts`。

---

## 实施顺序（给 Codex 的执行序）

```
1. P0-3  UI 锁死修复          ← 必须先做
2. P0-4  话术发展建议文案
3. P0-1  10 类运营标签映射
4. P0-2  总监级诊断摘要
5. P1-1  优化计划导出
6. P1-2  培养包最小版
7. P1-3  候选队列 → 培养包
8. P2-*  按优先级和数据就绪情况
```

---

## 验收清单（给用户手动测）

- [ ] 发现完成后 30 秒内能看到「本场诊断摘要」
- [ ] 片段卡片有运营向标签（如「塑品讲解」）
- [ ] 全文无「照着念 / 可直接口播」
- [ ] 可导出「下场优化计划」Markdown
- [ ] 高分候选可导出「培养包」
- [ ] 整场复盘不抢选中片段，可取消
- [ ] 母稿对比 `cue_1` 类格式错误已修复（见 master-comparison 补丁）

---

## 给 Codex 的一句话

> 按爱复盘模块地图在本系统落地五层能力；P0 先做 UI 解锁 + 总监摘要 + 话术发展建议；禁止照念式仿写与自动写母稿；改完跑测试并把 diff 交给 Cursor 审查。

---

## 参考链接

- 爱复盘官网：https://www.ifupan.com/
- 相关 Codex 文档：
  - `docs/superpowers/plans/2026-07-27-deal-segment-analysis-codex-handoff.md`
  - `docs/superpowers/plans/2026-07-27-analysis-ui-lock-codex-handoff.md`
  - `docs/superpowers/plans/2026-07-26-segment-type-scoring-v2.md`
