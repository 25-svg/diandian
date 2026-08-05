# Candidate Segment Discovery V2 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make recording analysis discover all twelve training-value segment types with traceable evidence while preserving compatibility with the existing review and master-comparison workflow.

**Architecture:** Extend the `archiveAnalysis` domain model with a V2 discovery schema and a narrow adapter into existing candidate fields. Keep parsing and normalization in `src/lib/archiveAnalysis.ts`; keep model invocation and UI rendering in `src/page/ArchiveAnalysis.svelte`. The new discovery data remains authoritative, while old score/tier fields are compatibility-only.

**Tech Stack:** TypeScript, Svelte, Node assert-based focused tests, existing MiniMax Tauri command.

## Global Constraints

- Only change the recording-analysis candidate discovery workflow.
- Do not modify second-stage scoring, master comparison, candidate admission, or AI assistant mining.
- Model output uses `{ "segments": [...] }`; parser also accepts legacy arrays.
- Preserve all twelve discovery types and four outcome values.
- Never infer missing prices, stock, links, after-sales promises, or confirmed conversions.
- Run only `archiveAnalysis` focused tests during this phase.

---

### Task 1: V2 discovery domain and parser

**Files:**
- Modify: `src/lib/archiveAnalysis.ts`
- Test: `src/lib/archiveAnalysis.test.ts`

**Interfaces:**
- Consumes: unknown JSON parsed from the model response.
- Produces: `parseDiscoverySegments(value: unknown): HighlightCandidate[]`.
- Produces: V2 fields on `HighlightCandidate`: `scene`, `customerNeed`, `originalText`, `keySentence`, `outcome`, `interrupted`, `whySelected`, `evidenceItems`.

- [ ] **Step 1: Write failing V2 schema tests**

Add cases that pass a `{ segments: [...] }` envelope containing a high-quality quote and assert that:

```ts
assert.equal(result[0].type, "高质量金句");
assert.equal(result[0].outcome, "unconfirmed");
assert.equal(result[0].interrupted, true);
assert.equal(result[0].hook, "最值得保留的一句话");
assert.equal(result[0].reason, "为什么值得进入下一步评分");
```

Also assert unknown types, invalid outcomes, missing evidence and invalid ranges are rejected.

- [ ] **Step 2: Run the focused test and confirm it fails**

Run:

```powershell
npx tsx src/lib/archiveAnalysis.test.ts
```

Expected: TypeScript import or assertion failure because V2 parser and fields do not exist.

- [ ] **Step 3: Implement the V2 types and compatibility adapter**

Add:

```ts
export type DiscoverySegmentType =
  | "完整成交链路"
  | "高质量金句"
  | "需求判断"
  | "产品推荐"
  | "产品讲解"
  | "异议处理"
  | "留人钩子"
  | "信任建立"
  | "售后与风险消除"
  | "价格、优惠或链接承接"
  | "催单与成交确认"
  | "需要改进的反面案例";

export type DiscoveryOutcome =
  | "confirmed_conversion"
  | "conversion_signal"
  | "unconfirmed"
  | "no_conversion";
```

Normalize `segment_id`, time strings, evidence items and V2 names. Populate legacy `evidence`, `hook`, `reason`, `signals`, `tier`, `score`, `chainStages` and `verificationStatus` without changing the original V2 fields.

- [ ] **Step 4: Run the focused domain test**

Run:

```powershell
npx tsx src/lib/archiveAnalysis.test.ts
```

Expected: `archiveAnalysis domain tests passed`.

### Task 2: Candidate selection compatibility

**Files:**
- Modify: `src/lib/archiveAnalysis.ts`
- Test: `src/lib/archiveAnalysis.test.ts`

**Interfaces:**
- Consumes: V2 `HighlightCandidate[]`.
- Produces: discovery candidates that retain incomplete chains, quotes and negative cases.

- [ ] **Step 1: Add failing selection tests**

Create candidates for `高质量金句`, `异议处理`, and `需要改进的反面案例` without complete sales chains. Assert `selectDiscoveryCandidates` retains all three.

- [ ] **Step 2: Run the focused test and confirm it fails**

Run:

```powershell
npx tsx src/lib/archiveAnalysis.test.ts
```

Expected: one or more V2 candidates are filtered or normalized to legacy types.

- [ ] **Step 3: Update normalization, merge and selection rules**

Ensure:

```ts
selectDiscoveryCandidates(v2Candidates)
```

does not require a complete chain. Merge only candidates with compatible type/product and overlapping ranges. Preserve `confirmed_conversion` as a discovery claim that still requires second-stage verification.

- [ ] **Step 4: Run the focused domain test**

Run:

```powershell
npx tsx src/lib/archiveAnalysis.test.ts
```

Expected: `archiveAnalysis domain tests passed`.

### Task 3: Replace the recording discovery prompt and response parsing

**Files:**
- Modify: `src/page/ArchiveAnalysis.svelte`
- Test: `src/lib/archiveAnalysis.test.ts`

**Interfaces:**
- Consumes: transcript chunks and the available product dictionary context.
- Produces: `{ segments: [...] }` model response parsed by `parseDiscoverySegments`.

- [ ] **Step 1: Add prompt contract assertions**

Export a prompt builder from `src/lib/archiveAnalysis.ts`:

```ts
buildCandidateDiscoveryPrompt(productDictionary: string): string
```

Assert the prompt contains all twelve types, four outcomes, the 30-second-to-3-minute guidance, interruption rules and the no-fabrication boundary.

- [ ] **Step 2: Run the focused test and confirm it fails**

Run:

```powershell
npx tsx src/lib/archiveAnalysis.test.ts
```

Expected: missing prompt builder.

- [ ] **Step 3: Implement and wire the prompt**

Move the approved prompt into `buildCandidateDiscoveryPrompt`. Update `ArchiveAnalysis.svelte` to call it and parse the full JSON envelope with `parseDiscoverySegments`.

When no enterprise product dictionary is available, pass:

```text
当前未提供企业商品词典。型号不确定时标记为待确认，不得猜测。
```

- [ ] **Step 4: Run the focused domain test**

Run:

```powershell
npx tsx src/lib/archiveAnalysis.test.ts
```

Expected: `archiveAnalysis domain tests passed`.

### Task 4: Display V2 discovery information

**Files:**
- Modify: `src/page/ArchiveAnalysis.svelte`
- Test: `src/lib/archiveAnalysis.test.ts`

**Interfaces:**
- Consumes: V2 fields on the selected candidate.
- Produces: beginner-readable cards with scene, key sentence, result and interruption status.

- [ ] **Step 1: Add presentation tests**

Add a pure helper:

```ts
candidateOutcomeLabel(outcome: DiscoveryOutcome): string
```

Assert all four outcomes map to plain Chinese:

```ts
confirmed_conversion -> 已发现成交确认，等待复核
conversion_signal -> 出现成交信号
unconfirmed -> 尚未确认结果
no_conversion -> 未形成成交
```

- [ ] **Step 2: Run the focused test and confirm it fails**

Run:

```powershell
npx tsx src/lib/archiveAnalysis.test.ts
```

Expected: missing helper.

- [ ] **Step 3: Update candidate cards**

Show:

- type and scene;
- key sentence;
- outcome label;
- interruption badge when `interrupted` is true;
- `whySelected` as the selection explanation.

Do not display first-stage compatibility score as a business score.

- [ ] **Step 4: Run focused tests and TypeScript check**

Run:

```powershell
npx tsx src/lib/archiveAnalysis.test.ts
npx svelte-check --tsconfig ./tsconfig.json
```

Expected: domain tests pass; Svelte check introduces no new errors in changed files.

### Task 5: Final focused verification

**Files:**
- Verify: `src/lib/archiveAnalysis.ts`
- Verify: `src/lib/archiveAnalysis.test.ts`
- Verify: `src/page/ArchiveAnalysis.svelte`

- [ ] **Step 1: Run the focused test**

```powershell
npx tsx src/lib/archiveAnalysis.test.ts
```

- [ ] **Step 2: Inspect the diff**

```powershell
git diff --check
git diff -- src/lib/archiveAnalysis.ts src/lib/archiveAnalysis.test.ts src/page/ArchiveAnalysis.svelte
```

Confirm no unrelated workflow or scoring code changed.

