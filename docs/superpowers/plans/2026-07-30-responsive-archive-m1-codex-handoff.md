# Responsive Archive M1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Archive Analysis usable while media/ASR work runs, prevent duplicate full-media jobs, and prevent a failed UI event from crashing a worker.

**Architecture:** Keep the existing persistent `tasks` table and `TaskManager`, but make browser conversion and video subtitle generation share that one CPU queue. ArchiveAnalysis starts or observes work in the background and never awaits playback preparation during page setup; stale progress polling is bounded to the active panel and terminates when a task reaches a terminal state. Individual module failures remain local.

**Tech Stack:** Tauri 2, Rust, Tokio, SQLx/SQLite, Svelte 3, TypeScript, Vitest-free Node static checks, Cargo tests.

## Global Constraints

- Do not alter live-dashboard KPI, payment-event, deal-segment, transcript, or order-matching data semantics.
- Do not delete original recordings, generated playable videos, subtitles, or imported data.
- A running media task must not disable Archive list navigation, page back navigation, tabs, or other pages.
- Retain existing task records and use existing `pending`, `processing`, `success`, and `failed` statuses in M1; M2 introduces heartbeat/recovery migration.
- All new handling of task/event/database errors must be non-panicking and must produce a local retryable error.

---

## File structure

- `src-tauri/src/progress/progress_reporter.rs` — safe event delivery for task progress.
- `src-tauri/src/handlers/video.rs` — place ASR in the existing `TaskManager`; remove conversion’s DB polling dependency; preserve task rows and dedupe behavior.
- `src-tauri/src/task/mod.rs` — expose a testable completion signal from a queued task, without changing global media concurrency of one.
- `src-tauri/src/database/task.rs` — add focused active-task lookup and tests needed by video handlers.
- `src/page/ArchiveAnalysis.svelte` — nonblocking preparation/transcript state, bounded observation, and module-local status.
- `src/lib/archiveAnalysis.test.ts` and `scripts/check-analysis-reactivity.mjs` — regression checks that page initialization does not await long-running media work.

### Task 1: Make progress emission non-panicking

**Files:**
- Modify: `src-tauri/src/progress/progress_reporter.rs:55-100`
- Test: `src-tauri/src/progress/progress_reporter.rs` unit tests added under `#[cfg(test)]`

**Consumes:** `EventEmitter::emit(&RecorderEvent)`.

**Produces:** `EventEmitter::emit` returns `Result<(), String>` and `ProgressReporter::update/finish` records delivery failure only in logs; no worker panics because a webview listener is gone.

- [ ] **Step 1: Write the failing test**

Add a test-only emitter path that returns an emission error and assert that reporting progress still completes:

```rust
#[tokio::test]
async fn progress_update_does_not_panic_when_event_delivery_fails() {
    let reporter = failing_test_reporter();
    reporter.update("transcribing").await;
    assert!(reporter.last_delivery_error().is_some());
}
```

- [ ] **Step 2: Run the focused test to verify it fails**

Run: `cargo test progress_update_does_not_panic_when_event_delivery_fails --manifest-path src-tauri/Cargo.toml`

Expected: FAIL because the test emitter/error observation does not exist and GUI code still uses `unwrap()`.

- [ ] **Step 3: Implement the minimal safe emitter**

Change the GUI emission branches from `emit(...).unwrap()` to `emit(...).map_err(|error| error.to_string())`; make the headless branch return `Ok(())` after sending. `ProgressReporter::update` and `finish` must call the emitter with `if let Err(error)` and `log::warn!`, then continue their database status update.

- [ ] **Step 4: Run the focused test to verify it passes**

Run: `cargo test progress_update_does_not_panic_when_event_delivery_fails --manifest-path src-tauri/Cargo.toml`

Expected: PASS.

### Task 2: Queue video subtitle generation with the same CPU gate as playback conversion

**Files:**
- Modify: `src-tauri/src/handlers/video.rs:1202-1525`
- Modify: `src-tauri/src/task/mod.rs:1-350`
- Modify: `src-tauri/src/database/task.rs:1-390`
- Test: `src-tauri/src/handlers/video.rs` handler tests and `src-tauri/src/database/task.rs` tests

**Consumes:** `TaskManager::add_task(Task)`, `Database::add_subtitle_task_if_idle`, `Database::add_playback_conversion_task_if_idle`, `generate_video_subtitle_inner`.

**Produces:** `queue_video_subtitle_task(state, event_id, id, completion_tx) -> Result<(), String>`; both full-media operations pass through `TaskManager` and only one starts at a time. The existing command waits on a Tokio one-shot only when its caller requires the generated subtitle, then reads the saved subtitle artifact after successful completion. A duplicate request returns the active task status instead of spawning FFmpeg/ASR again.

- [ ] **Step 1: Write the failing tests**

Add tests for task ordering and dedupe:

```rust
#[tokio::test]
async fn media_tasks_share_one_running_slot() {
    let mut manager = TaskManager::with_config(TaskManagerConfig::default());
    manager.start();
    let running = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    enqueue_counted_media_task(&manager, "subtitle:7", running.clone(), peak.clone()).await;
    enqueue_counted_media_task(&manager, "playback:7", running, peak.clone()).await;
    wait_for_task_manager_idle(&manager).await;
    assert_eq!(peak.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn active_subtitle_task_prevents_second_subtitle_row() {
    let db = test_db().await;
    assert!(db.add_subtitle_task_if_idle(&subtitle_task("first", 7), 7).await.unwrap());
    assert!(!db.add_subtitle_task_if_idle(&subtitle_task("second", 7), 7).await.unwrap());
}
```

- [ ] **Step 2: Run focused tests to verify failure**

Run: `cargo test media_tasks_share_one_running_slot active_subtitle_task_prevents_second_subtitle_row --manifest-path src-tauri/Cargo.toml`

Expected: the queue ordering test fails until subtitle generation is delegated to `TaskManager`; the database test remains green as a regression baseline.

- [ ] **Step 3: Implement queue delegation and remove DB busy-wait**

1. Extract the current subtitle body after task-row creation into `run_video_subtitle_task(state: State, reporter: ProgressReporter, id: i64) -> Result<String, String>`; it writes the canonical artifact before returning the subtitle.
2. In `generate_video_subtitle`, create `let (completion_tx, completion_rx) = tokio::sync::oneshot::channel::<Result<String, String>>();`, insert the task row atomically, then enqueue `Task::new(event_id, TaskPriority::High, async move { let result = run_video_subtitle_task(...).await; let task_result = result.as_ref().map(|_| ()).map_err(Clone::clone); let _ = completion_tx.send(result); task_result })`. Await `completion_rx` only in this command and return its subtitle string. Never retain a SQLx connection across that wait.
3. In `prepare_video_playback`, remove the `while get_active_video_subtitle_task + sleep` loop. The conversion task is enqueued at normal priority after the high-priority subtitle task, so it naturally waits in memory without DB polling or occupying a worker.
4. Keep `add_*_task_if_idle` as the only creator of a persistent row. If a row already exists, return the current `VideoPlaybackSource`/active subtitle state; do not create an additional worker.
5. On enqueue failure update exactly the newly-created task to `failed`; on handler completion update exactly that same task to `success` or `failed`.

- [ ] **Step 4: Run focused tests to verify pass**

Run: `cargo test media_tasks_share_one_running_slot active_subtitle_task_prevents_second_subtitle_row --manifest-path src-tauri/Cargo.toml`

Expected: PASS; peak concurrent media task count is one.

### Task 3: Keep ArchiveAnalysis initialization and media observation nonblocking

**Files:**
- Modify: `src/page/ArchiveAnalysis.svelte:1036-1081,1325-1465`
- Modify: `src/lib/archiveAnalysis.test.ts:1-180`
- Modify: `scripts/check-analysis-reactivity.mjs`

**Consumes:** `get_video_playback_source`, `prepare_video_playback`, `get_video_subtitle`, `generate_video_subtitle`, background task statuses.

**Produces:** `initialize()` returns after scheduling independent loads; `prepareVideoForPlayback()` and transcript generation display task progress without holding the whole page in a `while` loop.

- [ ] **Step 1: Write failing regression checks**

Extend `scripts/check-analysis-reactivity.mjs` to assert the source contains all of:

```js
assert.match(source, /void loadVideoPlaybackSource\(/);
assert.match(source, /void loadActiveMaster\(\)/);
assert.doesNotMatch(
  initializationBody,
  /await\s+(prepareVideoForPlayback|resumeOrGenerateVideoTranscript|waitForVideoTranscriptTask)/,
);
assert.match(source, /Promise\.allSettled|moduleLoadError|playbackLoadError/);
```

Add a pure TypeScript test for a terminal task state:

```ts
assert.equal(isTerminalBackgroundTaskStatus("success"), true);
assert.equal(isTerminalBackgroundTaskStatus("failed"), true);
assert.equal(isTerminalBackgroundTaskStatus("processing"), false);
```

- [ ] **Step 2: Run tests to verify failure**

Run: `npm run test:analysis-reactivity && node --loader ts-node/esm src/lib/archiveAnalysis.test.ts`

Expected: FAIL until the per-module task state and terminal-state helper exist.

- [ ] **Step 3: Implement local module states and bounded observation**

1. Add `playbackLoadError`, `transcriptLoadError`, and an `isTerminalBackgroundTaskStatus(status)` helper in `ArchiveAnalysis.svelte` (or `src/lib/archiveAnalysis.ts` when the helper is reused).
2. `initialize()` must invoke, not await, dashboard binding, playback source, master, payment events, and transcript refresh; after `loadSaved` it sets `stage` to an informational message and returns.
3. Replace the unbounded `while` in `prepareVideoForPlayback` with one background observer started after `prepare_video_playback` returns. It updates only player state and ends on ready, failed, or a maximum observation timeout; it must reset `isPreparingPlayback` in `finally`.
4. Replace the unbounded transcript wait with a background observer that stops once no active task exists; surface failure in `transcriptLoadError`, leaving saved transcript and all navigation intact.
5. Keep the player/analysis panels mounted when an observer fails. Render a retry button in that panel only; do not set the global `errorMessage` for expected media-job failures.

- [ ] **Step 4: Run tests to verify pass**

Run: `npm run test:analysis-reactivity && node --loader ts-node/esm src/lib/archiveAnalysis.test.ts`

Expected: PASS.

### Task 4: Mark M1 regression checks complete and prove the app is responsive

**Files:**
- Modify: `docs/superpowers/specs/2026-07-30-responsive-archive-background-jobs-design.md` (mark M1 implemented only after evidence)
- Modify: this plan’s checkboxes for completed steps

**Consumes:** Tasks 1–3.

**Produces:** repeatable build/test evidence and a manual acceptance record. No user data changes.

- [ ] **Step 1: Run backend formatting and tests**

Run:

```powershell
cargo fmt --manifest-path src-tauri/Cargo.toml -- --check
cargo test task::tests --manifest-path src-tauri/Cargo.toml
cargo test progress_update_does_not_panic_when_event_delivery_fails --manifest-path src-tauri/Cargo.toml
```

Expected: PASS.

- [ ] **Step 2: Run frontend checks**

Run:

```powershell
npm run test:analysis-reactivity
npm run test:company-analysis-workspace
npm run check
```

Expected: PASS. If `npm run check` fails due pre-existing errors, record the exact errors and do not describe M1 as fully green.

- [ ] **Step 3: Manual test protocol**

1. Start one long `.ts`/`.flv` playable-version conversion.
2. While it runs, open the Archive list, switch company/competitor tabs, open a second archive, then return to the first.
3. Start subtitle generation for the same recording; verify one task is queued/blocked rather than a second ffmpeg process launching.
4. Close and reopen only the analysis page; verify controls remain clickable and task status remains local to the player/transcript panel.
5. Force a task failure with a missing source file; verify a retry message appears in that panel and the rest of the page remains available.

- [ ] **Step 4: Update handoff status without committing unrelated work**

Run: `git diff --check -- src-tauri/src/progress/progress_reporter.rs src-tauri/src/handlers/video.rs src-tauri/src/task/mod.rs src-tauri/src/database/task.rs src/page/ArchiveAnalysis.svelte src/lib/archiveAnalysis.test.ts scripts/check-analysis-reactivity.mjs`

Expected: no whitespace errors. Do not stage or commit because this worktree already contains user-owned WIP changes.

## M2/M3 follow-up boundaries

M2 implements the database migration (`heartbeat_at`, `worker_pid`, `cancel_requested`, `blocked_reason`, and task resource metadata), SQLite WAL/pool configuration after confirming the Tauri SQL plugin pool boundary, and interrupted-task recovery. M3 implements the non-modal background task drawer and user-visible cancellation/retry. They are intentionally separate from M1 to keep the immediate UI unlock fix reviewable and reversible.
