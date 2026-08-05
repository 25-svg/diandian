# Stable TS Playback Conversion Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Automatically create browser-playable MP4 copies for TS imports without remounting the player during conversion.

**Architecture:** A small pure frontend helper chooses one of three player presentations: stable preparing placeholder, playable video, or manual retry. `ArchiveAnalysis.svelte` uses it for both player layouts and automatically queues the existing backend conversion command once per selected video. The existing backend task deduplication remains the authority for concurrent calls.

**Tech Stack:** Svelte 3, TypeScript, Tauri invoke, Rust backend task queue, Node ts-node tests.

## Global Constraints

- Do not set the HTML video source to raw TS while conversion is pending or processing.
- Conversion completion changes the playable source exactly once; do not autoplay.
- Keep original TS and completed MP4 sidecar files.
- Reuse the existing `prepare_video_playback` command and backend duplicate-task protection.
- Do not modify unrelated dirty files.

---

### Task 1: Model stable playback presentation

**Files:**
- Create: `src/lib/playbackPresentation.ts`
- Create: `src/lib/playbackPresentation.test.ts`

**Interfaces:**
- Consumes: `{ requiresPreparation: boolean; ready: boolean; preparing: boolean }`.
- Produces: `playbackPresentation(source)` returning `"player" | "preparing" | "retry"`, and `shouldAutoPreparePlayback(source)` returning a boolean.

- [ ] **Step 1: Write the failing test**

```ts
assert.equal(
  playbackPresentation({ requiresPreparation: true, ready: false, preparing: true }),
  "preparing",
);
assert.equal(
  playbackPresentation({ requiresPreparation: true, ready: true, preparing: false }),
  "player",
);
assert.equal(
  shouldAutoPreparePlayback({ requiresPreparation: true, ready: false, preparing: false }),
  true,
);
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `node --loader ts-node/esm src/lib/playbackPresentation.test.ts`

Expected: FAIL because `playbackPresentation` does not exist.

- [ ] **Step 3: Write minimal implementation**

```ts
export function playbackPresentation(source: PlaybackSourceState | null): PlaybackPresentation {
  if (source?.requiresPreparation && !source.ready) {
    return source.preparing ? "preparing" : "retry";
  }
  return "player";
}

export function shouldAutoPreparePlayback(source: PlaybackSourceState): boolean {
  return source.requiresPreparation && !source.ready && !source.preparing;
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `node --loader ts-node/esm src/lib/playbackPresentation.test.ts`

Expected: PASS.

### Task 2: Automatically queue conversion and render one stable state

**Files:**
- Modify: `src/page/ArchiveAnalysis.svelte`

**Interfaces:**
- Consumes: `playbackPresentation` from Task 1 and existing `prepare_video_playback` Tauri command.
- Produces: automatic task scheduling only for a source requiring preparation; stable `preparing` state until the existing poll reports `ready`.

- [ ] **Step 1: Implement automatic stable conversion**

In `loadVideoPlaybackSource`, when the source needs preparation but has no active task, call `prepareVideoForPlayback()` after state assignment. In both player layouts, render the stable placeholder when `playbackPresentation(playbackSource) !== "player"`; render `video` only when it is `"player"` and `videoPlayerUrl` is non-empty. Keep poll updates limited to status text and switch URL only after `ready`.

- [ ] **Step 2: Run focused checks**

Run: `node --loader ts-node/esm src/lib/playbackPresentation.test.ts && yarn check`

Expected: all PASS.

- [ ] **Step 3: Manual verification**

Import one TS file. Expect immediate stable `正在生成可播放版本` state, no raw TS playback attempt, one background conversion task, and a playable MP4 after completion.
