# NAS Video Storage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Archive completed recordings and imported videos to a company UGREEN NAS, verify each NAS copy, delete the local media only after success, and recover safely from network failures.

**Architecture:** Recording and import continue to write to the local output directory. A focused `nas_archive` service owns destination naming, transactional `.uploading` copies, verification, retry state, and final database path updates. Tauri commands expose configuration and manual retry; the settings and video-list UIs display simple operator-facing states.

**Tech Stack:** Rust 2021, Tokio, SQLx/SQLite, Tauri 2, Svelte 3, TypeScript, Windows SMB/UNC paths.

## Global Constraints

- Recording must never write directly to SMB; local disk remains the active recording target.
- Delete a local media file only after NAS size and media-duration verification succeeds and the database commit completes.
- Keep SQLite, provider secrets, the knowledge vault, transcript working files, and analysis jobs local.
- Store the UNC root, for example `\\192.168.1.100\直播录像`; never store a NAS password.
- A NAS outage must leave the local source intact and recover pending work after application restart.
- This phase supports the company LAN only and must not expose SMB to the internet.
- Do not overwrite an existing NAS file; create a collision-safe destination name.
- Run focused tests after each task and one full desktop check only at final integration.

---

## File Structure

- `src-tauri/src/config.rs`: persisted NAS settings and defaults.
- `src-tauri/src/database/video_archive.rs`: archive table schema, rows, and SQL operations.
- `src-tauri/src/nas_archive.rs`: path planning, transactional copy, verification, retry scheduling, and startup recovery.
- `src-tauri/src/handlers/config.rs`: save/test NAS settings commands.
- `src-tauri/src/handlers/video.rs`: list status, manual retry, and open NAS location commands; enqueue imported videos.
- `src-tauri/src/recorder_manager.rs`: enqueue completed recording videos.
- `src-tauri/src/state.rs`: shared archive service handle.
- `src-tauri/src/main.rs`: migration, command registration, service creation, and startup queue recovery.
- `src/lib/nasStorage.ts`: frontend types and beginner-facing status normalization.
- `src/lib/nasStorage.test.ts`: frontend status tests.
- `src/lib/interface.ts`: NAS fields on config/video interfaces.
- `src/lib/components/settings/NasVideoStorageSettings.svelte`: NAS settings panel.
- `src/page/Setting.svelte`: settings panel integration.
- `src/page/Clip.svelte`: video storage status, progress, retry, and NAS location actions.
- `scripts/internal-delivery/seed/Conf.template.toml` or current delivery seed source: disabled NAS defaults for newly installed clients.

### Task 1: Persist NAS Settings And Test Connectivity

**Files:**
- Modify: `src-tauri/src/config.rs`
- Modify: `src-tauri/src/handlers/config.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `src/lib/interface.ts`
- Test: `src-tauri/src/config.rs`

**Interfaces:**
- Produces: `NasVideoStorageConfig { enabled, root_path, archive_recordings, archive_imports, delete_local_after_archive }`
- Produces: `test_nas_video_storage(root_path: String) -> Result<NasConnectionResult, String>`
- Consumes: Windows credentials already registered for the UNC share.

- [ ] **Step 1: Write failing config default and round-trip tests**

```rust
#[test]
fn nas_storage_defaults_are_safe() {
    let config = Config::load(&path, &cache, &output).unwrap();
    assert!(!config.nas_video_storage.enabled);
    assert!(config.nas_video_storage.root_path.is_empty());
    assert!(config.nas_video_storage.archive_recordings);
    assert!(config.nas_video_storage.archive_imports);
    assert!(config.nas_video_storage.delete_local_after_archive);
}

#[test]
fn nas_storage_round_trips_through_toml() {
    let parsed: Config = toml::from_str(CONFIG_WITH_NAS).unwrap();
    assert_eq!(parsed.nas_video_storage.root_path, r"\\nas\直播录像");
}
```

- [ ] **Step 2: Run the focused config tests and verify failure**

Run:

```powershell
$env:CMAKE_GENERATOR='Ninja'
cargo test --manifest-path src-tauri/Cargo.toml config::tests::nas_storage -- --nocapture
```

Expected: FAIL because `nas_video_storage` and its type do not exist.

- [ ] **Step 3: Add the persisted configuration**

```rust
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
pub struct NasVideoStorageConfig {
    pub enabled: bool,
    pub root_path: String,
    pub archive_recordings: bool,
    pub archive_imports: bool,
    pub delete_local_after_archive: bool,
}

impl Default for NasVideoStorageConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            root_path: String::new(),
            archive_recordings: true,
            archive_imports: true,
            delete_local_after_archive: true,
        }
    }
}
```

Add `#[serde(default)] pub nas_video_storage: NasVideoStorageConfig` to `Config`.

- [ ] **Step 4: Implement configuration update and connection testing**

The connection command must:

1. Reject an empty path.
2. Require a UNC path on Windows.
3. Confirm the directory exists.
4. Create a uniquely named zero-byte probe file.
5. Flush and delete the probe file.
6. Return a safe result containing reachability and display path, never credentials.

```rust
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NasConnectionResult {
    pub reachable: bool,
    pub root_path: String,
}
```

- [ ] **Step 5: Register commands and run tests**

Run:

```powershell
$env:CMAKE_GENERATOR='Ninja'
cargo test --manifest-path src-tauri/Cargo.toml config::tests -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: config tests PASS and backend compiles.

- [ ] **Step 6: Commit only Task 1 files**

```powershell
git add src-tauri/src/config.rs src-tauri/src/handlers/config.rs src-tauri/src/main.rs src/lib/interface.ts
git commit -m "feat: add NAS video storage settings"
```

### Task 2: Add Durable Video Archive State

**Files:**
- Create: `src-tauri/src/database/video_archive.rs`
- Modify: `src-tauri/src/database/mod.rs`
- Modify: `src-tauri/src/main.rs`
- Test: `src-tauri/src/database/video_archive.rs`

**Interfaces:**
- Produces: `VideoArchiveRow`
- Produces: `enqueue_video_archive`, `claim_next_video_archive`, `update_video_archive_progress`, `complete_video_archive`, `fail_video_archive`, `reset_interrupted_video_archives`
- Consumes: existing `videos.id` and `videos.file`.

- [ ] **Step 1: Write failing database lifecycle tests**

```rust
#[tokio::test]
async fn archive_job_moves_through_pending_uploading_archived() {
    let db = test_database().await;
    let job = db.enqueue_video_archive(7, "recording").await.unwrap();
    assert_eq!(job.status, "pending");

    let claimed = db.claim_next_video_archive().await.unwrap().unwrap();
    assert_eq!(claimed.status, "uploading");

    db.complete_video_archive(job.id, r"\\nas\直播录像\主播\2026-07-26\a.mp4")
        .await
        .unwrap();
    assert_eq!(db.get_video_archive(job.id).await.unwrap().status, "archived");
}

#[tokio::test]
async fn interrupted_upload_is_reset_without_losing_source() {
    // Insert uploading, run reset, assert retry_wait and local_path unchanged.
}
```

- [ ] **Step 2: Run the database tests and verify failure**

Run:

```powershell
$env:CMAKE_GENERATOR='Ninja'
cargo test --manifest-path src-tauri/Cargo.toml database::video_archive::tests -- --nocapture
```

Expected: FAIL because the module and table do not exist.

- [ ] **Step 3: Add migration 25 and row model**

```sql
CREATE TABLE video_archive_jobs (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  video_id INTEGER NOT NULL UNIQUE REFERENCES videos(id) ON DELETE CASCADE,
  source_kind TEXT NOT NULL CHECK(source_kind IN ('recording','import')),
  status TEXT NOT NULL CHECK(status IN ('local_only','pending','uploading','archived','retry_wait','failed')),
  local_path TEXT NOT NULL,
  nas_path TEXT NOT NULL DEFAULT '',
  uploaded_bytes INTEGER NOT NULL DEFAULT 0,
  retry_count INTEGER NOT NULL DEFAULT 0,
  last_error TEXT NOT NULL DEFAULT '',
  next_retry_at TEXT,
  created_at TEXT NOT NULL DEFAULT (datetime('now')),
  updated_at TEXT NOT NULL DEFAULT (datetime('now'))
);
CREATE INDEX idx_video_archive_jobs_status_retry
ON video_archive_jobs(status, next_retry_at, id);
```

- [ ] **Step 4: Implement atomic state updates**

`claim_next_video_archive` must perform claim selection and status update in one SQL transaction. `complete_video_archive` must update `videos.file` and the archive job in one database transaction.

- [ ] **Step 5: Run focused database tests**

Run:

```powershell
$env:CMAKE_GENERATOR='Ninja'
cargo test --manifest-path src-tauri/Cargo.toml database::video_archive::tests -- --nocapture
```

Expected: all archive database tests PASS.

- [ ] **Step 6: Commit only Task 2 files**

```powershell
git add src-tauri/src/database/video_archive.rs src-tauri/src/database/mod.rs src-tauri/src/main.rs
git commit -m "feat: persist NAS video archive jobs"
```

### Task 3: Implement Safe NAS Copy And Verification

**Files:**
- Create: `src-tauri/src/nas_archive.rs`
- Modify: `src-tauri/src/lib.rs` or `src-tauri/src/main.rs` module declarations
- Test: `src-tauri/src/nas_archive.rs`

**Interfaces:**
- Produces: `NasArchiveService::archive_once(&self, job: &VideoArchiveRow, config: &NasVideoStorageConfig) -> Result<ArchiveOutcome, ArchiveError>`
- Produces: `plan_nas_destination(root, title, created_at, source) -> Result<PathBuf, ArchiveError>`
- Consumes: Task 1 config and Task 2 database methods.

- [ ] **Step 1: Write failing path and copy safety tests**

```rust
#[test]
fn destination_groups_by_sanitized_anchor_and_date() {
    let path = plan_nas_destination(
        Path::new(r"\\nas\直播录像"),
        r#"于千惠:新品/专场"#,
        "2026-07-26 13:00:00",
        Path::new(r"D:\videos\a.mp4"),
    ).unwrap();
    assert!(path.ends_with(r"于千惠_新品_专场\2026-07-26\a.mp4"));
}

#[tokio::test]
async fn failed_copy_keeps_local_source() {
    // Use a missing destination root, assert local source still exists.
}

#[tokio::test]
async fn successful_copy_uses_temporary_name_then_final_name() {
    // Assert no .uploading file remains and final bytes equal source bytes.
}
```

- [ ] **Step 2: Run focused tests and verify failure**

Run:

```powershell
$env:CMAKE_GENERATOR='Ninja'
cargo test --manifest-path src-tauri/Cargo.toml nas_archive::tests -- --nocapture
```

Expected: FAIL because the NAS archive module does not exist.

- [ ] **Step 3: Implement destination planning and collision handling**

Use `sanitize_filename` for the anchor directory and filename. When the destination exists, append `-2`, `-3`, and so on before the extension; never overwrite.

- [ ] **Step 4: Implement transactional copy**

Copy in bounded chunks to `<final-name>.uploading`, update progress after each chunk, call `sync_all`, verify byte length, run existing FFprobe metadata extraction, then rename to the final destination.

```rust
pub struct ArchiveOutcome {
    pub nas_path: PathBuf,
    pub bytes: u64,
}
```

The service must not delete the local file. Deletion happens only after Task 2's database completion transaction succeeds.

- [ ] **Step 5: Implement retry classification**

Map missing share, permission denied, timeout, copy failure, and verification failure to operator-safe Chinese messages. Use retry delays of 1, 5, 15, 30, and 60 minutes; after repeated failures retain `failed` for manual retry.

- [ ] **Step 6: Run focused service tests**

Run:

```powershell
$env:CMAKE_GENERATOR='Ninja'
cargo test --manifest-path src-tauri/Cargo.toml nas_archive::tests -- --nocapture
```

Expected: all path, copy, collision, verification, and failure-preservation tests PASS.

- [ ] **Step 7: Commit only Task 3 files**

```powershell
git add src-tauri/src/nas_archive.rs src-tauri/src/main.rs
git commit -m "feat: safely archive videos to NAS"
```

### Task 4: Enqueue Recordings And Imports, Then Recover On Restart

**Files:**
- Modify: `src-tauri/src/state.rs`
- Modify: `src-tauri/src/main.rs`
- Modify: `src-tauri/src/recorder_manager.rs`
- Modify: `src-tauri/src/handlers/video.rs`
- Test: `src-tauri/src/nas_archive.rs`
- Test: `src-tauri/src/database/video_archive.rs`

**Interfaces:**
- Produces: `NasArchiveService::enqueue(video_id, source_kind)`
- Produces: `NasArchiveService::resume_pending()`
- Consumes: recording/import `VideoRow` only after the video database row exists.

- [ ] **Step 1: Write failing enqueue policy tests**

```rust
#[tokio::test]
async fn recording_is_enqueued_only_when_enabled_for_recordings() {
    // Assert one pending job when enabled and none when disabled.
}

#[tokio::test]
async fn restart_resumes_pending_and_interrupted_jobs() {
    // Seed pending/uploading/retry_wait, run resume, assert each eligible job is processed once.
}
```

- [ ] **Step 2: Run focused policy tests and verify failure**

Run:

```powershell
$env:CMAKE_GENERATOR='Ninja'
cargo test --manifest-path src-tauri/Cargo.toml nas_archive::tests -- --nocapture
```

Expected: FAIL because enqueue/recovery policy is not integrated.

- [ ] **Step 3: Add the service to shared state and startup**

Create one background worker. At startup:

1. Reset interrupted `uploading` jobs to `retry_wait`.
2. Start the queue loop.
3. Wake immediately for eligible work.
4. Sleep without blocking Tauri or recorder tasks.

- [ ] **Step 4: Enqueue completed recordings**

Immediately after `RecorderManager` successfully inserts the completed `VideoRow`, enqueue the video only if `enabled && archive_recordings`.

- [ ] **Step 5: Enqueue completed imports**

Immediately after single or batch external import successfully inserts the `VideoRow`, enqueue only if `enabled && archive_imports`.

- [ ] **Step 6: Complete archive and delete local source safely**

Worker order:

1. Copy and verify NAS file.
2. Commit NAS path and `archived` status in SQLite.
3. If `delete_local_after_archive`, delete only the exact captured local source.
4. If deletion fails, keep archive `archived` and record a cleanup warning; never delete the NAS file.

- [ ] **Step 7: Run focused backend tests**

Run:

```powershell
$env:CMAKE_GENERATOR='Ninja'
cargo test --manifest-path src-tauri/Cargo.toml nas_archive::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml database::video_archive::tests -- --nocapture
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: policy, restart, and database tests PASS; backend compiles.

- [ ] **Step 8: Commit only Task 4 files**

```powershell
git add src-tauri/src/state.rs src-tauri/src/main.rs src-tauri/src/recorder_manager.rs src-tauri/src/handlers/video.rs src-tauri/src/nas_archive.rs
git commit -m "feat: archive completed recordings and imports"
```

### Task 5: Add Operator Settings And Video Status

**Files:**
- Create: `src/lib/nasStorage.ts`
- Create: `src/lib/nasStorage.test.ts`
- Create: `src/lib/components/settings/NasVideoStorageSettings.svelte`
- Modify: `src/lib/interface.ts`
- Modify: `src/page/Setting.svelte`
- Modify: `src/page/Clip.svelte`
- Modify: `src-tauri/src/handlers/video.rs`
- Modify: `src-tauri/src/main.rs`

**Interfaces:**
- Produces: `normalizeNasArchiveView(job): NasArchiveView`
- Produces Tauri commands: `retry_video_archive(video_id)`, `open_video_archive_location(video_id)`
- Produces Tauri command: `delete_archived_video(video_id, delete_nas_file)`
- Consumes: Task 1 settings commands and Task 2 archive fields.

- [ ] **Step 1: Write failing beginner-facing status tests**

```ts
assert.deepEqual(
  normalizeNasArchiveView({ status: "retry_wait", lastError: "Access denied" }),
  {
    label: "NAS 连接异常",
    detail: "视频已安全保留在本机，连接恢复后会自动重试",
    tone: "warning",
    canRetry: true,
  },
);
```

Cover all six persisted states and ensure raw Rust/SQL errors are not displayed as the primary label.

- [ ] **Step 2: Run the frontend unit test and verify failure**

Run:

```powershell
node --loader ts-node/esm src/lib/nasStorage.test.ts
```

Expected: FAIL because the normalizer does not exist.

- [ ] **Step 3: Implement status normalization**

Provide fixed Chinese labels:

- 本机
- 等待存入 NAS
- 正在存入 NAS
- 已存入 NAS
- NAS 连接异常
- 转存失败

- [ ] **Step 4: Build the settings component**

Use a checkbox for enablement, a folder/path text input, and explicit command buttons for “测试连接” and “保存设置”. Show a green success band or restrained warning band; never show secrets.

- [ ] **Step 5: Add list status and actions**

In the video list:

- Show progress only for `uploading`.
- Show “打开 NAS 位置” only for `archived`.
- Show “立即重试” for `retry_wait` and `failed`.
- State clearly when the local source is being retained.
- For archived videos, replace the ambiguous delete action with an explicit confirmation that distinguishes “仅移除系统记录” from “同时删除 NAS 视频”.

- [ ] **Step 6: Implement retry/open commands**

Manual retry sets the job to `pending` and wakes the worker. Open-location validates the stored path and opens its parent directory; missing NAS files return “NAS 文件不存在或当前不可访问”.

`delete_archived_video` must reject deletion unless the caller explicitly supplies `delete_nas_file`. When false, delete only the database record and retain the NAS file. When true, verify that the resolved path is below the configured NAS root before deleting the NAS file and database record.

- [ ] **Step 7: Run focused frontend and type checks**

Run:

```powershell
node --loader ts-node/esm src/lib/nasStorage.test.ts
npm run check
```

Expected: status tests PASS and Svelte reports no errors.

- [ ] **Step 8: Commit only Task 5 files**

```powershell
git add src/lib/nasStorage.ts src/lib/nasStorage.test.ts src/lib/components/settings/NasVideoStorageSettings.svelte src/lib/interface.ts src/page/Setting.svelte src/page/Clip.svelte src-tauri/src/handlers/video.rs src-tauri/src/main.rs
git commit -m "feat: show NAS archive controls and status"
```

### Task 6: Package Defaults And Final Integration

**Files:**
- Modify: `scripts/check-internal-delivery-package.ps1`
- Modify: `scripts/check-internal-delivery-exe.ps1`
- Modify: `docs/superpowers/specs/2026-07-26-nas-video-storage-design.md` only if implementation revealed a required correction.
- Test: existing delivery package check scripts.

**Interfaces:**
- Produces: an installer with NAS storage disabled and no embedded NAS credentials.
- Consumes: Tasks 1-5.

- [ ] **Step 1: Enforce safe package defaults**

Keep `NasVideoStorageConfig::default()` as the source of truth. Extend both delivery checkers to install the package in isolation, read the installed configuration through the application's config parser or a safe validation command, and assert that NAS storage is disabled with an empty root. Do not add another version-controlled configuration template containing provider secrets.

- [ ] **Step 2: Run all focused NAS tests**

Run:

```powershell
$env:CMAKE_GENERATOR='Ninja'
cargo test --manifest-path src-tauri/Cargo.toml config::tests::nas_storage -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml database::video_archive::tests -- --nocapture
cargo test --manifest-path src-tauri/Cargo.toml nas_archive::tests -- --nocapture
node --loader ts-node/esm src/lib/nasStorage.test.ts
```

Expected: all NAS-specific tests PASS.

- [ ] **Step 3: Run the single final desktop check**

Run:

```powershell
$env:CMAKE_GENERATOR='Ninja'
cargo test --manifest-path src-tauri/Cargo.toml
npm run check
npm run build
npm run tauri build
```

Expected: all backend tests PASS, Svelte check PASS, frontend build PASS, and Tauri desktop build exits 0.

- [ ] **Step 4: Perform real UGREEN NAS acceptance**

With the company UNC share configured:

1. Test connection.
2. Record a short test stream.
3. Confirm progress changes from pending to uploading to archived.
4. Play the NAS copy and compare its duration with the database duration.
5. Confirm the local media file is deleted only after archive completion.
6. Disconnect NAS, complete a second short recording, and confirm the local source remains.
7. Restore NAS and confirm automatic retry archives the second video.
8. Run one transcript and clip analysis from the NAS-backed video.

- [ ] **Step 5: Rebuild and validate the direct-install EXE**

Run the existing internal delivery builder and:

```powershell
$newExe = Get-ChildItem -LiteralPath 'D:\典典直播切片交付' -Filter '*直接双击安装*.exe' |
    Sort-Object LastWriteTime -Descending |
    Select-Object -First 1
.\scripts\check-internal-delivery-exe.ps1 -InstallerPath $newExe.FullName -SevenZipPath 'D:\典典直播切片交付验证\tools\7zip-26.02-extra\x64\7za.exe'
```

Expected: embedded archive, isolated install, and real startup checks PASS.

- [ ] **Step 6: Commit package defaults and acceptance records**

```powershell
git add scripts docs
git commit -m "chore: package NAS video storage support"
```
