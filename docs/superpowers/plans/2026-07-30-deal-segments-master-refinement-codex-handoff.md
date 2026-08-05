# Codex 任务：成交片段驱动母稿精确化 — 7/28 金典拍拍

> **背景：** 7/28 专场已对齐罗盘 **51 件 / 46 人 / ¥236,552**（`orders-leaderboard-reconcile-v4.json`）。  
> **目标：** 用 **已核验的商品讲解片段**（不是付款前 90 秒窗口）对比企业母稿，生成候选辅稿，人工确认后发布 **母稿 patch 版本**（如 1.0.0 → 1.0.1），让母稿更精确。  
> **原则：** 主播原话 verbatim 入库；AI 只给对比/建议；`verified` 才自动进母稿对比队列。

---

## 为什么不能直接用 51 条付款窗口

用户已验证：

| 类型 | 数量 | 能否直接改母稿 |
|------|------|----------------|
| 品牌+型号匹配（verified/weak） | ~26 | 可以候选 |
| 付款窗口讲别的品牌（mismatch） | 18 | **禁止** |
| 只有通用催单（generic） | 14 | 仅参考 |

**正确链路：**

```
订单商品 → 全场 SRT 搜该商品讲解段 → verification_tier → 母稿章节匹配
  → compare_highlight_to_master → 候选辅稿 → 人工审核 → publish_master_upgrade
```

---

## 已有能力（复用，勿重写）

| 模块 | 路径 | 用途 |
|------|------|------|
| 母稿存储 | `master-script-store`, vault `10-企业母稿/{scriptKey}/V{x}/` | 版本化 MD + DB |
| 对比评分 Prompt2 | `master-comparison` | 六维分数 + 硬门槛 |
| 升级建议 Prompt3 | `upgrade_review.rs` | add_as_support / golden_sentence / merge / replace |
| 发布升级 | `publish_master_upgrade` | patch 版本 |
| 前端队列 | `SupportCandidateQueue.svelte` | 人工通过/驳回 |
| 成交片段 join | `scripts/join_srt_deal_segments.py` | 付款锚点 + SRT |
| **新增** | `scripts/prepare_master_refinement_candidates.py` | 商品检索 + tier + 母稿章节匹配 |

---

## Phase 1：离线准备（Codex 先做）

### Task 1: 导出 master baseline

在 App 内或通过 Tauri 调试导出 `get_master_baseline(script_key)` → JSON：

```json
{
  "master": { "id", "scriptKey", "version", ... },
  "sections": [
    { "id", "kind", "title", "productCardId", "hostText", "masterText", ... }
  ]
}
```

保存：`d:\Desktop\master-baseline-export.json`

### Task 2: 生成 deal segments（逐字稿完成后）

```powershell
python scripts\join_srt_deal_segments.py `
  --events "d:\Desktop\payment-events-20260728-v4.json" `
  --srt "D:\path\to\archive\transcript.corrected.srt" `
  --output "d:\Desktop\deal-segments-20260728-v4.json" `
  --csv "d:\Desktop\deal-segments-20260728-v4.csv"
```

### Task 3: 生成母稿精修候选

```powershell
python scripts\prepare_master_refinement_candidates.py `
  --deal-segments "d:\Desktop\deal-segments-20260728-v4.json" `
  --srt "D:\path\to\archive\transcript.corrected.srt" `
  --master-baseline "d:\Desktop\master-baseline-export.json" `
  --output "d:\Desktop\master-refinement-candidates.json"
```

**输出字段：**

- `verificationTier`: `verified` | `weak`（mismatch/generic/unresolved 已过滤）
- `speechText`: 商品检索得到的讲解原文（母稿对比用）
- `paymentWindowText`: 旧付款窗口（debug 对照）
- `masterSectionId` / `masterProductCardId`: 匹配到的母稿商品章
- `unique_by_product`: 同商品去重后 shortlist（51 单 → ~43 商品）

**验收：**

- Ricoh GR 不能用 13:01 付款窗口的 D850 话术；必须有 GR 检索段或 tier=unresolved 被跳过
- DJI Osmo 360 09:34 应为 verified
- `unique_by_product` 长度 ≤ 43

### Task 4: 单元测试

`scripts/test_prepare_master_refinement_candidates.py`

- fixture：Osmo360 verified, Ricoh mismatch skipped, Sony70200 mismatch skipped

---

## Phase 2：批量进母稿对比（App / Tauri）

### Task 5: 新命令 `import_deal_refinement_candidates`

**Input:** `master-refinement-candidates.json` + archive `TranscriptSource` (7/28 live_id)

**Logic:**

```rust
for candidate in payload.unique_by_product {
  if candidate.verification_tier != "verified" { continue; }  // weak → manual only
  compare_highlight_to_master(CompareHighlightRequest {
    script_key,
    expected_master_script_id,
    master_section_id: candidate.master_section_id,
    source_start_ms: candidate.speech_start_sec * 1000,
    source_end_ms: candidate.speech_end_sec * 1000,
    candidate_type: "订单锚定商品讲解",
    product_card_id: candidate.master_product_card_id,
    section_kind: Product,
    candidate_segment: { original_text: candidate.speech_text, ... },
  })
}
```

**Reuse:** `src-tauri/src/master_script/comparison.rs` — 不 fork Prompt2/3。

### Task 6: ArchiveAnalysis UI —「从成交候选导入母稿对比」

- 按钮在 `SupportCandidateQueue` 或 ArchiveAnalysis 工具栏
- 选择 `master-refinement-candidates.json`
- 展示导入进度：`verified N 条自动对比，weak M 条待人工`
- 结果进入现有 `MasterComparisonPanel` + `SupportCandidateQueue`

### Task 7: 人工审核 + 发布

沿用现有流程（`docs/master-script-operator-guide.md`）：

1. 候选辅稿队列 → **通过 / 保留 / 驳回**
2. 版本差异页确认 diff
3. `publish_master_upgrade` → vault `V1.0.1` + DB

**母稿精确化含义：**

| Prompt3 决策 | 母稿变化 |
|--------------|----------|
| `add_as_support` | 商品章追加 `## 候选辅稿`（7/28 实测话术） |
| `add_as_golden_sentence` | 追加 `## 金句话术` |
| `merge_with_existing` | 追加 `## 待合并话术` + 人工定稿 |
| `replace_existing` | `[建议稿]` 替换（需 diff 确认） |

---

## Phase 3：精确化规则（产品要求）

1. **只改有订单证据的商品章** — 7/28 成交榜 43 品，优先 `unique_by_product`
2. **价格/链接/型号** — Prompt2 硬门槛：`dynamicFields` 与 ASR 事实一致才 ≥85 分
3. **同商品多单** — 只取 verified 最高分段，避免重复候选
4. **weak 不自动入库** — UI 标记「待人工复核」，防止 Ricoh 窗口误配
5. **母稿 baseline 视频** — 与 master source 同源的 clip 只作 reference，不进候选（已有 `master_source_baseline` 规则）

---

## 用户现在能做什么（逐字稿跑完前）

| 步骤 | 动作 |
|------|------|
| 1 | 等 7/28 录播逐字稿生成完成 |
| 2 | 确认关键商品校稿（型号/价格）|
| 3 | 跑 Task 2 + Task 3 脚本 |
| 4 | 检查 `master-refinement-candidates.json` 里 verified 条数 |
| 5 | Phase 2 完成后在 App 一键导入对比 |
| 6 | 候选队列审核 → 发布 1.0.1 |

**临时手工路径（Phase 2 未做前）：**

- ArchiveAnalysis 打开 7/28 录播
- 对 verified 候选的 `speechStartSec–speechEndSec` 手动选段
- 触发「对比母稿」→ 候选队列 → 发布升级

---

## Codex Prompt（可直接粘贴）

```
Implement docs/superpowers/plans/2026-07-30-deal-segments-master-refinement-codex-handoff.md

Phase 1:
- Finish/test scripts/prepare_master_refinement_candidates.py
- Add scripts/test_prepare_master_refinement_candidates.py with Osmo360/Ricoh/Sony70200 fixtures

Phase 2:
- Tauri command import_deal_refinement_candidates reading master-refinement-candidates.json
- Loop verified unique_by_product → compare_highlight_to_master
- ArchiveAnalysis button to import JSON and show progress

Do NOT bypass human SupportCandidateQueue or publish_master_upgrade diff confirmation.
Host speech must stay verbatim; tier=mismatch must never auto-compare.

Sample: 7/28 金典拍拍, script_key from active master, reconcile v4 on Desktop.
```

---

## 验收清单

- [ ] `master-refinement-candidates.json` 含 `unique_by_product`，mismatch 已排除
- [ ] verified 条目 `speechText` 含正确品牌/型号
- [ ] 批量 import 后 SupportCandidateQueue 有 pending 条目
- [ ] 人工通过后 vault 出现 `V1.0.1`，商品章含 7/28 实测辅稿
- [ ] Ricoh GR 未用 D850 窗口文本进入母稿
