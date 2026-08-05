# Codex 任务：成交片段分析 — 分阶段校稿 + 去「照着念」

> **来源：** Cursor 代码审查结论 + 产品策略确认  
> **日期：** 2026-07-27  
> **分支：** `obsidian-vault-read-sync`  
> **Cursor 职责：** 只审 diff，不改代码  
> **Codex 职责：** 按本文实施、跑测试、提交  

---

## 产品背景

- **系统目标：** 找到重要成交话术 → 人工审核 → 更新企业母稿 → 用于主播培训  
- **公司立场：** AI 只给建议，**不能**作为「可直接照着念的标准稿」  
- **选定策略：** **发现不阻塞校稿（A），母稿入库路径严格校稿（B）**

---

## 策略定义（必须先理解再改）

| 路径 | 是否阻塞（关键校稿未完成时） |
|------|------------------------------|
| 片段发现 `discoverCandidates` | ❌ 不阻塞 |
| 单段选中、复盘官 LLM | ❌ 不阻塞 |
| 整场母稿复盘 `runFullSessionReview` | ✅ 阻塞 |
| 候选辅稿入库 / 母稿升级 | ✅ 阻塞或明确降级且 UI 提示不可入库 |

**保持：** `currentHighlightWorkflowStage()` 的 bypass（`auditLoadStatus === "loaded"` 且有 transcript → 允许发现）。

**删除或补全：** 与上述策略矛盾的半实现逻辑（见 Task 1）。

---

## Task 1：统一校稿 Gate（P0）

### 改什么

**主要文件：**

- `src/page/ArchiveAnalysis.svelte`
- `src/lib/transcriptReview.ts`（若删除 continue 分支需同步测试）

**具体项：**

1. 删除 `hadPendingCriticalReview: false` 硬编码；若采用本策略，删除不可达的 `discoveryAction === "continue"` 流程及相关 UI（「全部处理完成，继续分析」按钮等）
2. 补全或删除空 `if` 块：`ArchiveAnalysis.svelte` 约 1056–1057、1110–1111 行
3. `runFullSessionReview`：保持 `pendingCriticalCount > 0` 时拦截，文案清晰
4. 单段 `compareHighlightToMaster`：校稿未完成时允许发起（与后端降级一致），UI 提示「逐字稿关键项待确认，评分仅供参考，不可入库」
5. 确保 `currentHighlightWorkflowStage()`、`highlightDiscoveryAction`、`runFullSessionReview` 三处策略一致

### 为什么这么改

当前代码三套逻辑打架：发现自动跑、continue 按钮永远不出、整场复盘又拦校稿。统一后行为可预期，符合「先找话术、后定标准」。

### 针对谁

- **直播运营 / 复盘同学**（日常找片段）
- **培训负责人 / 母稿维护者**（整场评分、候选辅稿）

### 有什么用

- 运营不用等校稿就能开始找成交时刻
- 进母稿的数据仍要求逐字稿关键项确认，避免错型号、错价格进入培训标准

### 验证

```powershell
npm run test:transcript-review
```

**手动：** 有关键校稿时 → 能自动发现；点「整场复盘」→ 被拦住并提示先校稿。

---

## Task 2：弱化「照着念」定位（P0）

### 改什么

**主要文件：**

- `src/lib/agent/prompts.ts`（`COMMERCE_REVIEW_PROMPT`）
- `src/page/ArchiveAnalysis.svelte`（`reviewSelectedCandidate` 的 systemPrompt、`parseReview` 结果展示）
- `src/lib/components/analysis/MasterComparisonPanel.svelte`

**具体项：**

1. Prompt：去掉「可直接口播」「照着念」；改为「参考改写建议」「培训讨论要点」
2. UI 字段 `spoken_script` 标签 → **「参考表达（非标准稿，不可直接照念）」**
3. `training_checklist` 标签 → **「培训讨论点 / 练习方向」**
4. 母稿对比面板加固定说明：**「以下为企业可复用价值评估，须经人工定稿后方可进入母稿培训」**

### 为什么这么改

公司不认可 AI 产出当标准口播稿；现有 Prompt/UI 易让人以为高分 = 可以照念，与培训理念冲突。

### 针对谁

- **主播 / 新人**（看复盘结果）
- **培训负责人**（决定是否采纳进母稿）

### 有什么用

- 降低「照着 AI 念就行」的误用
- 明确 AI = 辅助发现 + 给建议，定稿权在人
- 与「候选辅稿 → 人工审 → 更新母稿」流程一致

### 验证

跑一段复盘，确认 UI 无「可直接口播」类文案；有 Prompt/文案相关测试则更新。

---

## Task 3：结构化候选跳过冗余 Context Verify（P1）

### 改什么

**文件：** `src/page/ArchiveAnalysis.svelte` → `selectCandidate` / `verifyCandidateContext`

**逻辑：**

若 `isStructuredDiscoveryCandidate(candidate)` 且 `evidenceItems`、`whySelected`、`originalText` 齐全 → **跳过** legacy 三分类 context verify LLM。

仅对 legacy / 兜底候选保留 `verifyCandidateContext`。

### 为什么这么改

V2 发现已带证据，再跑 legacy 核验是重复 LLM、拖慢操作，且 legacy taxonomy 可能干扰展示。

### 针对谁

每天做多场复盘的 **运营同学**

### 有什么用

- 选中片段后更快进入复盘和母稿对比
- 少一次模型调用，省成本、少超时

### 验证

```powershell
node --loader ts-node/esm src/lib/archiveAnalysis.test.ts
```

补测试：结构化候选不应触发 context verify（可 mock `invoke` 计数）。

---

## Task 4：筛选 Tab 与七步链路展示（P1）

### 改什么

**文件：**

- `src/lib/archiveAnalysis.ts`（`compatibilityChainStages`）
- `src/page/ArchiveAnalysis.svelte`（筛选 tab 文案与逻辑）

**具体项：**

1. 发现阶段：`type === "完整成交链路"` 时 **不要** 在 `compatibilityChainStages` 预填全部 7 步；完整链路仅在 context verify 通过 / `verified_complete` 后展示
2. 筛选 tab「完整已核验」→ 改名为 **「完整成交链路（已核验）」**，或新增 **「可评分片段」** tab（filter: `isStructuredDiscoveryCandidate`）

### 为什么这么改

列表过早显示假完整链路会误导运营；V2 大量片段（金句、异议处理）在「完整已核验」里看不到，筛选名不副实。

### 针对谁

看候选列表做筛选的 **运营 / 培训负责人**

### 有什么用

- 列表信息更可信
- 按类型找片段更符合 V2 十二类设计

### 验证

`archiveAnalysis.test.ts` 中 `compatibilityChainStages` / 展示相关用例。

---

## Task 5：重发现清理 + 商品词典（P2）

### 改什么

**文件：**

- `src/page/ArchiveAnalysis.svelte` → `discoverCandidates`
- 知识库接入（若已有接口）：`get_knowledge_status` / vault 读取 → `buildCandidateDiscoveryPrompt(dictionary)`

**具体项：**

1. `discoverCandidates` 重跑时清空 `masterComparisons`（与 `reviews = {}` 一致）
2. 将企业商品词典传入 `buildCandidateDiscoveryPrompt`，替代当前固定 `""`

### 为什么这么改

重发现后旧评分残留污染 localStorage；空词典导致型号识别弱，影响母稿章节匹配。

### 针对谁

- 全体使用者（清理）
- 多 SKU 直播的 **运营**（词典）

### 有什么用

- 状态干净，恢复分析不串台
- 发现阶段商品名更准，母稿匹配率更高

---

## 不要改（Cursor 结论）

- 后端 `src-tauri/src/master_script/comparison.rs` 主评分逻辑：已合理
- 候选辅稿人工队列机制：保持，符合「AI 只建议」

---

## 完成后必跑测试

```powershell
cd d:\git_work\bili-shadowreplay-worktrees\obsidian-vault-read-sync

npm run test:transcript-review
npm run test:master-script
node --loader ts-node/esm src/lib/archiveAnalysis.test.ts
cargo test -p master-comparison
```

---

## Cursor 手动验收清单（改完后给用户）

- [ ] 有关键校稿：能自动发现片段
- [ ] 有关键校稿：整场复盘被拦，提示先校稿
- [ ] 结构化候选：选中后无多余 context verify 等待
- [ ] 复盘结果：无「照着念 / 可直接口播」表述
- [ ] 母稿面板：有「须人工定稿」说明
- [ ] 重新发现：旧 `masterComparisons` 已清理
- [ ] 「完整成交链路」：发现瞬间不显示假七步

---

## 参考文件（Cursor 已读）

| 文件 | 说明 |
|------|------|
| `src/page/ArchiveAnalysis.svelte` | 主流程编排 |
| `src/lib/archiveAnalysis.ts` | 发现/归一化/筛选领域逻辑 |
| `src/lib/transcriptReview.ts` | 工作流 stage / discovery action |
| `src/lib/agent/prompts.ts` | 复盘官 Prompt |
| `src/lib/components/analysis/MasterComparisonPanel.svelte` | 母稿对比 UI |
| `src-tauri/src/master_script/comparison.rs` | 后端评分与入库 |
| `docs/superpowers/plans/2026-07-26-segment-type-scoring-v2.md` | V2 十二类评分设计 |

---

## 给 Codex 的一句话

> 按「发现不阻塞、入库严格校稿、AI 只建议不照念」改成交片段分析；P0 做完再 P1/P2；改完跑测试并把 diff 交给 Cursor 审查。
