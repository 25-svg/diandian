# Codex 任务：片段分析页「按钮锁死 / 跳转不了」修复

> **来源：** Cursor 对照用户截图 + 代码审查  
> **日期：** 2026-07-27  
> **文件：** `src/page/ArchiveAnalysis.svelte`  

---

## 用户现象（截图）

- 右下：**「复盘官正在生成分析、参考表达和练习方向…」**（`reviewingId` 进行中）
- **「开始整场复盘」显示 1/27**（`isSessionReviewing === true`）
- 左侧约 27 个可评分片段
- 视频：**「此视频尚未生成可播放版本」**（与按钮锁无关，需单独点「生成可播放版本」）
- 用户反馈：**片段、Tab、按钮都跳转不了**

---

## 根因（按优先级）

### R1. 整场复盘循环「抢」选中片段（主因）

`runFullSessionReview()` 每处理一段就把 `selectedCandidateId` 改成下一段：

```typescript
for (const candidate of reusableCandidates) {
  selectedCandidateId = candidate.id;  // ← 用户手动点的片段会被覆盖
  await autoCompareCandidateToMaster(candidate, true);
}
```

27 段 × 每段母稿对比（Rust + LLM）≈ **数十分钟**。期间 UI 选中项自动跳走，**像点哪都没用**。

### R2. 复盘官 spinner 占满右栏

```svelte
{#if reviewingId && reviewingId === selectedCandidateId}
  <Loader2 /> 复盘官正在生成分析…
```

此状态下 **一眼结论 / 参考表达 / 练习方向** 三个 Tab 整段被替换，无法切换。

### R3. `reviewingId` 全局互斥

- `reviewSelectedCandidate`：`if (reviewingId) return;`
- 「重新复盘」：`disabled={Boolean(reviewingId)}`
- 换片段时 **不清** `reviewingId` → 新片段「开始复盘」点了也没反应

### R4. 发现完成后自动复盘第一段 + 用户点整场复盘叠加

`discoverCandidates` 结束 → `await selectCandidate(candidates[0])` → 自动开复盘官 LLM。  
用户再点「开始整场复盘」→ **复盘官 + 27 段母稿对比同时跑**，IPC/LLM 排队，整页像卡死。

---

## 修复要求

### Fix 1（P0）：整场复盘改为后台模式，不抢 UI 选中

- 新增 `sessionReviewTargetId` 或使用内部索引，**不要**在循环里改 `selectedCandidateId`
- 仅更新 `sessionReviewCompleted` 与进度文案
- 循环结束后可选：恢复用户进入复盘前的 `selectedCandidateId`

### Fix 2（P0）：增加「取消」

- `isSessionReviewing` 时显示 **「取消整场复盘」**，置 `sessionReviewAbort = true` 跳出 loop
- `reviewingId` 进行中显示 **「取消当前复盘」**，递增 `reviewRequestSequence` 并清 `reviewingId`

### Fix 3（P0）：换片段时取消旧复盘

在 `selectCandidate` 开头：

```typescript
if (reviewingId && reviewingId !== candidate.id) {
  reviewRequestSequence += 1;
  reviewingId = "";
}
```

### Fix 4（P1）：互斥启动

- `runFullSessionReview` 开头：若 `reviewingId` 非空，提示「请等待当前片段复盘完成或点取消」
- 或自动取消当前 `reviewingId` 再开始（二选一，推荐前者 + 取消按钮）

### Fix 5（P1）：右栏 loading 不挡 Tab

复盘进行中仍显示 Tab 栏；仅在内容区显示 loading，允许先看上一段已保存的 `selectedReview`。

### Fix 6（P2）：发现后不要强制 await 第一段完整复盘

`discoverCandidates` 末尾：

```typescript
// 改前
await selectCandidate(candidates[0]);

// 改后：只选中，复盘/母稿对比 lazy 或 fire-and-forget，不阻塞发现结束
selectedCandidateId = candidates[0]?.id || "";
void selectCandidate(candidates[0]); // 或拆成 selectOnly + 用户点「开始复盘」
```

---

## 验证

1. 发现 27 段后，右栏 Tab 可点；第一段复盘 spinner 不挡 Tab（Fix 5）
2. 点「开始整场复盘」后，**手动点左侧其他片段**，选中项保持不动（Fix 1）
3. 复盘进行中点「取消」，按钮恢复、可换片段（Fix 2）
4. A 段复盘未完成时点 B 段，B 段可「开始复盘」（Fix 3）
5. 侧边栏 / 左上角返回始终可点（不应被本页 async 阻塞）

---

## 测试

```powershell
npm run test:transcript-review
node --loader ts-node/esm src/lib/archiveAnalysis.test.ts
```

手动：27 段录播 → 发现 → 整场复盘 → 中途换片段、取消。

---

## 给用户的一句话说明（修完后）

「整场复盘会在后台逐段打分，不再抢走你正在看的片段；任何时候可以点取消。」
