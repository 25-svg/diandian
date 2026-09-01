import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import {
  SCORE_DIMENSIONS,
  buildFactSafetyInstruction,
  createEmptyTrainingState,
  filterTrainableRoles,
  isApprovedTrainingCase,
  isTrainingRoleAvailable,
  normalizeTrainingViewerRole,
  normalizeHumanMachineTurnCount,
  resetTrainingStateForRoleSwitch,
  selectTrainingCases,
  validateTrainingScores,
  type ScenarioTrainingState,
} from "./scenarioTraining.js";
import {
  TRAINING_ACCEPTANCE_APPROVED_CASES,
  TRAINING_ACCEPTANCE_CASES,
  TRAINING_ACCEPTANCE_CONFIRMED_FACTS,
  TRAINING_ACCEPTANCE_INVALID_SCORES,
  TRAINING_ACCEPTANCE_MEDIA_PATH,
  TRAINING_ACCEPTANCE_PASS_CRITERIA,
  TRAINING_ACCEPTANCE_REJECTED_CASES,
  TRAINING_ACCEPTANCE_ROLES,
  TRAINING_ACCEPTANCE_SCORES,
  TRAINING_ACCEPTANCE_SEED,
} from "../../tests/fixtures/training-acceptance-v1.js";

const [luo, hou, xiaoe, yu] = TRAINING_ACCEPTANCE_ROLES;

const fixtureMedia = readFileSync(resolve(process.cwd(), TRAINING_ACCEPTANCE_MEDIA_PATH.slice(1)));
assert.ok(fixtureMedia.length > 1_000);
assert.deepEqual([...fixtureMedia.subarray(0, 4)], [0x1a, 0x45, 0xdf, 0xa3]);
assert.match(fixtureMedia.toString("latin1"), /AUTOMATED TEST FIXTURE - NOT REAL EVIDENCE/);

assert.equal(TRAINING_ACCEPTANCE_PASS_CRITERIA.length, 14);
assert.equal(
  TRAINING_ACCEPTANCE_ROLES.every(
    (role) => role.sourceLabel?.startsWith("【自动测试夹具】") && role.description?.startsWith("【自动测试夹具】"),
  ),
  true,
);
assert.equal(
  TRAINING_ACCEPTANCE_CASES.every(
    (item) =>
      item.prompt.startsWith("【自动测试夹具】") &&
      item.viewerRole?.startsWith("【自动测试夹具】") &&
      (!item.realAnswer || item.realAnswer.startsWith("【自动测试夹具】")) &&
      (!item.evidenceId || item.evidenceId.startsWith("AUTO-EVIDENCE-")),
  ),
  true,
);

assert.equal(isTrainingRoleAvailable(luo), true);
assert.equal(isTrainingRoleAvailable(hou), true);
assert.equal(isTrainingRoleAvailable(xiaoe), true);
assert.equal(isTrainingRoleAvailable(yu), false);
assert.deepEqual(
  filterTrainableRoles(TRAINING_ACCEPTANCE_ROLES).map((role) => role.id),
  ["ROLE-LUO", "ROLE-HOU", "ROLE-XIAOE"],
);

assert.equal(TRAINING_ACCEPTANCE_APPROVED_CASES.every(isApprovedTrainingCase), true);
assert.equal(TRAINING_ACCEPTANCE_REJECTED_CASES.some(isApprovedTrainingCase), false);

const specialized = selectTrainingCases(TRAINING_ACCEPTANCE_CASES, {
  roleId: "ROLE-LUO",
  mode: "specialized",
  selectedModule: "objection_handling",
  seed: TRAINING_ACCEPTANCE_SEED,
});
assert.equal(specialized.length, 1);
assert.equal(specialized[0].roleId, "ROLE-LUO");
assert.equal(specialized[0].module, "objection_handling");
assert.equal(specialized[0].id, "LUO-objection_handling");

const comprehensive = selectTrainingCases(TRAINING_ACCEPTANCE_CASES, {
  roleId: "ROLE-LUO",
  mode: "comprehensive",
  seed: TRAINING_ACCEPTANCE_SEED,
});
assert.equal(comprehensive.length, 8);
assert.equal(comprehensive.every((item) => item.roleId === "ROLE-LUO"), true);
assert.equal(new Set(comprehensive.map((item) => item.module)).size, 8);
assert.equal(comprehensive.some((item) => item.id.startsWith("INVALID-")), false);
assert.deepEqual(
  selectTrainingCases(TRAINING_ACCEPTANCE_CASES, {
    roleId: "ROLE-LUO",
    mode: "comprehensive",
    seed: TRAINING_ACCEPTANCE_SEED,
  }).map((item) => item.id),
  comprehensive.map((item) => item.id),
);

assert.deepEqual(
  selectTrainingCases(TRAINING_ACCEPTANCE_CASES, {
    roleId: "ROLE-YU",
    mode: "comprehensive",
    seed: TRAINING_ACCEPTANCE_SEED,
  }),
  [],
);
assert.deepEqual(
  selectTrainingCases(TRAINING_ACCEPTANCE_CASES, {
    roleId: "ROLE-HOU",
    mode: "specialized",
    selectedModule: "opening",
    seed: TRAINING_ACCEPTANCE_SEED,
  }),
  [],
);

assert.deepEqual(createEmptyTrainingState(), {
  step: "roles",
  selectedRoleId: null,
  mode: "comprehensive",
  selectedModule: null,
  sessionId: null,
  question: null,
  answer: "",
  feedback: null,
  summary: null,
});

const dirtyState: ScenarioTrainingState = {
  step: "feedback",
  selectedRoleId: "ROLE-HOU",
  mode: "specialized",
  selectedModule: "incident",
  sessionId: "AUTO-SESSION-1",
  question: {
    caseId: "HOU-incident",
    module: "incident",
    prompt: "【自动测试夹具】旧问题",
  },
  answer: "【自动测试夹具】旧回答",
  feedback: {
    caseId: "HOU-incident",
    scores: TRAINING_ACCEPTANCE_SCORES,
    scoresValid: true,
    priorityImprovement: "【自动测试夹具】旧改进点",
    coachSuggestion: "【自动测试夹具】旧建议",
    realAnswer: "【自动测试夹具】旧参考回答",
    evidenceId: "AUTO-EVIDENCE-HOU-incident",
  },
  summary: null,
};
assert.deepEqual(resetTrainingStateForRoleSwitch(dirtyState), createEmptyTrainingState());

const validScores = validateTrainingScores(TRAINING_ACCEPTANCE_SCORES);
assert.equal(validScores.valid, true);
assert.deepEqual(validScores.missing, []);
assert.deepEqual(validScores.scores, TRAINING_ACCEPTANCE_SCORES);
assert.equal(SCORE_DIMENSIONS.length, 7);

const missingScores = validateTrainingScores(TRAINING_ACCEPTANCE_INVALID_SCORES);
assert.equal(missingScores.valid, false);
assert.equal(missingScores.scores, null);
assert.deepEqual(missingScores.missing, ["livePacing"]);
assert.equal(
  validateTrainingScores({ ...TRAINING_ACCEPTANCE_SCORES, riskCompliance: 6 }).valid,
  false,
);

const noFacts = buildFactSafetyInstruction([]);
assert.match(noFacts, /只能追问或标记待确认/);
assert.match(noFacts, /禁止补造事实/);
const confirmedFacts = buildFactSafetyInstruction([...TRAINING_ACCEPTANCE_CONFIRMED_FACTS]);
assert.match(confirmedFacts, /只能使用以下已确认事实/);
assert.match(confirmedFacts, /未覆盖的信息必须追问或标记待确认/);
assert.equal(confirmedFacts.includes("价格"), false);

assert.equal(normalizeTrainingViewerRole("  潜在买家\n "), "潜在买家");
assert.equal(normalizeTrainingViewerRole(""), "直播间观众");
assert.equal(normalizeTrainingViewerRole("x".repeat(41)), "直播间观众");
assert.equal(normalizeHumanMachineTurnCount(3), 3);
assert.equal(normalizeHumanMachineTurnCount(5), 5);
assert.equal(normalizeHumanMachineTurnCount(4), 3);

console.log("scenario training domain tests passed");
