import { convertFileSrc, invoke } from "@tauri-apps/api/core";
import {
  TRAINING_MODULES,
  isTrainingModuleKey,
  normalizeHumanMachineTurnCount,
  normalizeTrainingViewerRole,
  selectTrainingCases,
  validateTrainingScores,
  type TrainingCase,
  type TrainingFeedback,
  type TrainingMode,
  type TrainingModuleKey,
  type TrainingModuleScore,
  type TrainingNextResult,
  type TrainingQuestion,
  type TrainingRole,
  type TrainingScores,
  type TrainingStartResult,
  type TrainingSummary,
  type HumanMachineScenario,
  type HumanMachineHardCheck,
  type HumanMachineFollowUpFocus,
  type HumanMachineTurnCount,
} from "./scenarioTraining";

export interface StartTrainingRequest {
  roleId: string;
  mode: TrainingMode;
  selectedModule?: TrainingModuleKey;
  seed?: number;
}

export interface ScenarioTrainingApiAdapter {
  listTrainingRoles(): Promise<TrainingRole[]>;
  startTrainingSession(request: StartTrainingRequest): Promise<TrainingStartResult>;
  submitTrainingAnswer(request: {
    sessionId: string;
    traineeAnswer: string;
  }): Promise<TrainingFeedback>;
  nextTrainingQuestion(request: { sessionId: string }): Promise<TrainingNextResult>;
  completeTrainingSession(request: { sessionId: string }): Promise<TrainingSummary>;
  abandonTrainingSession(request: { sessionId: string }): Promise<void>;
  startHumanMachineScenario(request: {
    roleId: string;
    selectedModule?: TrainingModuleKey;
    seed?: number;
    totalTurns?: HumanMachineTurnCount;
  }): Promise<HumanMachineScenario>;
  submitHumanMachineTurn(request: {
    runId: string;
    traineeAnswer: string;
  }): Promise<HumanMachineScenario>;
  getActiveHumanMachineScenario(request: { roleId: string }): Promise<HumanMachineScenario | null>;
  abandonHumanMachineScenario(request: { runId: string }): Promise<void>;
}

declare global {
  interface Window {
    __SCENARIO_TRAINING_TEST_FIXTURE__?: string | ScenarioTrainingApiAdapter;
  }
}

function record(value: unknown): Record<string, any> {
  return value && typeof value === "object" ? (value as Record<string, any>) : {};
}

function text(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : value == null ? fallback : String(value);
}

function number(value: unknown, fallback = 0): number {
  const result = Number(value);
  return Number.isFinite(result) ? result : fallback;
}

function moduleKey(value: unknown): TrainingModuleKey | undefined {
  if (isTrainingModuleKey(value)) return value;
  const aliases: Record<string, TrainingModuleKey> = {
    retentionInteraction: "retention_interaction",
    needsConfirmation: "needs_discovery",
    needs_confirmation: "needs_discovery",
    productExplanation: "product_explanation",
    objectionHandling: "objection_handling",
    dealAdvancement: "conversion",
    deal_advancement: "conversion",
    productTransition: "transition",
    product_transition: "transition",
    incidentResponse: "incident",
    incident_response: "incident",
  };
  return typeof value === "string" ? aliases[value] : undefined;
}

function mode(value: unknown): TrainingMode {
  return value === "specialized" ? "specialized" : "comprehensive";
}

function normalizeRole(value: unknown): TrainingRole {
  const source = record(value);
  const unavailableReason = text(source.unavailableReason ?? source.unavailable_reason);
  const moduleCountSource = record(source.moduleCounts ?? source.module_counts);
  const moduleValues = Array.isArray(source.availableModules)
    ? source.availableModules
    : Array.isArray(source.available_modules)
      ? source.available_modules
      : Object.keys(moduleCountSource).filter((key) => number(moduleCountSource[key]) > 0);
  const modules = moduleValues
    .map(moduleKey)
    .filter(Boolean) as TrainingModuleKey[];
  const availableCaseCount = number(
    source.approvedCaseCount ?? source.approved_case_count ?? source.availableCaseCount ?? source.available_case_count ?? source.caseCount ?? source.case_count,
  );
  return {
    id: text(source.id ?? source.roleId ?? source.role_id),
    displayName: text(source.displayName ?? source.display_name ?? source.name, "待确认主播"),
    sourceLabel: text(source.sourceLabel ?? source.source_label) || undefined,
    description: text(source.description ?? source.summary) || unavailableReason || undefined,
    avatarUrl: text(source.avatarUrl ?? source.avatar_url) || undefined,
    availableCaseCount,
    availableModules: modules,
    evidenceStatus:
      source.evidenceStatus === "pending" || source.evidence_status === "pending"
        ? "pending"
        : availableCaseCount > 0
          ? "ready"
          : "empty",
    unavailableReason: unavailableReason || undefined,
  };
}

function normalizeQuestion(value: unknown): TrainingQuestion | null {
  if (!value) return null;
  const source = record(value);
  const normalizedModule = moduleKey(source.module ?? source.moduleKey ?? source.module_key);
  const caseId = text(source.caseId ?? source.case_id ?? source.id);
  const prompt = text(source.prompt ?? source.question ?? source.comment);
  if (!caseId || !normalizedModule || !prompt) return null;
  return {
    caseId,
    module: normalizedModule,
    prompt,
    viewerRole: normalizeTrainingViewerRole(source.viewerRole ?? source.viewer_role),
    sourceKind: "approved_comment",
  };
}

function normalizeSession(value: unknown) {
  const source = record(value);
  const nested = record(source.session);
  const merged = { ...nested, ...source };
  return {
    sessionId: text(merged.sessionId ?? merged.session_id ?? merged.id),
    roleId: text(merged.roleId ?? merged.role_id),
    displayName: text(merged.displayName ?? merged.display_name ?? merged.roleName ?? merged.role_name),
    mode: mode(merged.mode),
    selectedModule: moduleKey(merged.selectedModule ?? merged.selected_module),
    currentIndex: number(merged.currentIndex ?? merged.current_index, 0),
    totalQuestions: number(merged.totalQuestions ?? merged.total_questions, 1),
  };
}

function normalizeStart(value: unknown): TrainingStartResult {
  const source = record(value);
  const session = normalizeSession(source);
  const question = normalizeQuestion(source.question);
  if (!session.sessionId || !session.roleId || !question) {
    throw new Error("训练会话返回不完整，请检查已审核证据或稍后重试。");
  }
  return { ...session, question };
}

function normalizeNext(value: unknown): TrainingNextResult {
  const source = record(value);
  const session = normalizeSession(source);
  const question = normalizeQuestion(source.question);
  return {
    ...session,
    question,
    isComplete: Boolean(source.isComplete ?? source.is_complete) || !question,
  };
}

function normalizeFeedback(value: unknown): TrainingFeedback {
  const source = record(value);
  const validation = validateTrainingScores(source.scores);
  const evaluationStatus = text(source.evaluationStatus ?? source.evaluation_status);
  return {
    caseId: text(source.caseId ?? source.case_id),
    scores: validation.scores ?? record(source.scores),
    scoresValid: validation.valid && evaluationStatus !== "needs_review",
    priorityImprovement: text(source.priorityImprovement ?? source.priority_improvement, "待复核"),
    coachSuggestion: text(source.coachSuggestion ?? source.coach_suggestion, "待复核"),
    realAnswer: text(source.realAnswer ?? source.real_answer, "待复核"),
    clipPath: text(source.clipPath ?? source.clip_path) || undefined,
    evidenceId: text(source.evidenceId ?? source.evidence_id, "待复核"),
    evaluationStatus:
      validation.valid && evaluationStatus !== "needs_review" ? "complete" : "needs_review",
  };
}

function normalizeModuleScores(value: unknown): TrainingModuleScore[] {
  if (Array.isArray(value)) {
    return value
      .map((item) => {
        const source = record(item);
        const key = moduleKey(source.module ?? source.moduleKey ?? source.module_key);
        const validation = validateTrainingScores(source.dimensionScores ?? source.dimension_scores);
        return key
          ? {
              module: key,
              score: number(source.score, validation.scores
                ? Object.values(validation.scores).reduce((total, item) => total + item, 0) / 7
                : 0),
              dimensionScores: validation.scores ?? undefined,
            }
          : null;
      })
      .filter(Boolean) as TrainingModuleScore[];
  }
  return Object.entries(record(value))
    .map(([key, scoreValue]) => {
      const normalizedKey = moduleKey(key);
      const validation = validateTrainingScores(scoreValue);
      const normalizedScore = validation.scores
        ? Object.values(validation.scores).reduce((total, item) => total + item, 0) / 7
        : number(scoreValue);
      return normalizedKey
        ? { module: normalizedKey, score: normalizedScore, dimensionScores: validation.scores ?? undefined }
        : null;
    })
    .filter(Boolean) as TrainingModuleScore[];
}

function stringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.map((item) => text(item)).filter(Boolean) : [];
}

function normalizeSummary(value: unknown): TrainingSummary {
  const source = record(value);
  return {
    sessionId: text(source.sessionId ?? source.session_id),
    roleId: text(source.roleId ?? source.role_id),
    displayName: text(source.displayName ?? source.display_name ?? source.roleName ?? source.role_name),
    mode: mode(source.mode),
    selectedModule: moduleKey(source.selectedModule ?? source.selected_module),
    answeredCount: number(source.answeredCount ?? source.answered_count),
    moduleScores: normalizeModuleScores(source.moduleScores ?? source.module_scores),
    evidenceIds: stringArray(source.evidenceIds ?? source.evidence_ids),
    recurringIssues: stringArray(source.recurringIssues ?? source.recurring_issues),
    nextTrainingSuggestions: stringArray(
      source.nextTrainingSuggestions ?? source.next_training_suggestions,
    ),
  };
}

function normalizeHumanMachineScenario(value: unknown): HumanMachineScenario {
  const source = record(value);
  const normalizedModule = moduleKey(source.module ?? source.moduleKey ?? source.module_key);
  const turns = (Array.isArray(source.turns) ? source.turns : []).map((item) => {
    const turn = record(item);
    const rawFocus = text(turn.followUpFocus ?? turn.follow_up_focus);
    const followUpFocus = [
      "adaptive_follow_up",
      "needs_confirmation",
      "fact_clarification",
      "trust_building",
      "objection_resolution",
      "deal_advancement",
      "risk_compliance",
      "live_pacing",
    ].includes(rawFocus) ? rawFocus as HumanMachineFollowUpFocus : undefined;
    return {
      turnIndex: number(turn.turnIndex ?? turn.turn_index),
      viewerMessage: text(turn.viewerMessage ?? turn.viewer_message),
      viewerSource:
        text(turn.viewerSource ?? turn.viewer_source) === "approved_comment"
          ? ("approved_comment" as const)
          : ("ai_simulated_follow_up" as const),
      followUpFocus,
      traineeAnswer: text(turn.traineeAnswer ?? turn.trainee_answer) || undefined,
    };
  });
  const hardChecks = (Array.isArray(source.hardChecks ?? source.hard_checks)
    ? source.hardChecks ?? source.hard_checks
    : [])
    .map((item: unknown) => {
      const check = record(item);
      const key = text(check.key) as HumanMachineHardCheck["key"];
      if (!["needs_confirmation", "fact_boundary", "compliant_next_step"].includes(key)) return null;
      return {
        key,
        label: text(check.label),
        passed: Boolean(check.passed),
        detail: text(check.detail),
      };
    })
    .filter(Boolean) as HumanMachineHardCheck[];
  const validation = validateTrainingScores(source.scores);
  const evaluationStatus = text(source.evaluationStatus ?? source.evaluation_status, "pending") as HumanMachineScenario["evaluationStatus"];
  if (!text(source.runId ?? source.run_id) || !normalizedModule || turns.length === 0) {
    throw new Error("人机情景训练返回不完整，请重新开始。");
  }
  return {
    runId: text(source.runId ?? source.run_id),
    roleId: text(source.roleId ?? source.role_id),
    displayName: text(source.displayName ?? source.display_name, "待确认主播"),
    module: normalizedModule,
    currentTurn: number(source.currentTurn ?? source.current_turn, 1),
    totalTurns: normalizeHumanMachineTurnCount(source.totalTurns ?? source.total_turns),
    status: text(source.status, "active") as HumanMachineScenario["status"],
    turns,
    factConstraints: record(source.factConstraints ?? source.fact_constraints),
    hardChecks,
    evaluationStatus,
    scores: validation.scores ?? (source.scores == null ? null : record(source.scores)),
    scoresValid: evaluationStatus === "scored" && validation.valid,
    priorityImprovement: text(source.priorityImprovement ?? source.priority_improvement) || undefined,
    coachSuggestion: text(source.coachSuggestion ?? source.coach_suggestion) || undefined,
    realAnswer: text(source.realAnswer ?? source.real_answer),
    clipPath: text(source.clipPath ?? source.clip_path) || undefined,
    evidenceId: text(source.evidenceId ?? source.evidence_id),
  };
}

const tauriAdapter: ScenarioTrainingApiAdapter = {
  async listTrainingRoles() {
    const result = await invoke<unknown[]>("list_training_roles");
    return (Array.isArray(result) ? result : []).map(normalizeRole);
  },
  async startTrainingSession(request) {
    return normalizeStart(await invoke("start_training_session", { request }));
  },
  async submitTrainingAnswer(request) {
    return normalizeFeedback(await invoke("submit_training_answer", { request }));
  },
  async nextTrainingQuestion(request) {
    return normalizeNext(await invoke("next_training_question", { request }));
  },
  async completeTrainingSession(request) {
    return normalizeSummary(await invoke("complete_training_session", { request }));
  },
  async abandonTrainingSession(request) {
    await invoke("abandon_training_session", { request });
  },
  async startHumanMachineScenario(request) {
    return normalizeHumanMachineScenario(await invoke("start_human_machine_scenario", { request }));
  },
  async submitHumanMachineTurn(request) {
    return normalizeHumanMachineScenario(await invoke("submit_human_machine_turn", { request }));
  },
  async getActiveHumanMachineScenario(request) {
    const result = await invoke("get_active_human_machine_scenario", { request });
    return result ? normalizeHumanMachineScenario(result) : null;
  },
  async abandonHumanMachineScenario(request) {
    await invoke("abandon_human_machine_scenario", { request });
  },
};

const FIXTURE_ROLES: TrainingRole[] = [
  {
    id: "ROLE-LUO",
    displayName: "罗雨欣",
    sourceLabel: "【自动测试夹具】罗雨欣",
    description: "【自动测试夹具】八个训练板块均有合格案例。",
    availableCaseCount: 8,
    availableModules: TRAINING_MODULES.map((item) => item.key),
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

const FIXTURE_CASES: TrainingCase[] = TRAINING_MODULES.map((item) => ({
  id: `LUO-${item.key}`,
  roleId: "ROLE-LUO",
  module: item.key,
  viewerRole: "【自动测试夹具】潜在买家",
  prompt:
    item.key === "objection_handling"
      ? "【自动测试夹具】我觉得测试价格偏高，你会怎样回应？"
      : `【自动测试夹具】${item.label}场景：观众提出了一个需要现场回应的问题，你会怎么回答？`,
  realAnswer: `【自动测试夹具】${item.key} 模拟参考回答；不是主播真实原话。`,
  clipPath: "/tests/fixtures/media/evidence-1s.webm",
  evidenceId: `AUTO-EVIDENCE-LUO-${item.key}`,
  reviewStatus: "approved" as const,
  roleIdentityConfirmed: true,
  commentConfirmed: true,
  answerConfirmed: true,
  clipPlayable: true,
  factsConfirmed: true,
  moduleConfirmed: true,
})).concat([
  ...(["objection_handling", "incident"] as TrainingModuleKey[]).map((item, index) => ({
    id: index === 0 ? "HOU-objection" : "HOU-incident",
    roleId: "ROLE-HOU",
    module: item,
    viewerRole: "【自动测试夹具】质疑观众",
    prompt:
      item === "objection_handling"
        ? "【自动测试夹具】测试观众质疑成色，侯梦娜训练如何先核对事实？"
        : "【自动测试夹具】测试链接异常，侯梦娜训练如何控场？",
    realAnswer: `【自动测试夹具】${item} 模拟参考回答；不是主播真实原话。`,
    clipPath: "/tests/fixtures/media/evidence-1s.webm",
    evidenceId: `AUTO-EVIDENCE-${index === 0 ? "HOU-objection" : "HOU-incident"}`,
    reviewStatus: "approved" as const,
    roleIdentityConfirmed: true,
    commentConfirmed: true,
    answerConfirmed: true,
    clipPlayable: true,
    factsConfirmed: true,
    moduleConfirmed: true,
  })),
  ...(["retention_interaction", "transition"] as TrainingModuleKey[]).map((item, index) => ({
    id: index === 0 ? "XIAOE-retention" : "XIAOE-transition",
    roleId: "ROLE-XIAOE",
    module: item,
    viewerRole: "【自动测试夹具】互动观众",
    prompt:
      item === "retention_interaction"
        ? "【自动测试夹具】测试转入冷场，小鹅训练如何重新互动？"
        : "【自动测试夹具】测试商品讲完，小鹅训练如何衔接下一件？",
    realAnswer: `【自动测试夹具】${item} 模拟参考回答；不是主播真实原话。`,
    clipPath: "/tests/fixtures/media/evidence-1s.webm",
    evidenceId: `AUTO-EVIDENCE-${index === 0 ? "XIAOE-retention" : "XIAOE-transition"}`,
    reviewStatus: "approved" as const,
    roleIdentityConfirmed: true,
    commentConfirmed: true,
    answerConfirmed: true,
    clipPlayable: true,
    factsConfirmed: true,
    moduleConfirmed: true,
  })),
]);

interface FixtureSession {
  result: TrainingStartResult;
  cases: TrainingCase[];
  index: number;
  evidenceIds: string[];
  answeredCaseIds: Set<string>;
}

const fixtureSessions = new Map<string, FixtureSession>();
const fixtureHumanMachineRuns = new Map<string, HumanMachineScenario>();

function fixtureQuestion(item: TrainingCase): TrainingQuestion {
  return {
    caseId: item.id,
    module: item.module,
    prompt: item.prompt,
    viewerRole: normalizeTrainingViewerRole(item.viewerRole),
    sourceKind: "approved_comment",
  };
}

const fixtureAdapter: ScenarioTrainingApiAdapter = {
  async listTrainingRoles() {
    return FIXTURE_ROLES.map((role) => ({ ...role, availableModules: [...role.availableModules] }));
  },
  async startTrainingSession(request) {
    const role = FIXTURE_ROLES.find((item) => item.id === request.roleId);
    if (!role || role.availableCaseCount === 0) {
      throw new Error("暂无经审核训练证据，不能生成模拟题。");
    }
    const cases = selectTrainingCases(FIXTURE_CASES, {
      ...request,
      limit: request.mode === "comprehensive" ? 3 : 8,
    });
    if (cases.length === 0) throw new Error("所选板块暂无经审核训练证据。");
    const sessionId = `FIXTURE-${request.roleId}-${Date.now()}`;
    const result: TrainingStartResult = {
      sessionId,
      roleId: role.id,
      displayName: role.displayName,
      mode: request.mode,
      selectedModule: request.selectedModule,
      currentIndex: 0,
      totalQuestions: cases.length,
      question: fixtureQuestion(cases[0]),
    };
    fixtureSessions.set(sessionId, { result, cases, index: 0, evidenceIds: [], answeredCaseIds: new Set() });
    return result;
  },
  async submitTrainingAnswer(request) {
    const session = fixtureSessions.get(request.sessionId);
    if (!session) throw new Error("训练会话已失效，请重新开始。");
    if (!request.traineeAnswer.trim()) throw new Error("请先输入你的现场回答。");
    const item = session.cases[session.index];
    if (session.answeredCaseIds.has(item.id)) throw new Error("本题已经提交，请进入下一题。");
    session.evidenceIds.push(item.evidenceId || "待复核");
    session.answeredCaseIds.add(item.id);
    const scores: TrainingScores = {
      needsConfirmation: 4,
      factAccuracy: 5,
      trustBuilding: 4,
      expressionClarity: 5,
      dealAdvancement: 3,
      riskCompliance: 5,
      livePacing: 4,
    };
    const invalidScores = request.traineeAnswer.includes("【自动测试夹具：异常评分】");
    return {
      caseId: item.id,
      scores: invalidScores ? { ...scores, livePacing: undefined } : scores,
      scoresValid: !invalidScores,
      priorityImprovement: "成交推进：给观众一个更明确、但不过度催促的下一步。",
      coachSuggestion: "【AI练习建议，非主播原话】先确认需求，再说明已确认事实，最后给出可执行选择。",
      realAnswer: item.realAnswer || "待复核",
      clipPath: item.clipPath,
      evidenceId: item.evidenceId || "待复核",
    };
  },
  async nextTrainingQuestion(request) {
    const session = fixtureSessions.get(request.sessionId);
    if (!session) throw new Error("训练会话已失效，请重新开始。");
    const current = session.cases[session.index];
    if (current && !session.answeredCaseIds.has(current.id)) {
      throw new Error("请先提交当前题回答。");
    }
    session.index += 1;
    const item = session.cases[session.index];
    return {
      ...session.result,
      currentIndex: session.index,
      question: item ? fixtureQuestion(item) : null,
      isComplete: !item,
    };
  },
  async completeTrainingSession(request) {
    const session = fixtureSessions.get(request.sessionId);
    if (!session) throw new Error("训练会话已失效，请重新开始。");
    if (session.answeredCaseIds.size !== session.cases.length) {
      throw new Error("还有题目尚未回答，不能提前结束训练。");
    }
    const role = FIXTURE_ROLES.find((item) => item.id === session.result.roleId)!;
    const moduleKeys = [...new Set(session.cases.map((item) => item.module))];
    return {
      sessionId: request.sessionId,
      roleId: role.id,
      displayName: role.displayName,
      mode: session.result.mode,
      selectedModule: session.result.selectedModule,
      answeredCount: session.answeredCaseIds.size,
      moduleScores: moduleKeys.map((item) => ({ module: item, score: 4 })),
      evidenceIds: [...new Set(session.evidenceIds)],
      recurringIssues: ["成交推进不够明确"],
      nextTrainingSuggestions: ["下一轮继续练习需求确认后的自然收口。"],
    };
  },
  async abandonTrainingSession(request) {
    fixtureSessions.delete(request.sessionId);
  },
  async startHumanMachineScenario(request) {
    const role = FIXTURE_ROLES.find((item) => item.id === request.roleId);
    const item = selectTrainingCases(FIXTURE_CASES, {
      roleId: request.roleId,
      mode: request.selectedModule ? "specialized" : "comprehensive",
      selectedModule: request.selectedModule,
      limit: 1,
      seed: request.seed,
    })[0];
    if (!role || !item) throw new Error("当前主播暂无可用于人机训练的真实案例。");
    const run: HumanMachineScenario = {
      runId: `HM-FIXTURE-${request.roleId}-${Date.now()}`,
      roleId: role.id,
      displayName: role.displayName,
      module: item.module,
      currentTurn: 1,
      totalTurns: normalizeHumanMachineTurnCount(request.totalTurns),
      status: "active",
      turns: [{ turnIndex: 1, viewerMessage: item.prompt, viewerSource: "approved_comment" }],
      factConstraints: { source: "【自动测试夹具】已审核事实边界", unknownFacts: "必须先核实" },
      hardChecks: [],
      evaluationStatus: "pending",
      scores: null,
      scoresValid: false,
      realAnswer: item.realAnswer || "待复核",
      clipPath: item.clipPath,
      evidenceId: item.evidenceId || "待复核",
    };
    fixtureHumanMachineRuns.set(run.runId, run);
    return structuredClone(run);
  },
  async submitHumanMachineTurn(request) {
    const run = fixtureHumanMachineRuns.get(request.runId);
    if (!run || run.status !== "active") throw new Error("人机训练会话已失效，请重新开始。");
    const current = run.turns.find((item) => item.turnIndex === run.currentTurn);
    if (!current) throw new Error("当前对话轮次不存在。");
    current.traineeAnswer ||= request.traineeAnswer.trim();
    if (run.currentTurn < run.totalTurns) {
      run.currentTurn += 1;
      if (!run.turns.some((item) => item.turnIndex === run.currentTurn)) {
        const fixtureFollowUps: Array<{ message: string; focus: HumanMachineFollowUpFocus }> = [
          { message: "那你先帮我确认一下，怎么判断适不适合我？", focus: "needs_confirmation" },
          { message: "如果这些信息暂时不能确认，你能给我什么可核验的依据？", focus: "fact_clarification" },
          { message: "我还是有点犹豫，你会怎么让我放心？", focus: "trust_building" },
          { message: "确认清楚以后，我现在最合适的下一步是什么？", focus: "deal_advancement" },
        ];
        const followUp = fixtureFollowUps[run.currentTurn - 2];
        run.turns.push({
          turnIndex: run.currentTurn,
          viewerMessage: followUp.message,
          viewerSource: "ai_simulated_follow_up",
          followUpFocus: followUp.focus,
        });
      }
    } else {
      run.status = "completed";
      run.evaluationStatus = "scored";
      run.scores = {
        needsConfirmation: 4,
        factAccuracy: 5,
        trustBuilding: 4,
        expressionClarity: 5,
        dealAdvancement: 4,
        riskCompliance: 5,
        livePacing: 4,
      };
      run.scoresValid = true;
      run.priorityImprovement = "成交推进";
      run.coachSuggestion = "AI练习建议（非主播原话）：确认需求和事实后，给出一个可执行下一步。";
      run.hardChecks = [
        { key: "needs_confirmation", label: "先确认需求", passed: true, detail: "回答包含追问或确认动作。" },
        { key: "fact_boundary", label: "不编造未确认事实", passed: true, detail: "未发现确定性虚假承诺。" },
        { key: "compliant_next_step", label: "给出合规下一步", passed: true, detail: "回答给出了核实或选择动作。" },
      ];
    }
    return structuredClone(run);
  },
  async getActiveHumanMachineScenario(request) {
    const run = [...fixtureHumanMachineRuns.values()]
      .reverse()
      .find((item) => item.roleId === request.roleId && item.status === "active");
    return run ? structuredClone(run) : null;
  },
  async abandonHumanMachineScenario(request) {
    fixtureHumanMachineRuns.delete(request.runId);
  },
};

// A production build may offer the bundled sample scenarios only after the
// user explicitly chooses the visible test-mode action. Keep this module-local
// so a query string or an injected window value cannot silently enable them.
let userActivatedTrainingFixture = false;

function explicitAdapter(): ScenarioTrainingApiAdapter | null {
  if (typeof window === "undefined") return null;
  if (userActivatedTrainingFixture) return fixtureAdapter;
  // Acceptance fixtures are deliberately bundled for browser E2E, but a
  // production desktop build must never be switchable to sample evidence
  // through a query string or injected window value.
  if (!import.meta.env.DEV && import.meta.env.MODE !== "test") return null;
  const injected = window.__SCENARIO_TRAINING_TEST_FIXTURE__;
  if (injected && typeof injected === "object") return injected;
  const queryName = new URLSearchParams(window.location.search).get("trainingFixture");
  const name = typeof injected === "string" ? injected : queryName;
  return name === "acceptance-v1" ? fixtureAdapter : null;
}

export function canActivateTrainingFixture(): boolean {
  return typeof window !== "undefined";
}

export function activateTrainingFixtureForTesting(): boolean {
  if (!canActivateTrainingFixture()) return false;
  userActivatedTrainingFixture = true;
  return true;
}

function api(): ScenarioTrainingApiAdapter {
  return explicitAdapter() ?? tauriAdapter;
}

export function isExplicitTrainingFixtureActive(): boolean {
  return explicitAdapter() === fixtureAdapter;
}

export async function listTrainingRoles(): Promise<TrainingRole[]> {
  return api().listTrainingRoles();
}

export async function startTrainingSession(request: StartTrainingRequest): Promise<TrainingStartResult> {
  return api().startTrainingSession(request);
}

export async function submitTrainingAnswer(request: {
  sessionId: string;
  traineeAnswer: string;
}): Promise<TrainingFeedback> {
  return api().submitTrainingAnswer(request);
}

export async function nextTrainingQuestion(request: {
  sessionId: string;
}): Promise<TrainingNextResult> {
  return api().nextTrainingQuestion(request);
}

export async function completeTrainingSession(request: {
  sessionId: string;
}): Promise<TrainingSummary> {
  return api().completeTrainingSession(request);
}

export async function abandonTrainingSession(request: { sessionId: string }): Promise<void> {
  return api().abandonTrainingSession(request);
}

export async function startHumanMachineScenario(request: {
  roleId: string;
  selectedModule?: TrainingModuleKey;
  seed?: number;
  totalTurns?: HumanMachineTurnCount;
}): Promise<HumanMachineScenario> {
  return api().startHumanMachineScenario(request);
}

export async function submitHumanMachineTurn(request: {
  runId: string;
  traineeAnswer: string;
}): Promise<HumanMachineScenario> {
  return api().submitHumanMachineTurn(request);
}

export async function getActiveHumanMachineScenario(request: {
  roleId: string;
}): Promise<HumanMachineScenario | null> {
  return api().getActiveHumanMachineScenario(request);
}

export async function abandonHumanMachineScenario(request: { runId: string }): Promise<void> {
  return api().abandonHumanMachineScenario(request);
}

export function trainingClipUrl(path?: string): string {
  if (!path) return "";
  if (/^(data:|blob:|https?:|asset:)/i.test(path) || path.startsWith("/")) return path;
  try {
    return convertFileSrc(path);
  } catch {
    return "";
  }
}
