import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const source = readFileSync(new URL("../src-tauri/src/recorder_manager.rs", import.meta.url), "utf8");
const start = source.indexOf("pub async fn remove_recorder(");
const end = source.indexOf("async fn load_playlist_bytes(", start);

assert.ok(start >= 0 && end > start, "remove_recorder function must be present");
const removeRecorder = source.slice(start, end);

assert.doesNotMatch(
  removeRecorder,
  /remove_dir_all\s*\(/,
  "removing a recorder must not delete historical archive cache",
);

console.log("recorder removal preserves archive cache check passed");
