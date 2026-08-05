import type { MasterBaseline, MasterSection, SupportCandidate } from "./masterScript";

export type TrainingPackSection = {
  sectionKey: string;
  title: string;
  standardExample: string;
  goldenSegments: Array<{
    id: number;
    text: string;
    score: number;
    sourceKey: string;
    startMs: number;
    endMs: number;
  }>;
  discussionQuestions: string[];
  practiceRequirement: string;
  replayChecklist: string[];
};

export type TrainingPack = {
  title: string;
  masterVersion: string;
  generatedAt: string;
  sections: TrainingPackSection[];
};

function practiceFor(section: MasterSection): string {
  if (section.sectionKind === "product") return "用自己的语言练习“先问需求、再讲价值”，不得机械背诵范例。";
  if (section.sectionKind === "scenario") return "两人模拟顾客顾虑，练习先回应问题，再给可信证据和下一步。";
  if (section.sectionKind === "closing") return "练习给出清楚的下一步动作，未看到证据时不得宣称已经成交。";
  if (section.sectionKind === "opening") return "练习在 30 秒内说明本场价值，并给观众继续停留的理由。";
  return "用自己的语言复述场景目标，练习自然承接前后内容。";
}

export function buildTrainingPack(input: {
  baseline: MasterBaseline;
  candidates: readonly SupportCandidate[];
  generatedAt: string;
}): TrainingPack {
  const approved = input.candidates.filter((candidate) =>
    ["approved", "merged"].includes(candidate.status) && candidate.totalScore >= 85
  );
  const sections = [...input.baseline.sections]
    .sort((left, right) => left.position - right.position)
    .map((section) => {
      const goldenSegments = approved
        .filter((candidate) => candidate.masterSectionId === section.id)
        .map((candidate) => ({
          id: candidate.id,
          text: candidate.hostText,
          score: candidate.totalScore,
          sourceKey: candidate.sourceKey,
          startMs: candidate.sourceStartMs,
          endMs: candidate.sourceEndMs,
        }));
      return {
        sectionKey: section.sectionKey,
        title: section.title,
        standardExample: section.masterText,
        goldenSegments,
        discussionQuestions: [
          "这段话术解决了顾客的什么问题？",
          goldenSegments.length
            ? "金牌片段比企业母稿新增了什么表达或承接方法？"
            : "本章节还缺少哪类真实片段证据？",
          "哪些型号、价格、库存、链接或承诺必须按当场事实确认？",
        ],
        practiceRequirement: practiceFor(section),
        replayChecklist: [
          "表达是否保持主播自己的语言习惯",
          "关键事实是否可追溯且已确认",
          "是否自然推动到该场景的下一步",
        ],
      };
    });

  return {
    title: `${input.baseline.master.title}｜母稿培养包`,
    masterVersion: input.baseline.master.version,
    generatedAt: input.generatedAt,
    sections,
  };
}

function time(ms: number): string {
  const totalSeconds = Math.max(0, Math.floor(ms / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = totalSeconds % 60;
  return hours
    ? `${String(hours).padStart(2, "0")}:${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`
    : `${String(minutes).padStart(2, "0")}:${String(seconds).padStart(2, "0")}`;
}

function bullets(items: readonly string[]): string {
  return items.map((item) => `- ${item}`).join("\n");
}

export function formatTrainingPackMarkdown(pack: TrainingPack): string {
  const sections = pack.sections.map((section, index) => {
    const golden = section.goldenSegments.length
      ? section.goldenSegments.map((segment) => [
        `### 金牌片段 ${segment.id}｜${segment.score} 分`,
        `- 来源：${segment.sourceKey}｜${time(segment.startMs)}—${time(segment.endMs)}`,
        `> ${segment.text}`,
      ].join("\n")).join("\n\n")
      : "暂无已审定高分片段。";
    return [
      `## ${index + 1}. ${section.title}`,
      "",
      "### 企业母稿范例",
      section.standardExample,
      "",
      "### 已审定金牌片段",
      golden,
      "",
      "### 培训讨论题",
      bullets(section.discussionQuestions),
      "",
      "### 练习要求",
      section.practiceRequirement,
      "",
      "### 再播检查项",
      bullets(section.replayChecklist),
    ].join("\n");
  }).join("\n\n---\n\n");

  return [
    `# ${pack.title}`,
    "",
    `- 母稿版本：V${pack.masterVersion}`,
    `- 生成时间：${pack.generatedAt}`,
    "",
    "> 本培养包只包含企业母稿与人工审定素材。主播应理解场景与方法后用自己的语言表达，不得把 AI 建议当作标准稿。",
    "",
    sections,
    "",
  ].join("\n");
}

export function trainingPackFileName(pack: Pick<TrainingPack, "title" | "masterVersion">): string {
  const safe = pack.title.replace(/[<>:"/\\|?*\u0000-\u001f]/g, "_");
  return `${safe}-V${pack.masterVersion}.md`;
}
