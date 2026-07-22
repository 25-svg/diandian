import assert from "node:assert/strict";
import test from "node:test";
import {
  correctionBusinessReason,
  correctionCategoryLabel,
  correctionEditIdentity,
  correctionProgress,
  correctionSelectionTarget,
  canContinueAnalysis,
  createTranscriptReviewClient,
  analysisRequestIdentity,
  analysisWorkflowStage,
  firstPendingCorrectionId,
  highlightDiscoveryAction,
  highlightReviewGate,
  candidateReviewIdentity,
  isCurrentCandidateReview,
  isCurrentCandidateGeneration,
  transcriptReviewLock,
  isLatestRequestResult,
  isCurrentAnalysisRequest,
  isUnresolvedTranscriptPlaceholder,
  reviewStatusLabel,
  type InvokeFunction,
  type TranscriptAuditBundle,
  type TranscriptSource,
} from "./transcriptReview.js";

test("progress counts every explicit decision", () => {
  const corrections = [
    { id: "a", decision: "approved" },
    { id: "b", decision: "kept_original" },
    { id: "c", decision: "pending" },
  ] as const;

  assert.deepEqual(correctionProgress(corrections), { completed: 2, total: 3 });
  assert.equal(firstPendingCorrectionId(corrections), "c");
  assert.equal(firstPendingCorrectionId(corrections.slice(0, 2)), null);
});

test("business categories are normalized into novice language", () => {
  assert.equal(correctionCategoryLabel("product_model"), "商品型号");
  assert.equal(correctionCategoryLabel("商品型号 + MiniMax商品型号"), "商品型号");
  assert.equal(correctionCategoryLabel("price"), "价格");
  assert.equal(correctionBusinessReason("价格"), "价格直接影响成交判断，必须以直播原话为准。");
  assert.equal(correctionCategoryLabel("unknown_internal_code"), "关键信息");
  assert.equal(
    correctionBusinessReason("product_model"),
    "商品型号会影响客户判断和后续话术，建议逐字确认。",
  );
});

test("correction selection seeks to its start and finds the overlapping cue", () => {
  assert.deepEqual(
    correctionSelectionTarget(
      { startMs: 2_120, endMs: 2_900 },
      [
        { id: 10, start: 0, end: 2 },
        { id: 11, start: 2.1, end: 3.4 },
      ],
    ),
    { seekSeconds: 2.12, transcriptEntryId: 11 },
  );
  assert.deepEqual(
    correctionSelectionTarget({ startMs: -10, endMs: 0 }, []),
    { seekSeconds: 0, transcriptEntryId: null },
  );
});

test("correction selection treats adjacent transcript cues as half-open ranges", () => {
  assert.deepEqual(
    correctionSelectionTarget(
      { startMs: 2_000, endMs: 2_500 },
      [
        { id: 20, start: 1, end: 2 },
        { id: 21, start: 2, end: 3 },
      ],
    ),
    { seekSeconds: 2, transcriptEntryId: 21 },
  );
});

test("only the latest request for the active source or filter may update state", () => {
  assert.equal(isLatestRequestResult(4, 4, "video:9", "video:9"), true);
  assert.equal(isLatestRequestResult(3, 4, "video:9", "video:9"), false);
  assert.equal(isLatestRequestResult(4, 4, "video:8", "video:9"), false);
});

test("correction edit identity changes with source and correction content", () => {
  const correction = {
    id: "same-id",
    original: "A4PRO299新",
    proposed: "A4PRO2 99新",
    decidedText: null,
    decision: "pending" as const,
  };
  const originalIdentity = correctionEditIdentity("video:1", correction);

  assert.notEqual(correctionEditIdentity("video:2", correction), originalIdentity);
  assert.notEqual(
    correctionEditIdentity("video:1", { ...correction, proposed: "Ace Pro 2 99新" }),
    originalIdentity,
  );
});

test("critical pending corrections block analysis", () => {
  assert.equal(canContinueAnalysis({ pendingCriticalCount: 1 }), false);
  assert.equal(canContinueAnalysis({ pendingCriticalCount: 0 }), true);
});

test("workflow stage waits for recognition and routes critical review before discovery", () => {
  assert.equal(analysisWorkflowStage({
    hasTranscript: false,
    isRecognizing: true,
    auditState: "legacy-empty",
    pendingCriticalCount: 0,
  }), "recognizing");
  assert.equal(analysisWorkflowStage({
    hasTranscript: true,
    isRecognizing: false,
    auditState: "loading",
    pendingCriticalCount: 0,
  }), "recognizing");
  assert.equal(analysisWorkflowStage({
    hasTranscript: true,
    isRecognizing: false,
    auditState: "loaded",
    pendingCriticalCount: 1,
  }), "proofreading");
  assert.equal(analysisWorkflowStage({
    hasTranscript: true,
    isRecognizing: false,
    auditState: "loaded",
    pendingCriticalCount: 0,
  }), "discovering_highlights");
});

test("non-critical and legacy no-audit transcripts remain eligible for discovery", () => {
  assert.equal(analysisWorkflowStage({
    hasTranscript: true,
    isRecognizing: false,
    auditState: "loaded",
    pendingCriticalCount: 0,
  }), "discovering_highlights");
  assert.equal(analysisWorkflowStage({
    hasTranscript: true,
    isRecognizing: false,
    auditState: "legacy-empty",
    pendingCriticalCount: 0,
  }), "discovering_highlights");
});

test("audit load errors fail closed while backend legacy-empty bundles remain compatible", () => {
  const base = {
    hasTranscript: true,
    isRecognizing: false,
    pendingCriticalCount: 0,
  };
  assert.equal(analysisWorkflowStage({ ...base, auditState: "error" }), "proofreading");
  assert.equal(analysisWorkflowStage({ ...base, auditState: "legacy-empty" }), "discovering_highlights");
});

test("analysis request identity rejects stale source, source revision, transcript, and operation", () => {
  const active = analysisRequestIdentity("video:9", 4, 7);
  assert.equal(isCurrentAnalysisRequest(active, active, 3, 3), true);
  assert.equal(isCurrentAnalysisRequest(
    analysisRequestIdentity("video:8", 4, 7), active, 3, 3,
  ), false);
  assert.equal(isCurrentAnalysisRequest(
    analysisRequestIdentity("video:9", 3, 7), active, 3, 3,
  ), false);
  assert.equal(isCurrentAnalysisRequest(
    analysisRequestIdentity("video:9", 4, 6), active, 3, 3,
  ), false);
  assert.equal(isCurrentAnalysisRequest(active, active, 2, 3), false);
});

test("discovery action requires explicit continue after critical review", () => {
  const base = {
    stage: "discovering_highlights" as const,
    sourceKey: "video:9",
    requestedSourceKey: "video:9",
    discoveryCompleted: false,
    isDiscovering: false,
  };
  assert.equal(highlightDiscoveryAction({ ...base, hadPendingCriticalReview: false }), "auto");
  assert.equal(highlightDiscoveryAction({ ...base, hadPendingCriticalReview: true }), "continue");
  assert.equal(highlightDiscoveryAction({
    ...base,
    hadPendingCriticalReview: true,
    discoveryCompleted: true,
  }), "rerun");
});

test("discovery action blocks stale sources, proofreading, and in-flight work", () => {
  const base = {
    stage: "discovering_highlights" as const,
    sourceKey: "archive:douyin:room:live-2",
    requestedSourceKey: "archive:douyin:room:live-2",
    hadPendingCriticalReview: false,
    discoveryCompleted: false,
    isDiscovering: false,
  };
  assert.equal(highlightDiscoveryAction({
    ...base,
    requestedSourceKey: "archive:douyin:room:live-1",
  }), "blocked");
  assert.equal(highlightDiscoveryAction({ ...base, stage: "proofreading" }), "blocked");
  assert.equal(highlightDiscoveryAction({ ...base, isDiscovering: true }), "blocked");
});

test("single-candidate review is allowed only after gated discovery completes", () => {
  assert.deepEqual(highlightReviewGate("recognizing", false, false), {
    allowed: false,
    message: "请先等待逐字稿和校稿读取完成。",
  });
  assert.deepEqual(highlightReviewGate("proofreading", true, false), {
    allowed: false,
    message: "请先完成校稿确认，再开始片段复盘。",
  });
  assert.deepEqual(highlightReviewGate("discovering_highlights", false, false), {
    allowed: false,
    message: "请先完成高光片段识别。",
  });
  assert.deepEqual(highlightReviewGate("discovering_highlights", true, true), {
    allowed: false,
    message: "正在重新识别高光片段，请稍等。",
  });
  assert.deepEqual(highlightReviewGate("discovering_highlights", true, false), {
    allowed: true,
    message: "",
  });
});

test("transcript review lock uses novice read-only guidance during recognition", () => {
  assert.deepEqual(transcriptReviewLock(true), {
    disabled: true,
    reason: "正在重新识别逐字稿，请完成后再确认校稿。",
  });
  assert.deepEqual(transcriptReviewLock(false), { disabled: false, reason: "" });
});

test("candidate review identity rejects old generations even when candidate IDs repeat", () => {
  const oldReview = candidateReviewIdentity(4, "C01");
  const currentReview = candidateReviewIdentity(5, "C01");
  assert.equal(isCurrentCandidateReview(oldReview, 5, ["C01"], false), false);
  assert.equal(isCurrentCandidateReview(currentReview, 5, ["C01"], true), false);
  assert.equal(isCurrentCandidateReview(currentReview, 5, ["C02"], false), false);
  assert.equal(isCurrentCandidateReview(currentReview, 5, ["C01", "C02"], false), true);
  assert.equal(isCurrentCandidateGeneration(4, 5), false);
  assert.equal(isCurrentCandidateGeneration(5, 5), true);
});

test("review labels use novice language", () => {
  assert.equal(reviewStatusLabel("pending"), "等你确认");
  assert.equal(reviewStatusLabel("approved"), "已采用修改");
  assert.equal(reviewStatusLabel("kept_original"), "已保留原文");
});

test("unresolved transcript placeholders cannot be approved", () => {
  assert.equal(isUnresolvedTranscriptPlaceholder("[待确认]"), true);
  assert.equal(isUnresolvedTranscriptPlaceholder("  [待确认]  "), true);
  assert.equal(isUnresolvedTranscriptPlaceholder("[到手价待确认：识别为5830]"), true);
  assert.equal(isUnresolvedTranscriptPlaceholder("[听不清]"), true);
  assert.equal(isUnresolvedTranscriptPlaceholder("[疑似影石 Ace Pro 2]"), true);
  assert.equal(isUnresolvedTranscriptPlaceholder("影石 Ace Pro 2 99新"), false);
  assert.equal(isUnresolvedTranscriptPlaceholder("待确认后回复"), false);
});

test("audit loading preserves archive and video source serialization", async () => {
  const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
  const archive: TranscriptSource = {
    kind: "archive",
    platform: "douyin",
    roomId: "room-7",
    liveId: "live-8",
  };
  const video: TranscriptSource = { kind: "video", videoId: 42 };
  const bundle = (source: TranscriptSource): TranscriptAuditBundle => ({
    source,
    rawSrt: "raw",
    correctedSrt: "corrected",
    corrections: [],
    pendingCriticalCount: 0,
  });
  const invoke: InvokeFunction = async (command, args) => {
    calls.push({ command, args });
    return bundle((args as { source: TranscriptSource }).source);
  };
  const client = createTranscriptReviewClient(invoke);

  assert.deepEqual(await client.getTranscriptAudit(archive), bundle(archive));
  assert.deepEqual(await client.getTranscriptAudit(video), bundle(video));
  assert.deepEqual(calls, [
    { command: "get_transcript_audit", args: { source: archive } },
    { command: "get_transcript_audit", args: { source: video } },
  ]);
});

test("correction resolution uses the exact command and camelCase request", async () => {
  const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
  const expected = { pendingCriticalCount: 0 } as TranscriptAuditBundle;
  const invoke: InvokeFunction = async (command, args) => {
    calls.push({ command, args });
    return expected;
  };
  const client = createTranscriptReviewClient(invoke);
  const request = {
    source: { kind: "video", videoId: 9 } as const,
    correctionId: "correction-9",
    action: "approve" as const,
    decidedText: "影石 Ace Pro 2，99新",
    addToDictionaryCandidates: true,
  };

  assert.equal(await client.resolveTranscriptCorrection(request), expected);
  assert.deepEqual(calls, [
    { command: "resolve_transcript_correction", args: { request } },
  ]);
});

test("candidate adapters use exact commands and normalize the Rust row", async () => {
  const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
  const wireRow = {
    id: 3,
    candidate_type: "replacement",
    source_text: "A4PRO299新",
    target_text: "Ace Pro 2，99新",
    evidence_json: "[]",
    source_json: '{"sourceKind":"video"}',
    status: "pending",
    created_at: "2026-07-21 10:00:00",
    updated_at: "2026-07-21 10:00:00",
  } as const;
  const approvedWireRow = { ...wireRow, status: "approved" as const };
  const invoke: InvokeFunction = async (command, args) => {
    calls.push({ command, args });
    if (command === "list_transcript_dictionary_candidates") return [wireRow];
    if (command === "set_transcript_dictionary_candidate_status") return approvedWireRow;
    if (command === "export_transcript_dictionary_candidates") return "exported";
    throw new Error(`unexpected command: ${command}`);
  };
  const client = createTranscriptReviewClient(invoke);

  assert.deepEqual(await client.listDictionaryCandidates("pending"), [{
    id: 3,
    candidateType: "replacement",
    sourceText: "A4PRO299新",
    targetText: "Ace Pro 2，99新",
    evidenceJson: "[]",
    sourceJson: '{"sourceKind":"video"}',
    status: "pending",
    createdAt: "2026-07-21 10:00:00",
    updatedAt: "2026-07-21 10:00:00",
  }]);
  assert.equal((await client.setDictionaryCandidateStatus(3, "approved")).status, "approved");
  assert.equal(await client.exportDictionaryCandidates("json"), "exported");
  assert.equal(await client.exportDictionaryCandidates("csv"), "exported");
  assert.deepEqual(calls, [
    { command: "list_transcript_dictionary_candidates", args: { status: "pending" } },
    { command: "set_transcript_dictionary_candidate_status", args: { id: 3, status: "approved" } },
    { command: "export_transcript_dictionary_candidates", args: { format: "json" } },
    { command: "export_transcript_dictionary_candidates", args: { format: "csv" } },
  ]);
});

test("candidate listing sends null when no status filter is selected", async () => {
  const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
  const invoke: InvokeFunction = async (command, args) => {
    calls.push({ command, args });
    return [];
  };

  await createTranscriptReviewClient(invoke).listDictionaryCandidates();
  assert.deepEqual(calls, [
    { command: "list_transcript_dictionary_candidates", args: { status: null } },
  ]);
});
