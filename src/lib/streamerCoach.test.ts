import assert from "node:assert/strict";
import {
  DEFAULT_COACH_HOSTS,
  UNASSIGNED_COACH_HOST_ID,
  buildCoachHostSummaries,
  buildCoachSystemPrompt,
  parseCoachAnalysisEntries,
  parseCoachMessages,
} from "./streamerCoach.js";

const diagnosis = {
  headline: "异议回应有证据，但成交推进偏弱",
  highlights: ["先确认了观众用途"],
  issues: ["成交推进不够明确"],
  confirmations: ["价格待确认"],
  nextActions: ["确认需求后给出一个可执行下一步"],
  stats: {
    totalCandidates: 1,
    reviewedCount: 1,
    pendingCount: 0,
    matchedCount: 1,
    unmatchedCount: 0,
    highScoreCount: 0,
    riskCount: 1,
    averageScore: 82,
  },
};

const entries = [
  ["bsr:content-analysis:v3:session-luo-1", JSON.stringify({
    sourceTitle: "罗雨欣 8月30日场",
    updatedAt: "2026-08-30T12:00:00.000Z",
    anchorName: "罗雨欣",
    anchorIdentityConfirmed: true,
    commentEvidenceStatus: "available",
    commentCount: 18,
    diagnosis,
    candidates: [{ id: "C1", start: 32, end: 49, scene: "成色异议", evidence: "观众问成色，主播先展示机身细节" }],
    reviews: {
      C1: {
        beginner: { summary: "事实边界清楚", checks: ["具体成色等级待确认"] },
        spokenScript: "AI复盘建议：先展示，再说明以链接实拍为准。",
      },
    },
  })],
  ["bsr:content-analysis:v3:session-unknown", JSON.stringify({
    sourceTitle: "导入视频（标题含罗雨欣但身份未确认）",
    updatedAt: "2026-08-29T12:00:00.000Z",
    anchorName: "罗雨欣",
    anchorIdentityConfirmed: false,
    commentEvidenceStatus: "missing",
    diagnosis,
  })],
] as const;

const sessions = parseCoachAnalysisEntries(entries);
assert.equal(sessions.length, 2);
assert.equal(sessions[0].hostId, "ROLE-LUO");
assert.equal(sessions[0].evidenceStatus, "comment_present");
assert.equal(sessions[0].commentCount, 18);
assert.equal(sessions[0].evidenceItems[0].startSec, 32);
assert.equal(sessions[1].hostId, UNASSIGNED_COACH_HOST_ID, "未确认身份不能仅凭标题归档");
assert.equal(sessions[1].evidenceStatus, "speech_video_only");

const summaries = buildCoachHostSummaries(DEFAULT_COACH_HOSTS, sessions);
const luo = summaries.find((item) => item.host.id === "ROLE-LUO");
assert.ok(luo);
assert.equal(luo.sessionCount, 1);
assert.equal(luo.averageScore, 82);
assert.equal(luo.commentSessionCount, 1);
assert.equal(luo.recurringIssues[0].label, "成交推进不够明确");

const prompt = buildCoachSystemPrompt(DEFAULT_COACH_HOSTS[0], sessions.filter((item) => item.hostId === "ROLE-LUO"));
assert.match(prompt, /当前只辅导主播“罗雨欣”/);
assert.match(prompt, /session-luo-1/);
assert.match(prompt, /价格待确认/);
assert.match(prompt, /禁止生成一条不存在的真实评论/);

assert.deepEqual(parseCoachMessages("not-json"), []);
assert.equal(parseCoachMessages(JSON.stringify([
  { id: "1", role: "user", content: " 怎么改？ ", createdAt: "2026-08-30" },
  { id: "2", role: "system", content: "不得保留" },
])).length, 1);

console.log("streamerCoach model tests passed");
