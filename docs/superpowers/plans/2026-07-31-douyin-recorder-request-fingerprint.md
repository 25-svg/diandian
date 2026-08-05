# Douyin Recorder Request Fingerprint Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restore automatic local recording for live Douyin rooms with a coherent signed request and visible pull failures.

**Architecture:** The recorder crate owns web request fingerprinting and one retry with refreshed room identity. `RecorderInfo` passes a non-sensitive retry error to the Svelte room card.

**Tech Stack:** Rust, reqwest, deno_core, Tokio, Svelte, TypeScript.

## Global Constraints

- Never log or modify stored cookies.
- Do not select Volcengine or change ASR configuration.
- Preserve existing recordings and HLS retry behavior.
- Refresh `sec_uid` at most once per failed H5 lookup.

---

### Task 1: Stabilize the signed web room request

**Files:**
- Modify: `src-tauri/crates/recorder/src/platforms/douyin/api.rs`
- Test: `src-tauri/crates/recorder/src/platforms/douyin/api.rs`

**Produces:** `build_web_room_request(room_id, ms_token) -> (String, HeaderMap)`.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn web_room_request_uses_one_browser_fingerprint() {
    let (query, headers) = build_web_room_request("123", "token");
    assert!(query.contains("browser_platform=Win32"));
    assert!(query.contains("browser_version=126.0.0.0"));
    assert!(headers["user-agent"].to_str().unwrap().contains("Windows NT"));
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test -p recorder web_room_request_uses_one_browser_fingerprint`

Expected: FAIL because `build_web_room_request` is absent.

- [ ] **Step 3: Implement the minimal builder**

```rust
const DOUYIN_WEB_USER_AGENT: &str =
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36";

fn build_web_room_request(room_id: &str, ms_token: &str) -> (String, HeaderMap) {
    let query = format!("aid=6383&app_name=douyin_web&live_id=1&device_platform=web&language=zh-CN&enter_from=web_live&cookie_enabled=true&screen_width=1920&screen_height=1080&browser_language=zh-CN&browser_platform=Win32&browser_name=Chrome&browser_version=126.0.0.0&web_rid={room_id}&ms_token={ms_token}");
    let mut headers = HeaderMap::new();
    headers.insert("User-Agent", DOUYIN_WEB_USER_AGENT.parse().unwrap());
    headers.insert("Referer", "https://live.douyin.com/".parse().unwrap());
    (query, headers)
}
```

Use `query` verbatim for signing and append only `a_bogus` to the final URL; add the stored Cookie header after building these headers.

- [ ] **Step 4: Verify GREEN and commit**

Run: `cargo test -p recorder web_room_request_uses_one_browser_fingerprint`

Expected: PASS.

```bash
git add src-tauri/crates/recorder/src/platforms/douyin/api.rs
git commit -m "fix: stabilize douyin room request fingerprint"
```

### Task 2: Bound identity recovery and carry retry error

**Files:**
- Modify: `src-tauri/crates/recorder/src/lib.rs`
- Modify: `src-tauri/crates/recorder/src/traits.rs`
- Modify: `src-tauri/crates/recorder/src/platforms/douyin.rs`
- Modify: `src-tauri/crates/recorder/src/platforms/douyin/api.rs`
- Modify: `src-tauri/src/recorder_manager.rs`
- Test: `src-tauri/crates/recorder/src/platforms/douyin/api.rs`

**Produces:** `RecorderInfo.last_error: String`; one H5 lookup retry after `sec_uid` refresh.

- [ ] **Step 1: Write the failing recovery test**

```rust
#[test]
fn h5_identity_recovery_refreshes_once() {
    let attempts = h5_recovery_attempts(&[H5Failure::InvalidRequest, H5Failure::InvalidRequest]);
    assert_eq!(attempts.request_count, 2);
    assert_eq!(attempts.sec_uid_refresh_count, 1);
}
```

- [ ] **Step 2: Verify RED**

Run: `cargo test -p recorder h5_identity_recovery_refreshes_once`

Expected: FAIL because `h5_recovery_attempts` is absent.

- [ ] **Step 3: Implement bounded recovery and health state**

```rust
match get_room_info_h5(client, account, room_id, sec_uid).await {
    Ok(info) => Ok(info),
    Err(error) if is_identity_error(&error) => {
        let refreshed = get_room_owner_sec_uid(client, room_id).await?;
        get_room_info_h5(client, account, room_id, &refreshed).await
    }
    Err(error) => Err(error),
}
```

Add `last_error: String` to `RecorderInfo`. Add `last_error: Arc<RwLock<String>>` to `DouyinExtra`; write concise errors in `check_status` and `update_entries`, clear before `RecordStart`, and override `DouyinRecorder::info()` to return it. Default trait info and `recorder_manager.rs` placeholder data use `String::new()`.

- [ ] **Step 4: Verify GREEN and commit**

Run: `cargo test -p recorder h5_identity_recovery_refreshes_once && cargo test -p recorder`

Expected: PASS.

```bash
git add src-tauri/crates/recorder/src/lib.rs src-tauri/crates/recorder/src/traits.rs src-tauri/crates/recorder/src/platforms/douyin.rs src-tauri/crates/recorder/src/platforms/douyin/api.rs src-tauri/src/recorder_manager.rs
git commit -m "fix: recover douyin recorder room identity"
```

### Task 3: Render pull failure on the room card

**Files:**
- Modify: `src/lib/interface.ts`
- Modify: `src/page/Room.svelte`
- Create: `src/lib/recorderStatus.ts`
- Create: `src/lib/recorderStatus.test.ts`

**Consumes:** `RecorderInfo.last_error` from Task 2.

- [ ] **Step 1: Add a failing room-card state test**

Extract the room-card recorder state choice into a small frontend helper. Add a focused test that fails before implementation for a live room with `last_error`, while preserving recording as the highest-priority state.

```ts
export interface RecorderInfo {
  // existing fields
  last_error: string;
}
```

```svelte
{:else if room.last_error}
  <span title={room.last_error}>拉流失败，正在重试</span>
{:else if room.room_info.status}
  <span>直播进行中</span>
{/if}
```

- [ ] **Step 2: Implement the retry state**

Update `RecorderInfo`, implement the helper, and use it from `Room.svelte`. A live room with `last_error` shows the retry state; recording remains first; generic live remains the fallback.

- [ ] **Step 3: Verify and commit**

Run the focused frontend test and `yarn check`.

Expected: both PASS.

```bash
git add src/lib/interface.ts src/lib/recorderStatus.ts src/lib/recorderStatus.test.ts src/page/Room.svelte
git commit -m "fix: show douyin pull retry status"
```

### Task 4: Restart and verify local recording

**Files:** No source changes.

- [ ] **Step 1: Start the frontend and desktop app**

Run: `yarn dev -- --host 127.0.0.1 --port 8054 --strictPort`

Run: `src-tauri/target/debug/bili-shadowreplay.exe`

Expected: Vite listens on port 8054 and the app window loads.

- [ ] **Step 2: Verify active room state**

Wait 15 seconds for a configured live Douyin room. Expect red `录制中` plus a new record row, or `拉流失败，正在重试` with a concrete non-sensitive reason. A known failure must not remain only `直播进行中`.

- [ ] **Step 3: Run final verification**

Run: `cargo test -p recorder && yarn check`

Expected: PASS.
