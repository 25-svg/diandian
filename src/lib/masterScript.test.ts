import assert from "node:assert/strict";
import {
  chunkProgress,
  masterPublishGate,
  masterStatusLabel,
  supportAdmissionPresentation,
  candidateDecisionStatus,
  masterBatchPurposeLabel,
  canActivateEnterpriseMaster,
  hostScriptAutomationAction,
  enterpriseMasterResumeAction,
  readyVideoIdsForEnterpriseMaster,
  shouldAutoGenerateEnterpriseDraft,
  supportCandidatePresentation,
  supportCandidateVersionPresentation,
  buildTrainingGuide,
  trainingGuideFileName,
  friendlyMasterError,
  upgradePublishGate,
  buildTransactionMasterView,
  evidenceGradeLabel,
  riskStatusLabel,
  segmentDecisionLabel,
  upgradeDecisionIsActionable,
  upgradeDecisionLabel,
  upgradeDecisionNextAction,
  supportCandidateUpgradeReview,
  supportCandidateCanUpgradeMaster,
  presentSegmentScoreDetails,
} from "./masterScript.js";

assert.equal(masterStatusLabel("transcribing"), "正在生成母稿逐字稿");
assert.equal(masterStatusLabel("reviewing"), "逐字稿等待确认");
assert.deepEqual(
  masterPublishGate({ pendingCriticalCount: 1, blockingIssues: [] }),
  { allowed: false, reason: "请先确认逐字稿中的关键内容" },
);
assert.deepEqual(
  masterPublishGate({ pendingCriticalCount: 0, blockingIssues: [] }),
  { allowed: true, reason: "可以发布母稿 V1.0" },
);
assert.equal(chunkProgress(6, 10), 60);
assert.equal(chunkProgress(3, 0), 0);
assert.equal(masterBatchPurposeLabel("sample"), "主播话术");
assert.equal(masterBatchPurposeLabel("enterprise"), "公司统一话术");
assert.equal(canActivateEnterpriseMaster("sample"), false);
assert.equal(canActivateEnterpriseMaster("enterprise"), true);
assert.equal(hostScriptAutomationAction("collecting"), "process");
assert.equal(hostScriptAutomationAction("failed"), "process");
assert.equal(hostScriptAutomationAction("ready_for_synthesis"), "generate");
assert.equal(hostScriptAutomationAction("draft_ready"), "review");
assert.equal(hostScriptAutomationAction("published"), "ready");
assert.equal(
  enterpriseMasterResumeAction("enterprise", "draft_ready", "ready"),
  "review",
);
assert.equal(
  enterpriseMasterResumeAction("enterprise", "ready_for_synthesis", "failed"),
  "generate",
);
assert.equal(
  enterpriseMasterResumeAction("enterprise", "processing", "generating"),
  "wait",
);
assert.equal(
  enterpriseMasterResumeAction("sample", "draft_ready", "ready"),
  "none",
);
assert.deepEqual(
  readyVideoIdsForEnterpriseMaster([
    {
      batch: { purpose: "sample" },
      items: [
        { videoId: 11, processingStatus: "ready" },
        { videoId: 12, processingStatus: "reviewing" },
      ],
    },
    {
      batch: { purpose: "enterprise" },
      items: [{ videoId: 11, processingStatus: "ready" }],
    },
    {
      batch: { purpose: "sample" },
      items: [{ videoId: 13, processingStatus: "ready" }],
    },
  ]),
  [11, 13],
);
assert.equal(shouldAutoGenerateEnterpriseDraft("enterprise", "ready_for_synthesis", null), true);
assert.equal(shouldAutoGenerateEnterpriseDraft("enterprise", "ready_for_synthesis", "generating"), false);
assert.equal(shouldAutoGenerateEnterpriseDraft("sample", "ready_for_synthesis", null), false);
assert.deepEqual(supportAdmissionPresentation("candidate_queue", 85), {
  tone: "success",
  label: "已进入候选辅稿",
  detail: "母稿匹配与可复用分 85，等待人工审核",
});
assert.deepEqual(supportAdmissionPresentation("blocked", 100, ["价格仍待确认"]), {
  tone: "warning",
  label: "暂不能进入候选辅稿",
  detail: "价格仍待确认",
});
assert.equal(segmentDecisionLabel("support_candidate"), "候选辅稿");
assert.equal(upgradeDecisionLabel("add_as_support"), "建议新增为辅稿");
assert.equal(upgradeDecisionLabel("add_as_golden_sentence"), "建议加入金句话术库");
assert.equal(upgradeDecisionLabel("replace_existing"), "建议人工审核后替换");
assert.equal(upgradeDecisionLabel("merge_with_existing"), "建议与现有章节合并");
assert.equal(upgradeDecisionLabel("duplicate"), "与企业母稿重复");
assert.equal(upgradeDecisionLabel("reject"), "不建议采用");
assert.match(upgradeDecisionNextAction("replace_existing"), /提交人工审核/);
assert.match(upgradeDecisionNextAction("merge_with_existing"), /提交人工审核/);
assert.equal(upgradeDecisionIsActionable("duplicate"), false);
assert.equal(upgradeDecisionIsActionable("reject"), false);
assert.equal(upgradeDecisionIsActionable("add_as_support"), true);
const upgradeReviewCandidate = {
  comparisonJson: JSON.stringify({
    comparison: { totalScore: 88 },
    upgradeReview: {
      comparisonDecision: "merge_with_existing",
      matchedMasterSection: "异议处理/质量担忧",
      sameScene: true,
      newValue: "补充了检测证据",
      whyBetter: ["比母稿更具体"],
      duplicateContent: ["同样先回应顾虑"],
      riskOrUncertainty: ["检测范围待确认"],
      originalHostWords: "我们每台机器都经过检测。",
      trainingSuggestion: "[建议稿] 先说明检测范围。",
      recommendedAction: "人工审核后合并",
    },
  }),
};
assert.equal(
  supportCandidateUpgradeReview(upgradeReviewCandidate)?.comparisonDecision,
  "merge_with_existing",
);
assert.equal(supportCandidateUpgradeReview({ comparisonJson: "{}" }), null);
assert.equal(supportCandidateCanUpgradeMaster(upgradeReviewCandidate), true);
assert.equal(supportCandidateCanUpgradeMaster({ comparisonJson: "{}" }), false);
assert.equal(supportCandidateCanUpgradeMaster({
  comparisonJson: JSON.stringify({
    upgradeReview: {
      ...JSON.parse(upgradeReviewCandidate.comparisonJson).upgradeReview,
      comparisonDecision: "duplicate",
    },
  }),
}), false);
assert.equal(segmentDecisionLabel("golden_sentence"), "金句话术");
assert.equal(segmentDecisionLabel("training_material"), "训练素材");
assert.equal(segmentDecisionLabel("reference_only"), "仅作复盘参考");
assert.equal(segmentDecisionLabel("blocked"), "禁止使用");
assert.equal(evidenceGradeLabel("A"), "A级 · 证据充分");
assert.equal(evidenceGradeLabel("B"), "B级 · 少量事实待确认");
assert.equal(evidenceGradeLabel("C"), "C级 · 仅作观察素材");
assert.equal(riskStatusLabel("passed"), "无关键风险");
assert.equal(riskStatusLabel("needs_review"), "需要人工确认");
assert.equal(riskStatusLabel("blocked"), "存在阻断风险");
assert.deepEqual(
  presentSegmentScoreDetails({
    sceneGoal: {
      score: 24,
      maxScore: 30,
      reason: "完成产品匹配",
      evidence: ["00:00:06 原话"],
    },
    persuasiveness: undefined,
  }),
  [
    {
      label: "场景目标完成度",
      detail: {
        score: 24,
        maxScore: 30,
        reason: "完成产品匹配",
        evidence: ["00:00:06 原话"],
      },
    },
  ],
);
assert.deepEqual(presentSegmentScoreDetails(undefined), []);
assert.equal(candidateDecisionStatus("加入下一版企业话术"), "approved");
assert.equal(candidateDecisionStatus("保留为局部素材"), "held");
assert.equal(candidateDecisionStatus("不采用"), "rejected");
assert.deepEqual(
  supportCandidatePresentation({
    comparisonJson: JSON.stringify({
      moduleTags: ["需求探询", "产品讲解"],
      improvements: ["需求探询：先复述顾客需求，再给出对应推荐", "产品讲解：说明成色和配置，降低顾客顾虑"],
      risks: ["价格需按当天直播间实际信息确认"],
      suggestedInsertionPoint: "放入位置：产品讲解开头；使用方式：先问需求，再接产品推荐",
    }),
  }),
  {
    scenario: "需求探询、产品讲解",
    reasons: ["先复述顾客需求，再给出对应推荐", "说明成色和配置，降低顾客顾虑"],
    checks: ["价格需按当天直播间实际信息确认"],
    insertion: "放入位置：产品讲解开头；使用方式：先问需求，再接产品推荐",
  },
);
assert.deepEqual(
  supportCandidatePresentation({ comparisonJson: "not-json" }),
  {
    scenario: "完整成交场景",
    reasons: ["保留主播原话，作为完整成交过程的可复用示范。"],
    checks: ["未发现需要额外确认的事项。"],
    insertion: "放入位置由审核人确认；使用时保留主播原话。",
  },
);
assert.deepEqual(supportCandidateVersionPresentation(8, 8), {
  current: true,
  label: "当前企业标准",
  detail: "这条话术按当前企业标准评分，可以继续人工审核。",
});
assert.deepEqual(supportCandidateVersionPresentation(7, 8), {
  current: false,
  label: "需要按新版复核",
  detail: "这条话术按旧版企业标准评分，不能直接加入新版企业话术。",
});
const trainingGuide = buildTrainingGuide({
  master: { id: 8, scriptKey: "MS-ENTERPRISE", version: "2.0.0", title: "金典拍拍企业标准话术" },
  sections: [{
    id: 10,
    masterScriptId: 8,
    sectionKey: "product-intro",
    position: 2,
    sectionKind: "product",
    productCardId: null,
    title: "产品讲解",
    masterText: "老师先告诉我您的预算，我再给您推荐合适的型号。",
  }],
}, "2026-07-24");
assert.match(trainingGuide.html, /金典拍拍企业标准话术/);
assert.match(trainingGuide.html, /产品讲解/);
assert.match(trainingGuide.html, /价格、库存、优惠、链接、成色等以当场事实为准/);
assert.match(trainingGuide.rtf, /^\{\\rtf1/);
assert.equal(trainingGuideFileName(trainingGuide, "rtf"), "金典拍拍企业标准话术_V2.0.0_培训版.rtf");
assert.deepEqual(friendlyMasterError("os error 2: file not found"), {
  title: "找不到原视频文件",
  detail: "这场录播的原文件已移动或删除，系统没有改动已有逐字稿和母稿。",
  nextAction: "请重新导入原视频后，再继续处理该场。",
});
assert.deepEqual(friendlyMasterError("MiniMax 母稿章节格式错误: expected `,` or `}`"), {
  title: "本次整理结果没有生成成功",
  detail: "模型返回内容格式异常，已有逐字稿和已发布企业标准均保持不变。",
  nextAction: "点击“重新生成”即可，不需要重新转写视频。",
});
assert.deepEqual(friendlyMasterError("母稿已更新，请按最新版本重新评分"), {
  title: "企业标准已更新",
  detail: "这条结果基于旧版企业标准，不能直接加入新版。",
  nextAction: "回到对应录播，按当前企业标准重新分析该片段。",
});
assert.deepEqual(upgradePublishGate(false, 1), { allowed: false, reason: "请先确认母稿差异" });
assert.deepEqual(upgradePublishGate(true, 0), { allowed: false, reason: "至少选择一条已通过的候选辅稿" });
assert.deepEqual(upgradePublishGate(true, 2), { allowed: true, reason: "可以发布母稿新版本" });

const transactionMasterView = buildTransactionMasterView({
  draft: {
    title: "金典拍拍企业话术",
    evidence: [],
    patterns: [{ name: "先问需求", observation: "先确认预算和用途", evidenceIds: ["V1-01"] }],
    sections: [{
      title: "需求确认",
      purpose: "确认预算和用途",
      evidenceIds: ["V1-01"],
      fixedSpeech: "老师，您预算大概多少，主要拍什么？",
      conditions: [],
      dynamicFields: ["预算", "用途"],
    }],
    operatingRules: ["先完成当前用户的关键动作，再回应其他用户。"],
  },
  candidates: [
    {
      id: 1,
      candidateKey: "complete-1",
      masterScriptId: 8,
      masterSectionId: 10,
      sourceKey: "video:1",
      sourceStartMs: 60_000,
      sourceEndMs: 180_000,
      hostText: "老师，我先按您的需求挑一只，再验货、报价、上链接。",
      comparisonJson: JSON.stringify({ moduleTags: ["完整成交链路"], improvements: ["七步链路完整"] }),
      totalScore: 88,
      admission: "candidate_queue",
      status: "approved",
      createdAt: "2026-07-25T00:00:00Z",
    },
    {
      id: 2,
      candidateKey: "local-1",
      masterScriptId: 8,
      masterSectionId: 10,
      sourceKey: "video:2",
      sourceStartMs: 30_000,
      sourceEndMs: 45_000,
      hostText: "我先把当前老师这只验完，马上回来回答您。",
      comparisonJson: "{}",
      totalScore: 0,
      admission: "review_only",
      status: "held",
      createdAt: "2026-07-25T00:00:00Z",
    },
  ],
});
assert.equal(transactionMasterView.rules.length, 2);
assert.equal(transactionMasterView.completeExamples.length, 1);
assert.equal(transactionMasterView.completeExamples[0].score, 88);
assert.equal(transactionMasterView.trainingMaterials.length, 1);
assert.match(transactionMasterView.trainingMaterials[0].text, /当前老师这只验完/);

console.log("master script UI rules passed");
