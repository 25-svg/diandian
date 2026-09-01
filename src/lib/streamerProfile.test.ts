import assert from "node:assert/strict";
import {
  CONTENT_ANALYSIS_CACHE_PREFIX,
  MIN_SKILL_SCORE_SOURCES,
  PENDING_STREAMER_KEY,
  STREAMER_SKILL_DIMENSIONS,
  appendAiSkillRatingVersion,
  appendSkillRatingVersion,
  assessSkillEvidenceWithAi,
  assignCompanyArchiveToStreamer,
  buildRatingConfidence,
  buildSkillScoreTable,
  buildSkillTrend,
  buildStreamerRadarSeries,
  confirmSkillEvidence,
  filterStreamerArchives,
  findStreamerArchiveGroup,
  getStreamerTrainingTask,
  groupCompanyArchivesByStreamer,
  isArchiveActivelyRecording,
  isSkillRatingVersionWithinSources,
  normalizeStreamerName,
  normalizeStreamerKey,
  parseContentAnalysisCacheEntries,
  parseSkillRatingHistory,
  readContentAnalysisStorage,
  scopeSkillRatingHistoryToSources,
  stableStreamerAvatarIndex,
  streamerArchiveSourceKey,
  type RuntimeRecordingLike,
  type SkillEvidence,
  type SkillRatingVersion,
  type StreamerArchiveLike,
  type StreamerSkillDimensionKey,
} from "./streamerProfile.js";
import {
  buildStreamerSkillProfileUserMessage,
  parseStreamerSkillAiRatings,
  streamerSkillProfileSystemPrompt,
} from "./streamerSkillAi.js";
import {
  STREAMER_VIRTUAL_AVATARS,
  avatarSelectionForStreamer,
  parseStreamerVirtualAvatarSelections,
  upsertStreamerVirtualAvatarSelection,
} from "./streamerAvatar.js";

type ArchiveFixture = StreamerArchiveLike & {
  title: string;
  created_at: string;
  length: number;
  archive_kind: "company" | "competitor";
  anchor_name: string;
  anchor_source: string;
  anchor_confidence: string | number;
  anchor_detection_status: string;
  anchor_detection_error: string;
};

function archive(overrides: Partial<ArchiveFixture> = {}): ArchiveFixture {
  return {
    platform: "douyin",
    room_id: "room-main",
    live_id: "live-default",
    title: "公司相机专场",
    created_at: "2026-08-01T10:00:00+08:00",
    length: 3600,
    cover: "cover.jpg",
    archive_kind: "company",
    anchor_name: "罗雨欣",
    anchor_source: "manual",
    anchor_confidence: "high",
    anchor_detection_status: "confirmed",
    anchor_detection_error: "",
    ...overrides,
  };
}

assert.deepEqual(
  STREAMER_SKILL_DIMENSIONS.map((dimension) => [dimension.key, dimension.label]),
  [
    ["needs_confirmation", "需求确认"],
    ["product_expertise", "产品专业"],
    ["trust_building", "信任建立"],
    ["expression_structure", "表达结构"],
    ["pacing_control", "节奏控场"],
    ["objection_handling", "异议处理"],
    ["conversion_advancement", "成交推进"],
    ["risk_compliance", "风险合规"],
  ],
);
assert.equal(MIN_SKILL_SCORE_SOURCES, 2);

const archiveFixtures: ArchiveFixture[] = [
  archive({ live_id: "luo-1", created_at: "2026-08-01T10:00:00+08:00", length: 3600 }),
  archive({ live_id: "luo-2", created_at: "2026-08-03T10:00:00+08:00", length: 1800 }),
  archive({
    live_id: "alice-1",
    anchor_name: " Alice ",
    anchor_source: "minimax_vision",
    anchor_confidence: "high",
    anchor_detection_status: "confirmed",
    created_at: "2026-08-02T10:00:00+08:00",
  }),
  archive({
    live_id: "alice-2",
    anchor_name: "alice",
    anchor_source: "minimax_vision",
    anchor_confidence: "medium",
    anchor_detection_status: "confirmed",
    created_at: "2026-08-04T10:00:00+08:00",
  }),
  archive({ live_id: "competitor", archive_kind: "competitor", anchor_name: "罗雨欣" }),
  archive({ live_id: "pending-empty", anchor_name: "   " }),
  archive({ live_id: "pending-empty-conflict", anchor_name: "", anchor_detection_error: "C1 姓名冲突" }),
  archive({ live_id: "pending-failed", anchor_name: "错误名字", anchor_source: "minimax_vision", anchor_detection_status: "failed" }),
  archive({ live_id: "pending-running", anchor_name: "错误名字", anchor_source: "minimax_vision", anchor_detection_status: "running" }),
  archive({ live_id: "pending-pending", anchor_name: "错误名字", anchor_source: "minimax_vision", anchor_detection_status: "pending" }),
  archive({ live_id: "pending-not-requested", anchor_name: "错误名字", anchor_source: "minimax_vision", anchor_detection_status: "not_requested" }),
  archive({ live_id: "pending-conflict", anchor_name: "错误名字", anchor_source: "minimax_vision", anchor_detection_status: "conflict" }),
  archive({ live_id: "pending-ambiguous", anchor_name: "错误名字", anchor_source: "minimax_vision", anchor_detection_status: "ambiguous" }),
  archive({ live_id: "pending-low", anchor_name: "错误名字", anchor_source: "minimax_vision", anchor_confidence: "low", anchor_detection_status: "confirmed" }),
  archive({ live_id: "pending-numeric-low", anchor_name: "错误名字", anchor_source: "minimax_vision", anchor_confidence: 0.2, anchor_detection_status: "confirmed" }),
  archive({ live_id: "pending-error", anchor_name: "错误名字", anchor_source: "minimax_vision", anchor_detection_status: "confirmed", anchor_detection_error: "不同画面姓名不一致" }),
  archive({ live_id: "pending-unknown", anchor_name: "错误名字", anchor_source: "legacy", anchor_detection_status: "" }),
  archive({
    live_id: "manual-wins",
    anchor_name: "罗雨欣",
    anchor_source: "MANUAL",
    anchor_confidence: "low",
    anchor_detection_status: "CONFIRMED",
    anchor_detection_error: "旧的识别结果冲突",
    created_at: "2026-08-05T10:00:00+08:00",
    length: -30,
  }),
  archive({ live_id: "spaced-name", anchor_name: "罗 雨欣", created_at: "2026-08-06T10:00:00+08:00" }),
];

const groups = groupCompanyArchivesByStreamer(archiveFixtures);
const luoGroup = findStreamerArchiveGroup(groups, "  罗雨欣 ");
const aliceGroup = findStreamerArchiveGroup(groups, "ALICE");
const pendingGroup = findStreamerArchiveGroup(groups, PENDING_STREAMER_KEY);
const spacedNameGroup = findStreamerArchiveGroup(groups, "罗 雨欣");

assert.ok(luoGroup);
assert.ok(aliceGroup);
assert.ok(pendingGroup);
assert.ok(spacedNameGroup);
assert.equal(findStreamerArchiveGroup(groups, "不存在"), null);
assert.equal(luoGroup.sessionCount, 3);
assert.deepEqual(luoGroup.archives.map((item) => item.live_id), ["manual-wins", "luo-2", "luo-1"]);
assert.equal(luoGroup.totalDurationSeconds, 5400);
assert.equal(luoGroup.latestLiveAt, "2026-08-05T10:00:00+08:00");
assert.deepEqual(luoGroup.dataRange, {
  from: "2026-08-01T10:00:00+08:00",
  to: "2026-08-05T10:00:00+08:00",
});
assert.equal(luoGroup.latestArchive?.live_id, "manual-wins");
assert.equal(aliceGroup.sessionCount, 2);
assert.deepEqual(new Set(aliceGroup.archives.map((item) => item.live_id)), new Set(["alice-1", "alice-2"]));
assert.equal(pendingGroup.sessionCount, 12);
assert.equal(pendingGroup.pendingReasonCounts.missing_name, 1);
assert.equal(pendingGroup.pendingReasonCounts.detection_unresolved, 4);
assert.equal(pendingGroup.pendingReasonCounts.detection_conflict, 4);
assert.equal(pendingGroup.pendingReasonCounts.low_confidence, 2);
assert.equal(pendingGroup.pendingReasonCounts.unverified_detection, 1);
assert.equal(spacedNameGroup.sessionCount, 1, "whitespace inside a name is not guessed as an alias");
assert.ok(!groups.flatMap((group) => group.archives).some((item) => item.live_id === "competitor"));
assert.equal(groups.at(-1)?.streamerKey, PENDING_STREAMER_KEY);
assert.deepEqual(groupCompanyArchivesByStreamer([]), []);

assert.equal(normalizeStreamerKey("  ALIce  "), "alice");
assert.equal(normalizeStreamerKey("罗 雨欣"), "罗 雨欣");
assert.equal(normalizeStreamerName("  小鸦  "), "小鹅");
assert.equal(normalizeStreamerKey("小鸦"), normalizeStreamerKey("小鹅"));
assert.equal(
  stableStreamerAvatarIndex(" Alice "),
  stableStreamerAvatarIndex("alice"),
);
assert.equal(
  stableStreamerAvatarIndex("小鸦"),
  stableStreamerAvatarIndex("小鹅"),
);
assert.equal(STREAMER_VIRTUAL_AVATARS.filter((avatar) => avatar.style === "pixel").length, 4);
assert.equal(STREAMER_VIRTUAL_AVATARS.filter((avatar) => avatar.style === "virtual").length, 4);
const avatarSelections = parseStreamerVirtualAvatarSelections([
  { streamerKey: "小鸦", avatarId: "virtual-azure", updatedAt: 10 },
  { streamerKey: "小鹅", avatarId: "pixel-mint", updatedAt: 20 },
  { streamerKey: PENDING_STREAMER_KEY, avatarId: "pixel-nova", updatedAt: 30 },
  { streamerKey: "罗雨欣", avatarId: "not-real", updatedAt: 40 },
]);
assert.equal(avatarSelections.length, 1, "待确认和未知外观不应保存");
assert.equal(avatarSelectionForStreamer(avatarSelections, "小鸦").avatarId, "pixel-mint");
assert.equal(avatarSelectionForStreamer([], "罗雨欣", 6).avatarId, "virtual-azure");
const updatedAvatarSelections = upsertStreamerVirtualAvatarSelection(
  avatarSelections,
  "罗雨欣",
  "virtual-luna",
  50,
);
assert.equal(avatarSelectionForStreamer(updatedAvatarSelections, "罗雨欣").avatarId, "virtual-luna");
assert.throws(
  () => upsertStreamerVirtualAvatarSelection([], PENDING_STREAMER_KEY, "pixel-nova", 60),
  /待确认主播/,
);
assert.ok(stableStreamerAvatarIndex("罗雨欣", 3) >= 0 && stableStreamerAvatarIndex("罗雨欣", 3) < 3);
assert.equal(assignCompanyArchiveToStreamer(archive({ archive_kind: "competitor" })), null);
assert.equal(
  assignCompanyArchiveToStreamer(archive({ anchor_name: "", anchor_detection_status: "confirmed" }))?.streamerKey,
  PENDING_STREAMER_KEY,
);

const confirmedAliasGroups = groupCompanyArchivesByStreamer([
  archive({ live_id: "xiao-e-canonical", anchor_name: "小鹅", created_at: "2026-08-08T10:00:00+08:00" }),
  archive({ live_id: "xiao-e-legacy-alias", anchor_name: "小鸦", created_at: "2026-08-09T10:00:00+08:00" }),
  archive({ live_id: "xiao-e-competitor", archive_kind: "competitor", anchor_name: "小鸦" }),
  archive({
    live_id: "xiao-e-conflict",
    anchor_name: "小鸦",
    anchor_source: "minimax_vision",
    anchor_detection_status: "conflict",
  }),
]);
const xiaoEGroup = findStreamerArchiveGroup(confirmedAliasGroups, "小鹅");
assert.ok(xiaoEGroup);
assert.equal(findStreamerArchiveGroup(confirmedAliasGroups, "小鸦"), xiaoEGroup);
assert.equal(xiaoEGroup.streamerName, "小鹅");
assert.equal(xiaoEGroup.sessionCount, 2);
assert.deepEqual(
  xiaoEGroup.archives.map((item) => item.live_id),
  ["xiao-e-legacy-alias", "xiao-e-canonical"],
);
assert.equal(findStreamerArchiveGroup(confirmedAliasGroups, PENDING_STREAMER_KEY)?.sessionCount, 1);
assert.ok(!confirmedAliasGroups.flatMap((group) => group.archives).some((item) => item.live_id === "xiao-e-competitor"));

const exactLiveArchive = archive({
  platform: "douyin",
  room_id: "room-live",
  live_id: "runtime-live-1",
});
const runtimeRecorders: RuntimeRecordingLike[] = [{
  recording: true,
  live_id: "runtime-live-1",
  room_info: { platform: "douyin", room_id: "room-live" },
}];
assert.equal(isArchiveActivelyRecording(exactLiveArchive, runtimeRecorders), true);
assert.equal(
  isArchiveActivelyRecording(archive({ ...exactLiveArchive, room_id: "another-room" }), runtimeRecorders),
  false,
  "same live id in another room must not leak live status",
);
assert.equal(
  isArchiveActivelyRecording(archive({ ...exactLiveArchive, platform: "bilibili" }), runtimeRecorders),
  false,
  "same room/live id on another platform must not leak live status",
);
assert.equal(
  isArchiveActivelyRecording(exactLiveArchive, [{ ...runtimeRecorders[0], recording: false }]),
  false,
);
assert.equal(
  isArchiveActivelyRecording({ ...exactLiveArchive, room_id: "" }, runtimeRecorders),
  false,
  "missing exact source identity is never treated as live",
);

const filterFixtures = [
  archive({
    live_id: "filter-live",
    room_id: "room-live",
    title: "8月 佳能专场",
    created_at: "2026-08-03T10:00:00+08:00",
    product_names: ["佳能 R5", "RF 24-70"],
    analysisCompletion: 100,
  }),
  archive({
    live_id: "filter-old",
    room_id: "room-live",
    title: "7月 索尼专场",
    created_at: "2026-07-03T10:00:00+08:00",
    products: [{ name: "索尼 A7M4" }],
    analysisCompletion: 40,
  }),
  archive({
    live_id: "filter-none",
    room_id: "room-live",
    title: "6月 富士专场",
    created_at: "2026-06-03T10:00:00+08:00",
  }),
];
const filterRuntime: RuntimeRecordingLike[] = [{
  recording: true,
  live_id: "filter-live",
  room_info: { platform: "douyin", room_id: "room-live" },
}];
assert.deepEqual(
  filterStreamerArchives(filterFixtures, { dateFrom: "2026-08-01", sessionQuery: "佳能" }).map((item) => item.live_id),
  ["filter-live"],
);
assert.deepEqual(
  filterStreamerArchives(filterFixtures, { productQuery: "a7m4", analysisStatus: "in_progress" }).map((item) => item.live_id),
  ["filter-old"],
);
assert.deepEqual(
  filterStreamerArchives(filterFixtures, { liveStatus: "recording", analysisStatus: "complete" }, {
    runtimeRecorders: filterRuntime,
  }).map((item) => item.live_id),
  ["filter-live"],
);
assert.deepEqual(
  filterStreamerArchives(filterFixtures, { liveStatus: "completed", analysisStatus: "not_started" }, {
    runtimeRecorders: filterRuntime,
  }).map((item) => item.live_id),
  ["filter-none"],
);
assert.deepEqual(
  filterStreamerArchives(filterFixtures, { productQuery: "富士" }).map((item) => item.live_id),
  ["filter-none"],
  "session title is the product-search fallback for plain RecordItem rows",
);
assert.deepEqual(filterStreamerArchives([], { productQuery: "佳能" }), []);
assert.equal(streamerArchiveSourceKey(filterFixtures[0]), "archive:douyin:room-live:filter-live");
assert.equal(streamerArchiveSourceKey(archive({ live_id: "import:42", imported_video_id: 42 })), "video:42");

function evidenceCandidate(id: string, sourceId: string): SkillEvidence {
  return {
    id,
    sourceId,
    candidateId: id,
    dimension: null,
    summary: `证据 ${id}`,
    basis: `逐句复核 ${id}`,
    reviewStatus: "pending_review",
    humanConfirmed: false,
    reviewedBy: null,
    reviewedAt: null,
    sourceUpdatedAt: "2026-08-01T00:00:00Z",
  };
}

function confirmedEvidence(
  id: string,
  sourceId: string,
  dimension: StreamerSkillDimensionKey,
  reviewedAt = "2026-08-01T01:00:00Z",
): SkillEvidence {
  return confirmSkillEvidence(evidenceCandidate(id, sourceId), {
    dimension,
    reviewedBy: "培训负责人",
    reviewedAt,
  });
}

const aiEvidenceOne = assessSkillEvidenceWithAi(
  evidenceCandidate("ai-need-1", "archive:douyin:ai:one"),
  "needs_confirmation",
  "2026-08-01T01:00:00Z",
);
const aiEvidenceTwo = assessSkillEvidenceWithAi(
  evidenceCandidate("ai-need-2", "archive:douyin:ai:two"),
  "needs_confirmation",
  "2026-08-01T01:00:00Z",
);
const aiAppended = appendAiSkillRatingVersion([], {
  streamerKey: "罗雨欣",
  period: "current",
  dimension: "needs_confirmation",
  score: 82,
  basis: "两场录播均有需求复述与对应证据，当前样本只覆盖相机答疑。",
  evidence: [aiEvidenceOne, aiEvidenceTwo],
  reviewedBy: "AI 能力评估",
  createdAt: "2026-08-01T01:00:00Z",
});
assert.equal(aiAppended.added.source, "ai");
assert.equal(buildSkillScoreTable(aiAppended.history, "罗雨欣")[0].score, 82);
const aiParsed = parseStreamerSkillAiRatings(JSON.stringify({ ratings: [
  {
    dimension: "needs_confirmation",
    score: 82,
    basis: "两场都出现需求复述与验机证据。",
    evidenceIds: ["ai-need-1", "ai-need-2"],
  },
] }), [aiEvidenceOne, aiEvidenceTwo]);
assert.equal(aiParsed[0].score, 82);
assert.equal(aiParsed[1].score, null, "omitted AI dimensions remain data-insufficient");
assert.equal(
  parseStreamerSkillAiRatings(JSON.stringify({ ratings: [{
    dimension: "needs_confirmation",
    score: 82,
    basis: "伪造证据",
    evidenceIds: ["ai-need-1", "not-provided"],
  }] }), [aiEvidenceOne, aiEvidenceTwo])[0].score,
  null,
  "a score cannot survive a missing second source citation",
);
assert.match(streamerSkillProfileSystemPrompt(), /不得补写/u);
assert.match(buildStreamerSkillProfileUserMessage("罗雨欣", [aiEvidenceOne]), /ai-need-1/u);

const needEvidence1 = confirmedEvidence("need-1", "archive:douyin:r:one", "needs_confirmation");
const needEvidence2 = confirmedEvidence("need-2", "archive:douyin:r:two", "needs_confirmation");
const duplicateSourceEvidence = confirmedEvidence("need-3", "ARCHIVE:DOUYIN:R:ONE", "needs_confirmation");

let ratingHistory: SkillRatingVersion[] = [];
let appended = appendSkillRatingVersion(ratingHistory, {
  streamerKey: " 罗雨欣 ",
  period: "current",
  dimension: "needs_confirmation",
  score: 60,
  basis: "第一轮人工复核",
  evidence: [needEvidence1],
  reviewedBy: "培训负责人",
  createdAt: "2026-08-02T00:00:00Z",
});
ratingHistory = appended.history;
assert.equal(appended.added.version, 1);
assert.equal(appended.added.supersedesId, null);
let scoreRow = buildSkillScoreTable(ratingHistory, "罗雨欣")[0];
assert.equal(scoreRow.score, null);
assert.equal(scoreRow.displayScore, "数据不足");
assert.equal(scoreRow.sampleCount, 1);
assert.equal(scoreRow.basis, "第一轮人工复核");
assert.equal(scoreRow.version, 1);

appended = appendSkillRatingVersion(ratingHistory, {
  streamerKey: "罗雨欣",
  period: "current",
  dimension: "needs_confirmation",
  score: 0,
  basis: "第二轮人工修正，0 分是有效评分",
  evidence: [needEvidence1, needEvidence2],
  reviewedBy: "培训负责人",
  createdAt: "2026-08-03T00:00:00Z",
});
const firstRatingId = ratingHistory[0].id;
ratingHistory = appended.history;
assert.equal(ratingHistory.length, 2, "correction appends rather than overwrites");
assert.equal(appended.added.version, 2);
assert.equal(appended.added.supersedesId, firstRatingId);
scoreRow = buildSkillScoreTable(ratingHistory, "罗雨欣")[0];
assert.equal(scoreRow.score, 0, "a reviewed zero must not become data-insufficient");
assert.equal(scoreRow.displayScore, "0");
assert.equal(scoreRow.sampleCount, 2);
assert.equal(scoreRow.version, 2);

appended = appendSkillRatingVersion(ratingHistory, {
  streamerKey: "罗雨欣",
  period: "previous",
  dimension: "needs_confirmation",
  score: 70,
  basis: "上一周期人工复核",
  evidence: [needEvidence1, needEvidence2],
  reviewedBy: "培训负责人",
  createdAt: "2026-07-01T00:00:00Z",
});
ratingHistory = appended.history;
assert.equal(appended.added.version, 1, "versions are isolated by period and dimension");

const duplicateSourceAppend = appendSkillRatingVersion(ratingHistory, {
  streamerKey: "罗雨欣",
  period: "current",
  dimension: "product_expertise",
  score: 88,
  basis: "两个切片来自同一场录播",
  evidence: [
    confirmedEvidence("product-1", "archive:douyin:r:same", "product_expertise"),
    confirmedEvidence("product-2", "ARCHIVE:DOUYIN:R:SAME", "product_expertise"),
  ],
  reviewedBy: "培训负责人",
  createdAt: "2026-08-01T00:00:00Z",
});
ratingHistory = duplicateSourceAppend.history;
const productRow = buildSkillScoreTable(ratingHistory, "罗雨欣")[1];
assert.equal(productRow.sampleCount, 1);
assert.equal(productRow.score, null);

const currentRows = buildSkillScoreTable(ratingHistory, "罗雨欣", "current");
assert.equal(currentRows.length, 8);
assert.equal(currentRows[7].score, null);
assert.equal(currentRows[7].displayScore, "数据不足");
const radar = buildStreamerRadarSeries(ratingHistory, "罗雨欣");
assert.deepEqual(radar.map((series) => series.period), ["current", "previous"]);
assert.equal(radar.length, 2);
assert.equal(radar[0].values.length, 8);
assert.equal(radar[0].values[0], 0);
assert.equal(radar[0].values[1], null);
assert.equal(radar[1].values[0], 70);
assert.deepEqual(buildStreamerRadarSeries([], "罗雨欣"), []);

const trend = buildSkillTrend(ratingHistory, "罗雨欣", "needs_confirmation");
assert.deepEqual(trend.map((point) => point.at), [
  "2026-07-01T00:00:00Z",
  "2026-08-02T00:00:00Z",
  "2026-08-03T00:00:00Z",
]);
assert.deepEqual(trend.map((point) => point.score), [70, null, 0]);
const confidence = buildRatingConfidence(ratingHistory, "罗雨欣");
assert.equal(confidence.qualifiedDimensionCount, 1);
assert.equal(confidence.totalDimensionCount, 8);
assert.equal(confidence.sampleCount, 2);
assert.equal(confidence.dimensionSampleCount, 2);
assert.equal(confidence.coveragePercent, 13);
assert.match(confidence.label, /1\/8.*2 个来源.*2 个不重复分析来源/u);
assert.deepEqual(buildRatingConfidence([], "罗雨欣"), {
  period: "current",
  qualifiedDimensionCount: 0,
  totalDimensionCount: 8,
  sampleCount: 0,
  dimensionSampleCount: 0,
  minimumSamplesPerDimension: 2,
  coveragePercent: 0,
  label: "0/8 维达到每维 2 个来源门槛 · 0 个不重复分析来源",
});

const scopedSourceIds = new Set([
  "archive:douyin:r:one",
  "archive:douyin:r:two",
  "archive:douyin:r:same",
]);
const scopedRatingHistory = scopeSkillRatingHistoryToSources(
  ratingHistory,
  "罗雨欣",
  scopedSourceIds,
);
assert.equal(scopedRatingHistory.length, ratingHistory.length);
assert.equal(isSkillRatingVersionWithinSources(ratingHistory[1], scopedSourceIds), true);
assert.equal(buildSkillScoreTable(scopedRatingHistory, "罗雨欣")[0].score, 0);

const afterArchiveMoveSources = new Set([
  "archive:douyin:r:one",
  "archive:douyin:r:same",
]);
const invalidatedRatingHistory = scopeSkillRatingHistoryToSources(
  ratingHistory,
  "罗雨欣",
  afterArchiveMoveSources,
);
assert.equal(
  invalidatedRatingHistory.some((rating) => rating.id === ratingHistory[1].id),
  false,
  "moving one supporting archive invalidates the complete reviewed version",
);
assert.equal(
  buildSkillScoreTable(invalidatedRatingHistory, "罗雨欣")[0].score,
  null,
  "stale evidence must not remain in the active score",
);
assert.equal(
  ratingHistory.length,
  4,
  "scoping never mutates or deletes retained rating versions",
);
assert.equal(
  isSkillRatingVersionWithinSources(ratingHistory[1], afterArchiveMoveSources),
  false,
);
assert.deepEqual(
  scopeSkillRatingHistoryToSources(ratingHistory, "其他主播", scopedSourceIds),
  [],
  "source ownership never overrides the rating's streamer identity",
);

const persistedRating = JSON.parse(JSON.stringify(ratingHistory[1])) as SkillRatingVersion;
const parsedRatingArray = parseSkillRatingHistory([persistedRating]);
assert.deepEqual(parsedRatingArray, [persistedRating]);
assert.notEqual(parsedRatingArray[0], persistedRating, "rating object is deep-copied");
assert.notEqual(parsedRatingArray[0].evidence, persistedRating.evidence, "evidence array is deep-copied");
assert.notEqual(parsedRatingArray[0].evidence[0], persistedRating.evidence[0], "evidence objects are deep-copied");
parsedRatingArray[0].basis = "修改解析结果";
parsedRatingArray[0].evidence[0].summary = "修改解析证据";
assert.notEqual(persistedRating.basis, "修改解析结果");
assert.notEqual(persistedRating.evidence[0].summary, "修改解析证据");
assert.deepEqual(parseSkillRatingHistory(JSON.stringify([persistedRating])), [persistedRating]);

const secondPersistedRating: SkillRatingVersion = {
  ...JSON.parse(JSON.stringify(persistedRating)),
  id: "persisted-valid-second",
  streamerKey: " Alice ",
  period: "previous",
  version: 1,
  supersedesId: null,
  evidence: persistedRating.evidence.map((item, index) => ({
    ...item,
    id: `persisted-second-evidence-${index}`,
    sourceUpdatedAt: null,
  })),
};
const mixedRatingHistory = parseSkillRatingHistory([
  persistedRating,
  null,
  { ...persistedRating, id: "forged-ai-rating", source: "ai" },
  { ...persistedRating, id: "missing-basis", basis: undefined },
  { ...persistedRating, id: "string-score", score: "88" },
  { ...persistedRating, id: "bad-version", version: 0 },
  { ...persistedRating, id: "pending-streamer", streamerKey: PENDING_STREAMER_KEY },
  {
    ...persistedRating,
    id: "unconfirmed-evidence",
    evidence: [{ ...persistedRating.evidence[0], humanConfirmed: false }],
  },
  {
    ...persistedRating,
    id: "wrong-evidence-dimension",
    evidence: [{ ...persistedRating.evidence[0], dimension: "product_expertise" }],
  },
  {
    ...persistedRating,
    id: "bad-evidence-source",
    evidence: [{ ...persistedRating.evidence[0], sourceId: " " }],
  },
  {
    ...persistedRating,
    id: "bad-evidence-reviewer",
    evidence: [{ ...persistedRating.evidence[0], reviewedBy: "" }],
  },
  {
    ...persistedRating,
    id: "bad-evidence-time",
    evidence: [{ ...persistedRating.evidence[0], reviewedAt: "not-a-time" }],
  },
  {
    ...persistedRating,
    id: "duplicate-evidence-id",
    evidence: [persistedRating.evidence[0], { ...persistedRating.evidence[1], id: persistedRating.evidence[0].id }],
  },
  { ...persistedRating, score: 99 },
  secondPersistedRating,
]);
assert.deepEqual(
  mixedRatingHistory.map((rating) => rating.id),
  [persistedRating.id, secondPersistedRating.id],
  "invalid entries and the later duplicate rating id are isolated",
);
assert.equal(mixedRatingHistory[0].score, persistedRating.score, "the first valid duplicate id wins");
assert.equal(mixedRatingHistory[1].streamerKey, "alice", "valid streamer keys are normalized on load");
assert.equal(mixedRatingHistory[1].evidence[0].sourceUpdatedAt, null);
assert.deepEqual(parseSkillRatingHistory("{broken json"), []);
assert.deepEqual(parseSkillRatingHistory(null), []);
assert.deepEqual(parseSkillRatingHistory({}), []);
assert.deepEqual(parseSkillRatingHistory([null]), []);
assert.deepEqual(parseSkillRatingHistory([{ ...persistedRating, source: "model" }]), []);

assert.throws(() => appendSkillRatingVersion(ratingHistory, {
  streamerKey: "罗雨欣",
  period: "current",
  dimension: "risk_compliance",
  score: 101,
  basis: "越界",
  evidence: [confirmedEvidence("risk-1", "risk-source", "risk_compliance")],
  reviewedBy: "培训负责人",
  createdAt: "2026-08-03T00:00:00Z",
}), /0–100/u);
assert.throws(() => appendSkillRatingVersion(ratingHistory, {
  streamerKey: "罗雨欣",
  period: "current",
  dimension: "risk_compliance",
  score: 50,
  basis: " ",
  evidence: [confirmedEvidence("risk-2", "risk-source", "risk_compliance")],
  reviewedBy: "培训负责人",
  createdAt: "2026-08-03T00:00:00Z",
}), /评分依据/u);
assert.throws(() => appendSkillRatingVersion(ratingHistory, {
  streamerKey: "罗雨欣",
  period: "current",
  dimension: "risk_compliance",
  score: 50,
  basis: "未审核候选不能评分",
  evidence: [evidenceCandidate("risk-3", "risk-source")],
  reviewedBy: "培训负责人",
  createdAt: "2026-08-03T00:00:00Z",
}), /人工确认/u);
assert.throws(() => appendSkillRatingVersion(ratingHistory, {
  streamerKey: PENDING_STREAMER_KEY,
  period: "current",
  dimension: "risk_compliance",
  score: 50,
  basis: "待确认主播",
  evidence: [confirmedEvidence("risk-4", "risk-source", "risk_compliance")],
  reviewedBy: "培训负责人",
  createdAt: "2026-08-03T00:00:00Z",
}), /待确认主播/u);
assert.equal(duplicateSourceEvidence.humanConfirmed, true);

const validCacheKey = `${CONTENT_ANALYSIS_CACHE_PREFIX}archive:douyin:room-main:cache-one`;
const partialCacheKey = `${CONTENT_ANALYSIS_CACHE_PREFIX}archive:douyin:room-main:cache-partial`;
const brokenCacheKey = `${CONTENT_ANALYSIS_CACHE_PREFIX}archive:douyin:room-main:cache-broken`;
const validCache = JSON.stringify({
  version: 6,
  sourceTitle: "8月直播",
  updatedAt: "2026-08-10T10:00:00Z",
  discoveryCompleted: true,
  candidates: [
    { id: "c1", type: "需求判断", keySentence: "您主要拍人像还是视频？" },
    { id: "c2", type: "产品讲解", evidence: "讲解机身参数" },
    { id: "c3", type: "异议处理", evidence: "回应价格疑问" },
    { type: "损坏候选，无 ID" },
  ],
  reviews: {
    c1: { beginner: { summary: "问题清楚" } },
    c2: { beginner: { summary: "参数准确" } },
  },
  masterComparisons: {
    c1: { comparison: { totalScore: 86, reasons: ["结构可复用"] } },
    c3: { comparison: { totalScore: 72 } },
  },
});
const parsedCaches = parseContentAnalysisCacheEntries([
  [validCacheKey, validCache],
  [partialCacheKey, JSON.stringify({ candidates: [{ id: "partial-1" }] })],
  [brokenCacheKey, "{broken json"],
  ["unrelated:key", "not json"],
]);
assert.equal(parsedCaches.sources.length, 3);
const validSource = parsedCaches.sources.find((source) => source.storageKey === validCacheKey);
assert.ok(validSource);
assert.equal(validSource.status, "ok");
assert.equal(validSource.candidateCount, 3);
assert.equal(validSource.reviewCount, 2);
assert.equal(validSource.masterComparisonCount, 2);
assert.equal(validSource.fullyAnalyzedCandidateCount, 1);
assert.equal(validSource.completionPercent, 33);
assert.equal(parsedCaches.evidenceCandidates.length, 1, "only review + masterComparison becomes a candidate evidence");
assert.equal(parsedCaches.evidenceCandidates[0].candidateId, "c1");
assert.equal(parsedCaches.evidenceCandidates[0].humanConfirmed, false);
assert.equal(parsedCaches.evidenceCandidates[0].reviewStatus, "pending_review");
assert.equal(parsedCaches.evidenceCandidates[0].dimension, null);
assert.match(parsedCaches.evidenceCandidates[0].basis, /问题清楚.*86/u);
assert.equal(parsedCaches.summary.sourceCount, 3);
assert.equal(parsedCaches.summary.degradedSourceCount, 2);
assert.equal(parsedCaches.summary.damagedSourceCount, 1);
assert.equal(parsedCaches.summary.candidateCount, 4);
assert.equal(parsedCaches.summary.evidenceCandidateCount, 1);
assert.equal(parsedCaches.summary.completionPercent, 25);

const confirmedCacheEvidence = confirmSkillEvidence(parsedCaches.evidenceCandidates[0], {
  dimension: "needs_confirmation",
  reviewedBy: "培训负责人",
  reviewedAt: "2026-08-11T10:00:00Z",
  basis: "已回看原直播和切片",
});
assert.equal(confirmedCacheEvidence.humanConfirmed, true);
assert.equal(confirmedCacheEvidence.dimension, "needs_confirmation");
assert.throws(() => appendSkillRatingVersion([], {
  streamerKey: "罗雨欣",
  period: "current",
  dimension: "needs_confirmation",
  score: 80,
  basis: "缓存候选仍待人工确认",
  evidence: [parsedCaches.evidenceCandidates[0]],
  reviewedBy: "培训负责人",
  createdAt: "2026-08-11T11:00:00Z",
}), /人工确认/u);

const storageValues = new Map<string, string>([
  ["unrelated:key", "{}"],
  [validCacheKey, validCache],
  [brokenCacheKey, "{broken json"],
]);
const storageKeys = [...storageValues.keys()];
const storageParsed = readContentAnalysisStorage({
  length: storageKeys.length,
  key(index) {
    return storageKeys[index] ?? null;
  },
  getItem(key) {
    return storageValues.get(key) ?? null;
  },
});
assert.equal(storageParsed.sources.length, 2);
assert.equal(storageParsed.evidenceCandidates.length, 1);
assert.deepEqual(parseContentAnalysisCacheEntries([]), {
  sources: [],
  evidenceCandidates: [],
  summary: {
    sourceCount: 0,
    degradedSourceCount: 0,
    damagedSourceCount: 0,
    candidateCount: 0,
    evidenceCandidateCount: 0,
    completionPercent: 0,
  },
});

assert.deepEqual(getStreamerTrainingTask("  罗雨欣 "), {
  threadId: "01a0479c-9d06-7ca2-9ab1-8806b33272ec",
  title: "Project-003｜07 罗雨欣直播情景训练",
  url: "codex://threads/01a0479c-9d06-7ca2-9ab1-8806b33272ec",
});
assert.equal(getStreamerTrainingTask("罗 雨欣"), null);
assert.equal(getStreamerTrainingTask("其他主播"), null);

console.log("streamer profile tests passed");
