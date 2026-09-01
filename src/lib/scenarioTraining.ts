export const TRAINING_MODULES = [
  { key: "opening", label: "开场", description: "建立第一印象，快速说明本场价值。" },
  { key: "retention_interaction", label: "留人互动", description: "回应观众、制造参与感并稳住在线。" },
  { key: "needs_discovery", label: "需求确认", description: "追问预算、用途和真实顾虑。" },
  { key: "product_explanation", label: "产品讲解", description: "围绕已确认事实讲清核心价值。" },
  { key: "objection_handling", label: "异议处理", description: "处理价格、成色、配件和售后疑问。" },
  { key: "conversion", label: "成交推进", description: "给出明确、合规且不过度催促的下一步。" },
  { key: "transition", label: "转品衔接", description: "自然结束当前商品并承接下一件。" },
  { key: "incident", label: "突发情况", description: "应对链接、库存、投诉和节奏中断。" },
] as const;

export type TrainingModuleKey = (typeof TRAINING_MODULES)[number]["key"];
export type TrainingMode = "comprehensive" | "specialized";
export type TrainingStep = "roles" | "experience" | "setup" | "question" | "feedback" | "summary";
export type TrainingExperience = "evidence_quiz" | "human_machine";

export const SCORE_DIMENSIONS = [
  { key: "needsConfirmation", label: "需求确认" },
  { key: "factAccuracy", label: "事实准确" },
  { key: "trustBuilding", label: "信任建立" },
  { key: "expressionClarity", label: "表达清晰" },
  { key: "dealAdvancement", label: "成交推进" },
  { key: "riskCompliance", label: "风险合规" },
  { key: "livePacing", label: "直播节奏" },
] as const;

export type ScoreDimensionKey = (typeof SCORE_DIMENSIONS)[number]["key"];
export type TrainingScores = Record<ScoreDimensionKey, number>;

export interface TrainingRole {
  id: string;
  displayName: string;
  sourceLabel?: string;
  description?: string;
  avatarUrl?: string;
  availableCaseCount: number;
  availableModules: TrainingModuleKey[];
  evidenceStatus?: "ready" | "empty" | "pending";
  unavailableReason?: string;
}

export interface TrainingCase {
  id: string;
  roleId: string;
  module: TrainingModuleKey;
  prompt: string;
  viewerRole?: string;
  realAnswer?: string;
  clipPath?: string;
  evidenceId?: string;
  reviewStatus: "approved" | "pending" | "rejected";
  roleIdentityConfirmed: boolean;
  commentConfirmed: boolean;
  answerConfirmed: boolean;
  clipPlayable: boolean;
  factsConfirmed: boolean;
  moduleConfirmed: boolean;
}

export interface TrainingQuestion {
  caseId: string;
  module: TrainingModuleKey;
  prompt: string;
  viewerRole?: string;
  sourceKind?: "approved_comment";
}

export interface TrainingSession {
  sessionId: string;
  roleId: string;
  displayName: string;
  mode: TrainingMode;
  selectedModule?: TrainingModuleKey;
  currentIndex: number;
  totalQuestions: number;
}

export interface TrainingStartResult extends TrainingSession {
  question: TrainingQuestion;
}

export interface TrainingNextResult extends TrainingSession {
  question: TrainingQuestion | null;
  isComplete: boolean;
}

export interface TrainingFeedback {
  caseId: string;
  scores: TrainingScores | Record<string, unknown>;
  scoresValid: boolean;
  priorityImprovement: string;
  coachSuggestion: string;
  realAnswer: string;
  clipPath?: string;
  evidenceId: string;
  evaluationStatus?: "complete" | "needs_review";
}

export interface TrainingModuleScore {
  module: TrainingModuleKey;
  score: number;
  dimensionScores?: Partial<TrainingScores>;
}

export interface TrainingSummary {
  sessionId: string;
  roleId: string;
  displayName: string;
  mode: TrainingMode;
  selectedModule?: TrainingModuleKey;
  answeredCount: number;
  moduleScores: TrainingModuleScore[];
  evidenceIds: string[];
  recurringIssues: string[];
  nextTrainingSuggestions: string[];
}

export interface HumanMachineTurn {
  turnIndex: number;
  viewerMessage: string;
  viewerSource: "approved_comment" | "ai_simulated_follow_up";
  followUpFocus?: HumanMachineFollowUpFocus;
  traineeAnswer?: string;
}

export type HumanMachineTurnCount = 3 | 5;

export type HumanMachineFollowUpFocus =
  | "adaptive_follow_up"
  | "needs_confirmation"
  | "fact_clarification"
  | "trust_building"
  | "objection_resolution"
  | "deal_advancement"
  | "risk_compliance"
  | "live_pacing";

export const HUMAN_MACHINE_TURN_OPTIONS: readonly HumanMachineTurnCount[] = [3, 5];

export function normalizeHumanMachineTurnCount(value: unknown): HumanMachineTurnCount {
  return Number(value) === 5 ? 5 : 3;
}

export interface HumanMachineHardCheck {
  key: "needs_confirmation" | "fact_boundary" | "compliant_next_step";
  label: string;
  passed: boolean;
  detail: string;
}

export interface HumanMachineScenario {
  runId: string;
  roleId: string;
  displayName: string;
  module: TrainingModuleKey;
  currentTurn: number;
  totalTurns: HumanMachineTurnCount;
  status: "active" | "completed" | "abandoned";
  turns: HumanMachineTurn[];
  factConstraints: Record<string, unknown>;
  hardChecks: HumanMachineHardCheck[];
  evaluationStatus: "pending" | "scored" | "needs_review";
  scores: TrainingScores | Record<string, unknown> | null;
  scoresValid: boolean;
  priorityImprovement?: string;
  coachSuggestion?: string;
  realAnswer: string;
  clipPath?: string;
  evidenceId: string;
}

export interface ScenarioTrainingState {
  step: TrainingStep;
  selectedRoleId: string | null;
  mode: TrainingMode;
  selectedModule: TrainingModuleKey | null;
  sessionId: string | null;
  question: TrainingQuestion | null;
  answer: string;
  feedback: TrainingFeedback | null;
  summary: TrainingSummary | null;
}

export function getTrainingModule(key?: string | null) {
  return TRAINING_MODULES.find((item) => item.key === key);
}

export function isTrainingModuleKey(value: unknown): value is TrainingModuleKey {
  return typeof value === "string" && TRAINING_MODULES.some((item) => item.key === value);
}

export function isTrainingRoleAvailable(role: TrainingRole): boolean {
  return role.availableCaseCount > 0 && role.evidenceStatus !== "empty";
}

export function filterTrainableRoles(roles: TrainingRole[]): TrainingRole[] {
  return roles.filter(isTrainingRoleAvailable);
}

export function isApprovedTrainingCase(caseItem: TrainingCase): boolean {
  return (
    caseItem.reviewStatus === "approved" &&
    caseItem.roleIdentityConfirmed &&
    caseItem.commentConfirmed &&
    caseItem.answerConfirmed &&
    caseItem.clipPlayable &&
    caseItem.factsConfirmed &&
    caseItem.moduleConfirmed &&
    Boolean(caseItem.prompt.trim()) &&
    Boolean(caseItem.realAnswer?.trim()) &&
    Boolean(caseItem.clipPath?.trim()) &&
    Boolean(caseItem.evidenceId?.trim())
  );
}

function seededRank(seed: number, text: string): number {
  let hash = seed >>> 0;
  for (let index = 0; index < text.length; index += 1) {
    hash = Math.imul(hash ^ text.charCodeAt(index), 2654435761) >>> 0;
  }
  return hash;
}

export function selectTrainingCases(
  cases: TrainingCase[],
  options: {
    roleId: string;
    mode: TrainingMode;
    selectedModule?: TrainingModuleKey | null;
    limit?: number;
    seed?: number;
  },
): TrainingCase[] {
  const selected = cases.filter((caseItem) => {
    if (!isApprovedTrainingCase(caseItem) || caseItem.roleId !== options.roleId) return false;
    if (options.mode === "specialized") {
      return Boolean(options.selectedModule) && caseItem.module === options.selectedModule;
    }
    return true;
  });

  const seed = Number.isFinite(options.seed) ? Number(options.seed) : 20260828;
  selected.sort((left, right) => seededRank(seed, left.id) - seededRank(seed, right.id));
  return selected.slice(0, Math.max(0, options.limit ?? selected.length));
}

export function createEmptyTrainingState(): ScenarioTrainingState {
  return {
    step: "roles",
    selectedRoleId: null,
    mode: "comprehensive",
    selectedModule: null,
    sessionId: null,
    question: null,
    answer: "",
    feedback: null,
    summary: null,
  };
}

export function resetTrainingStateForRoleSwitch(
  _state?: ScenarioTrainingState,
): ScenarioTrainingState {
  return createEmptyTrainingState();
}

export function validateTrainingScores(
  scores: unknown,
): { valid: boolean; scores: TrainingScores | null; missing: ScoreDimensionKey[] } {
  const source = scores && typeof scores === "object" ? (scores as Record<string, unknown>) : {};
  const missing = SCORE_DIMENSIONS.filter(({ key }) => {
    const value = source[key];
    return typeof value !== "number" || !Number.isFinite(value) || value < 1 || value > 5;
  }).map(({ key }) => key);
  if (missing.length > 0) return { valid: false, scores: null, missing };
  const normalized = Object.fromEntries(
    SCORE_DIMENSIONS.map(({ key }) => [key, Number(source[key])]),
  ) as TrainingScores;
  return { valid: true, scores: normalized, missing: [] };
}

export function buildFactSafetyInstruction(confirmedFacts: string[]): string {
  const facts = confirmedFacts.map((item) => item.trim()).filter(Boolean);
  if (facts.length === 0) {
    return "商品参数、价格、库存、赠品或售后资料不足。只能追问或标记待确认，禁止补造事实。";
  }
  return `只能使用以下已确认事实：${facts.join("；")}。未覆盖的信息必须追问或标记待确认。`;
}

export function normalizeTrainingViewerRole(value: unknown): string {
  if (typeof value !== "string") return "直播间观众";
  const normalized = value.replace(/\s+/gu, " ").trim();
  return normalized && normalized.length <= 40 ? normalized : "直播间观众";
}
