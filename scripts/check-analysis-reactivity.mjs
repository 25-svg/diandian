import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { compile, preprocess } from "svelte/compiler";
import sveltePreprocess from "svelte-preprocess";

const componentSource = readFileSync(
  new URL("../src/page/ArchiveAnalysis.svelte", import.meta.url),
  "utf8",
);
const processed = await preprocess(
  componentSource,
  sveltePreprocess({ typescript: true }),
  { filename: "src/page/ArchiveAnalysis.svelte" },
);
const { js } = compile(processed.code, {
  filename: "src/page/ArchiveAnalysis.svelte",
  generate: "dom",
});
const updateStart = js.code.indexOf("$$self.$$.update = () => {");

assert.notEqual(
  updateStart,
  -1,
  "ArchiveAnalysis must have a reactive update function",
);

const updateCode = js.code.slice(updateStart);
assert.match(
  updateCode,
  /currentSource\s*=\s*sourceIdentity\(archive,\s*video\)/,
  "currentSource must explicitly depend on archive and video prop changes",
);

assert.match(
  componentSource,
  /auditLoadStatus\s*=\s*"error"/,
  "audit load failures must enter an explicit fail-closed error state",
);
const transcriptUpdateCode = componentSource.slice(
  componentSource.indexOf("function updateTranscriptReview"),
  componentSource.indexOf("async function regenerateAndAnalyze"),
);
assert.ok(
  transcriptUpdateCode.indexOf("if (isTranscribing) return")
    < transcriptUpdateCode.indexOf("reviewRequestToken"),
  "the parent must reject transcript review events while recognition is active",
);
assert.ok(
  transcriptUpdateCode.indexOf("reviewRequestToken")
    < transcriptUpdateCode.indexOf("isCurrentAnalysisRequest"),
  "transcript review events must carry a parent-owned request token and transcript identity",
);
assert.match(
  componentSource,
  /async function reviewSelectedCandidate[\s\S]{0,500}candidateReviewGate\.allowed/,
  "every candidate review entry point must converge on the workflow gate",
);

const panelSource = readFileSync(
  new URL("../src/lib/components/analysis/TranscriptReviewPanel.svelte", import.meta.url),
  "utf8",
);
assert.match(
  panelSource,
  /const submissionIdentity[\s\S]{0,300}const submissionToken[\s\S]*?await resolveTranscriptCorrection[\s\S]*?isCurrentAnalysisRequest[\s\S]*?dispatch\("update"/,
  "the review panel must validate stale submissions before dispatching results",
);
assert.doesNotMatch(
  panelSource,
  /bundle\s*=\s*updated/,
  "the review panel must not show local success before parent token validation",
);
assert.match(
  panelSource,
  /if \(!currentCorrection \|\| submitting \|\| disabled\) return/,
  "the review panel must reject new decisions while recognition makes it read-only",
);
assert.match(
  componentSource,
  /await get_static_url[\s\S]{0,500}isCurrentInitializationRequest/,
  "initialization must validate its source token after resolving the video URL",
);
const initializationCode = componentSource.slice(
  componentSource.indexOf("async function initialize"),
  componentSource.indexOf("async function loadActiveMaster"),
);
const playbackLookupCode = componentSource.slice(
  componentSource.indexOf("async function loadVideoPlaybackSource"),
  componentSource.indexOf("async function loadActiveMaster"),
);
const playbackErrorCode = componentSource.slice(
  componentSource.indexOf("async function handleVideoElementError"),
  componentSource.indexOf("async function prepareVideoForPlayback"),
);
assert.match(
  initializationCode,
  /void loadVideoPlaybackSource\(selectedVideo, requestIdentity, requestId\)/,
  "video playback lookup must not block the company analysis workspace when the database pool is busy",
);
assert.match(
  initializationCode,
  /analysisMode === "company_deal"\s*\?\s*void loadActiveMaster\(\)\s*:\s*await loadActiveMaster\(\)/,
  "company analysis must render before optional master-script lookup completes",
);
assert.match(
  playbackLookupCode,
  /if \(nextPlaybackSource\.preparing\)\s*\{\s*void observeVideoPlaybackUntilReady/,
  "an existing user-started playback task may be observed",
);
assert.doesNotMatch(
  playbackLookupCode,
  /else\s*\{\s*void prepareVideoForPlayback\(\);\s*\}/,
  "opening an analysis page must never start a playback conversion",
);
assert.doesNotMatch(
  playbackErrorCode,
  /prepareVideoForPlayback\(\)/,
  "a decode error must show the manual conversion action instead of repeatedly restarting FFmpeg",
);
assert.match(
  componentSource,
  /let playbackLoadError\s*=\s*""[\s\S]*?let transcriptLoadError\s*=\s*""/,
  "media failures must have local player and transcript error state",
);
assert.match(
  componentSource,
  /const TASK_POLL_MAX_ATTEMPTS\s*=\s*120/,
  "media task observation must be bounded instead of waiting forever",
);
assert.match(
  componentSource,
  /playbackLoadError\s*=\s*`播放器暂不可用：/,
  "player lookup failure must not replace the global page status",
);
assert.match(
  componentSource,
  /async function reviewSelectedCandidate[\s\S]*?async function copyText/,
  "reviewSelectedCandidate must remain an independently auditable async boundary",
);
const reviewCode = componentSource.slice(
  componentSource.indexOf("async function reviewSelectedCandidate"),
  componentSource.indexOf("async function copyText"),
);
const transcriptRefreshCode = componentSource.slice(
  componentSource.indexOf("async function smartRefreshTranscript"),
  componentSource.indexOf("async function loadTranscriptReview"),
);
assert.ok(
  transcriptRefreshCode.indexOf("invalidateTranscriptBoundRequests()")
    < transcriptRefreshCode.indexOf("await invoke"),
  "re-recognition must invalidate review submissions before its first await",
);
const discoveryCode = componentSource.slice(
  componentSource.indexOf("async function discoverCandidates"),
  componentSource.indexOf("function candidateTranscript"),
);
assert.ok(
  discoveryCode.indexOf("candidateGeneration += 1")
    < discoveryCode.indexOf("isDiscovering = true")
    && discoveryCode.indexOf("isDiscovering = true") < discoveryCode.indexOf("await invoke"),
  "rediscovery must create a generation, cancel reviews, and mark busy before awaiting",
);
assert.match(
  reviewCode,
  /isCurrentCandidateReview/,
  "candidate review responses must bind candidate generation and candidate id",
);
assert.ok(
  reviewCode.indexOf('await invoke<string>("minimax_chat"')
    < reviewCode.indexOf("isCurrentAnalysisRequest"),
  "review model responses must be bound to source and transcript identity",
);
assert.ok(
  transcriptUpdateCode.indexOf("invalidateTranscriptBoundRequests()")
    < transcriptUpdateCode.indexOf("transcript = bundle.correctedSrt"),
  "corrected transcript updates must invalidate discovery and review before replacement",
);

assert.match(
  componentSource,
  /playerFrameStatus\s*=\s*"loading"[\s\S]*?setTimeout[\s\S]*?playerFrameStatus\s*=\s*"error"/,
  "embedded archive playback must fail closed behind a bounded connection timeout",
);
assert.match(
  componentSource,
  /event\.source\s*!==\s*playerFrame\.contentWindow/,
  "embedded player readiness messages must be accepted only from the active iframe",
);
assert.match(
  componentSource,
  /class:is-ready=\{playerFrameStatus === "ready"\}[\s\S]*?重新连接/,
  "the raw iframe must stay hidden until ready and expose an in-app reconnect action on failure",
);

const appLiveSource = readFileSync(
  new URL("../src/AppLive.svelte", import.meta.url),
  "utf8",
);
assert.match(
  appLiveSource,
  /bsr:embed-shell-ready[\s\S]*?on:playbackReady[\s\S]*?bsr:embed-playback-error/,
  "the embedded live page must report shell readiness plus playback success or failure",
);

const playerSource = readFileSync(
  new URL("../src/lib/components/Player.svelte", import.meta.url),
  "utf8",
);
assert.match(
  playerSource,
  /dispatch\("playbackReady"\)[\s\S]*?dispatch\("playbackError"/,
  "the media player must expose deterministic ready and error events to its embedding page",
);
assert.match(
  playerSource,
  /if \(embedded\)[\s\S]*?Avoid browser alerts or reload loops inside the iframe/,
  "embedded playback failures must not show a browser alert or enter a reload loop",
);

console.log("ArchiveAnalysis source reactivity check passed");
