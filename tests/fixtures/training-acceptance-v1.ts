import type {
  TrainingCase,
  TrainingModuleKey,
  TrainingRole,
  TrainingScores,
} from "../../src/lib/scenarioTraining.js";

/**
 * Automated acceptance fixture only.
 *
 * Every user-facing string is marked with `【自动测试夹具】`. None of these
 * prompts, answers, identities, facts, evidence ids, or media files are real
 * livestream evidence and they must never be written to the production
 * knowledge base.
 */
export const TRAINING_ACCEPTANCE_FIXTURE_NAME = "acceptance-v1" as const;
export const TRAINING_ACCEPTANCE_SEED = 20260828;
export const TRAINING_ACCEPTANCE_MEDIA_PATH = "/tests/fixtures/media/evidence-1s.webm";

const ALL_MODULES: TrainingModuleKey[] = [
  "opening",
  "retention_interaction",
  "needs_discovery",
  "product_explanation",
  "objection_handling",
  "conversion",
  "transition",
  "incident",
];

export const TRAINING_ACCEPTANCE_ROLES: TrainingRole[] = [
  {
    id: "ROLE-LUO",
    displayName: "罗雨欣",
    sourceLabel: "【自动测试夹具】罗雨欣",
    description: "【自动测试夹具】八个训练板块均有合格案例。",
    availableCaseCount: 8,
    availableModules: [...ALL_MODULES],
    evidenceStatus: "ready",
  },
  {
    id: "ROLE-HOU",
    displayName: "侯梦娜",
    sourceLabel: "【自动测试夹具】已校正 1 个历史来源标签",
    description: "【自动测试夹具】用于验证来源标签映射和主播隔离。",
    availableCaseCount: 2,
    availableModules: ["objection_handling", "incident"],
    evidenceStatus: "ready",
  },
  {
    id: "ROLE-XIAOE",
    displayName: "小鹅",
    sourceLabel: "【自动测试夹具】已校正 1 个历史来源标签",
    description: "【自动测试夹具】用于验证误识别标签不会覆盖显示姓名。",
    availableCaseCount: 2,
    availableModules: ["retention_interaction", "transition"],
    evidenceStatus: "ready",
  },
  {
    id: "ROLE-YU",
    displayName: "于千惠",
    sourceLabel: "【自动测试夹具】于千惠",
    description: "【自动测试夹具】只有待审核或缺片段案例，可训练数为零。",
    availableCaseCount: 0,
    availableModules: [],
    evidenceStatus: "empty",
  },
];

const MODULE_PROMPTS: Record<TrainingModuleKey, string> = {
  opening: "【自动测试夹具】刚进入直播间，你会怎样开场说明本场价值？",
  retention_interaction: "【自动测试夹具】在线观众变少了，你会怎样留人互动？",
  needs_discovery: "【自动测试夹具】我只说想买设备，你会先确认哪些需求？",
  product_explanation: "【自动测试夹具】请只根据已确认资料讲清这件测试商品。",
  objection_handling: "【自动测试夹具】我觉得测试价格偏高，你会怎样回应？",
  conversion: "【自动测试夹具】我还在犹豫，你会怎样合规推进下一步？",
  transition: "【自动测试夹具】当前商品讲完了，你会怎样自然转品？",
  incident: "【自动测试夹具】链接暂时异常，你会怎样稳住直播节奏？",
};

function approvedCase(
  id: string,
  roleId: string,
  module: TrainingModuleKey,
  overrides: Partial<TrainingCase> = {},
): TrainingCase {
  return {
    id,
    roleId,
    module,
    viewerRole: "【自动测试夹具】潜在买家",
    prompt: MODULE_PROMPTS[module],
    realAnswer: `【自动测试夹具】${module} 模拟参考回答；不是主播真实原话。`,
    clipPath: TRAINING_ACCEPTANCE_MEDIA_PATH,
    evidenceId: `AUTO-EVIDENCE-${id}`,
    reviewStatus: "approved",
    roleIdentityConfirmed: true,
    commentConfirmed: true,
    answerConfirmed: true,
    clipPlayable: true,
    factsConfirmed: true,
    moduleConfirmed: true,
    ...overrides,
  };
}

export const TRAINING_ACCEPTANCE_APPROVED_CASES: TrainingCase[] = [
  ...ALL_MODULES.map((module) => approvedCase(`LUO-${module}`, "ROLE-LUO", module)),
  approvedCase("HOU-objection", "ROLE-HOU", "objection_handling", {
    prompt: "【自动测试夹具】测试观众质疑成色，侯梦娜训练如何先核对事实？",
  }),
  approvedCase("HOU-incident", "ROLE-HOU", "incident", {
    prompt: "【自动测试夹具】测试链接异常，侯梦娜训练如何控场？",
  }),
  approvedCase("XIAOE-retention", "ROLE-XIAOE", "retention_interaction", {
    prompt: "【自动测试夹具】测试转入冷场，小鹅训练如何重新互动？",
  }),
  approvedCase("XIAOE-transition", "ROLE-XIAOE", "transition", {
    prompt: "【自动测试夹具】测试商品讲完，小鹅训练如何衔接下一件？",
  }),
];

export const TRAINING_ACCEPTANCE_REJECTED_CASES: TrainingCase[] = [
  approvedCase("INVALID-pending", "ROLE-LUO", "opening", {
    reviewStatus: "pending",
    prompt: "【自动测试夹具】待审核案例不得进入训练。",
  }),
  approvedCase("INVALID-role", "ROLE-YU", "needs_discovery", {
    roleIdentityConfirmed: false,
    prompt: "【自动测试夹具】主播身份错误案例不得进入训练。",
  }),
  approvedCase("INVALID-module", "ROLE-LUO", "incident", {
    moduleConfirmed: false,
    prompt: "【自动测试夹具】板块未经确认案例不得进入训练。",
  }),
  approvedCase("INVALID-video", "ROLE-LUO", "objection_handling", {
    clipPlayable: false,
    clipPath: "",
    prompt: "【自动测试夹具】缺少可播放片段案例不得进入训练。",
  }),
];

export const TRAINING_ACCEPTANCE_CASES: TrainingCase[] = [
  ...TRAINING_ACCEPTANCE_APPROVED_CASES,
  ...TRAINING_ACCEPTANCE_REJECTED_CASES,
];

export const TRAINING_ACCEPTANCE_SCORES: TrainingScores = {
  needsConfirmation: 4,
  factAccuracy: 5,
  trustBuilding: 4,
  expressionClarity: 5,
  dealAdvancement: 3,
  riskCompliance: 5,
  livePacing: 4,
};

export const TRAINING_ACCEPTANCE_INVALID_SCORES = {
  needsConfirmation: 4,
  factAccuracy: 5,
  trustBuilding: 4,
  expressionClarity: 5,
  dealAdvancement: 3,
  riskCompliance: 5,
  // Deliberately missing livePacing: malformed model/runtime data must fail closed.
} as const;

export const TRAINING_ACCEPTANCE_CONFIRMED_FACTS = [
  "【自动测试夹具】测试商品只确认了外观状态",
] as const;

export const TRAINING_ACCEPTANCE_PASS_CRITERIA = [
  "进入训练页只显示角色选择，不提前显示训练问题。",
  "选择后仅选中卡保持清晰，其他卡变暗或虚化，姓名始终可读。",
  "训练主页显示正确的当前主播，历史来源标签不能覆盖正式显示名。",
  "专项训练只返回当前主播和所选板块的已审核案例。",
  "综合训练可跨板块，但不得跨主播或混入未审核案例。",
  "无合格证据时显示明确空态，不补题、不调用外部 AI。",
  "切换主播会清空旧题、旧答案、反馈、得分、板块和证据。",
  "角色卡支持 Tab、Enter、Space 和清晰焦点。",
  "减少动态效果时状态立即完成，业务结果不依赖动画事件。",
  "每轮严格完成问题、回答、七维评分、真实证据、下一题。",
  "结束总结包含板块得分、证据、反复问题和下次建议。",
  "商品事实不足时只能追问或标记待确认，禁止补造事实。",
  "评分缺维度、越界或证据未知时显示待复核，不自动补齐。",
  "测试视频必须实际加载并可读取大于零的时长。",
] as const;

export const TRAINING_ACCEPTANCE_FIXTURE = {
  name: TRAINING_ACCEPTANCE_FIXTURE_NAME,
  notice: "【自动测试夹具】非真实评论、非主播真实回答、不得进入生产知识库。",
  seed: TRAINING_ACCEPTANCE_SEED,
  mediaPath: TRAINING_ACCEPTANCE_MEDIA_PATH,
  roles: TRAINING_ACCEPTANCE_ROLES,
  cases: TRAINING_ACCEPTANCE_CASES,
  scores: TRAINING_ACCEPTANCE_SCORES,
  passCriteria: TRAINING_ACCEPTANCE_PASS_CRITERIA,
} as const;
