import type { RecordItem } from "../../src/lib/db";
import type { RecorderInfo, RecorderList } from "../../src/lib/interface";
import {
  STREAMER_SKILL_DIMENSIONS,
  type SkillEvidence,
  type SkillRatingPeriod,
  type SkillRatingVersion,
  type StreamerSkillDimensionKey,
} from "../../src/lib/streamerProfile";

function archive(overrides: Partial<RecordItem> & Pick<RecordItem, "live_id" | "room_id" | "title">): RecordItem {
  return {
    platform: "douyin",
    parent_id: "",
    length: 7_200,
    size: 2_400_000_000,
    created_at: "2026-08-20T02:00:00.000Z",
    cover: "",
    anchor_name: "",
    anchor_source: "ocr",
    anchor_confidence: "high",
    anchor_detection_status: "confirmed",
    anchor_detection_error: "",
    anchor_detected_at: "2026-08-20T02:10:00.000Z",
    archive_kind: "company",
    classification_source: "manual",
    ...overrides,
  };
}

export const STREAMER_ARCHIVE_FIXTURE: RecordItem[] = [
  archive({
    live_id: "live-luo-001",
    room_id: "room-luo",
    title: "罗雨欣 S9 相机专场",
    anchor_name: "罗雨欣",
    created_at: "2026-08-20T02:00:00.000Z",
  }),
  archive({
    live_id: "live-luo-002",
    room_id: "room-luo",
    title: "罗雨欣 G9 新品答疑",
    anchor_name: "罗雨欣",
    created_at: "2026-08-27T06:00:00.000Z",
    length: 3_600,
    size: 1_200_000_000,
  }),
  archive({
    live_id: "live-zhang-001",
    room_id: "room-zhang",
    title: "张敏 S5M2 新人专场",
    anchor_name: "张敏",
    created_at: "2026-08-25T01:00:00.000Z",
    length: 5_400,
  }),
  archive({
    live_id: "live-unknown-001",
    room_id: "room-unknown",
    title: "S9 晚间专场",
    anchor_detection_status: "not_requested",
    anchor_confidence: "",
  }),
  archive({
    live_id: "live-conflict-001",
    room_id: "room-conflict",
    title: "G9 双机位专场",
    anchor_detection_status: "failed",
    anchor_detection_error: "多个画面中的主播姓名不一致",
    anchor_confidence: "",
  }),
  archive({
    live_id: "live-pending-001",
    room_id: "room-pending",
    title: "S5M2 午间专场",
    anchor_detection_status: "running",
    anchor_confidence: "",
  }),
  archive({
    live_id: "live-competitor-001",
    room_id: "room-competitor",
    title: "竞品 S9 对比专场",
    anchor_name: "罗雨欣",
    archive_kind: "competitor",
    created_at: "2026-08-26T03:00:00.000Z",
  }),
];

function recorder(roomId: string, userName: string, options: Partial<RecorderInfo> = {}): RecorderInfo {
  return {
    room_info: {
      platform: "douyin",
      room_id: roomId,
      room_title: `${userName || roomId}直播间`,
      room_cover: "",
      status: Boolean(options.recording),
    },
    user_info: {
      user_id: `user-${roomId}`,
      user_name: userName,
      user_avatar: "",
    },
    platform_live_id: options.platform_live_id || "",
    live_id: options.live_id || "",
    recording: Boolean(options.recording),
    enabled: options.enabled ?? true,
    current_streamer: options.current_streamer || "",
    current_streamer_source: options.current_streamer_source || "",
  };
}

export const STREAMER_RECORDER_FIXTURE: RecorderList = {
  count: 6,
  recorders: [
    recorder("room-luo", "公司相机店", { live_id: "live-luo-002", recording: true }),
    recorder("room-zhang", "公司相机店二号"),
    recorder("room-unknown", "公司相机店"),
    recorder("room-conflict", "公司相机店"),
    recorder("room-pending", "公司相机店"),
    recorder("room-competitor", "竞品观察"),
  ],
};

export const STREAMER_RECORDER_MISMATCH_FIXTURE: RecorderList = {
  count: STREAMER_RECORDER_FIXTURE.count,
  recorders: STREAMER_RECORDER_FIXTURE.recorders.map((item) =>
    item.room_info.room_id === "room-luo"
      ? { ...item, live_id: "similar-room-but-different-live", recording: true }
      : item
  ),
};

export function archivesForRoom(roomId: string, offset = 0, limit = 100): RecordItem[] {
  return STREAMER_ARCHIVE_FIXTURE
    .filter((item) => item.room_id === roomId)
    .slice(offset, offset + limit);
}

export const CONTENT_ANALYSIS_FIXTURE: Record<string, unknown> = {
  "archive:douyin:room-luo:live-luo-001": {
    sourceTitle: "罗雨欣 S9 相机专场",
    discoveryCompleted: true,
    candidates: [
      {
        id: "L1-C1",
        title: "先复述顾虑，再给验机证据",
        start: 120,
        end: 168,
        score: 91,
        summary: "确认顾客担心快门数后展示检测结果。",
      },
    ],
    reviews: {
      "L1-C1": {
        status: "approved",
        evidence: "录播 02:00–02:48，主播复述需求并展示检测单。",
        trainingChecklist: "继续练习需求复述和证据闭环。",
      },
    },
    masterComparisons: {
      "L1-C1": {
        comparison: {
          totalScore: 91,
          reasons: ["需求复述和检测证据均可回看。"],
        },
      },
    },
  },
  "archive:douyin:room-luo:live-luo-002": {
    sourceTitle: "罗雨欣 G9 新品答疑",
    discoveryCompleted: false,
    candidates: [
      {
        id: "L2-C1",
        title: "价格异议回应",
        start: 310,
        end: 350,
        score: 78,
        summary: "已解释价格构成，等待人工复核证据。",
      },
      {
        id: "L2-C2",
        title: "售后边界说明",
        start: 420,
        end: 458,
        score: 74,
        summary: "候选片段已发现，尚未完成逐段复盘。",
      },
    ],
    reviews: {
      "L2-C1": {
        status: "approved",
        evidence: "录播 05:10–05:50，主播解释价格构成并回应顾虑。",
        trainingChecklist: "继续练习价格异议中的价值分层。",
      },
    },
    masterComparisons: {
      "L2-C1": {
        comparison: {
          totalScore: 78,
          reasons: ["价格构成解释完整，可从原场次回看。"],
        },
      },
    },
  },
};

const LUO_SOURCE_IDS = [
  "archive:douyin:room-luo:live-luo-001",
  "archive:douyin:room-luo:live-luo-002",
] as const;

function reviewedEvidence(
  dimension: StreamerSkillDimensionKey,
  period: SkillRatingPeriod,
  sourceId: string,
  index: number,
): SkillEvidence {
  const reviewedAt = period === "current"
    ? "2026-08-28T08:00:00.000Z"
    : "2026-07-28T08:00:00.000Z";
  return {
    id: `fixture:${period}:${dimension}:${index + 1}`,
    sourceId,
    candidateId: `${period}-${dimension}-${index + 1}`,
    dimension,
    summary: `${dimension}固定验收证据 ${index + 1}`,
    basis: "人工逐条打开录播并核对片段与复盘记录。",
    reviewStatus: "human_confirmed",
    humanConfirmed: true,
    reviewedBy: "验收审核员",
    reviewedAt,
    sourceUpdatedAt: reviewedAt,
  };
}

function ratingVersion(
  dimension: StreamerSkillDimensionKey,
  period: SkillRatingPeriod,
  dimensionIndex: number,
  sourceIds: readonly string[],
): SkillRatingVersion {
  const createdAt = period === "current"
    ? "2026-08-28T08:00:00.000Z"
    : "2026-07-28T08:00:00.000Z";
  return {
    id: `fixture-rating:${period}:${dimension}:v1`,
    streamerKey: "罗雨欣",
    period,
    dimension,
    score: period === "current" ? 88 - dimensionIndex : 76 - dimensionIndex,
    basis: `${period === "current" ? "当前" : "上一"}周期人工评分依据`,
    evidence: sourceIds.map((sourceId, index) => reviewedEvidence(dimension, period, sourceId, index)),
    version: 1,
    supersedesId: null,
    reviewedBy: "验收审核员",
    createdAt,
    source: "manual",
  };
}

export const SINGLE_SOURCE_SKILL_RATING_HISTORY_FIXTURE: SkillRatingVersion[] = [
  ratingVersion("needs_confirmation", "current", 0, [LUO_SOURCE_IDS[0]]),
];

export const FULL_SKILL_RATING_HISTORY_FIXTURE: SkillRatingVersion[] = (
  ["current", "previous"] as const
).flatMap((period) => STREAMER_SKILL_DIMENSIONS.map((dimension, index) =>
  ratingVersion(dimension.key, period, index, LUO_SOURCE_IDS)
));

export const EMPTY_COMPANY_ARCHIVE_FIXTURE: RecordItem[] = STREAMER_ARCHIVE_FIXTURE.filter(
  (item) => item.archive_kind === "competitor",
);
