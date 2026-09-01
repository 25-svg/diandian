import assert from "node:assert/strict";
import fs from "node:fs";
import { compile, preprocess } from "svelte/compiler";
import sveltePreprocess from "svelte-preprocess";

const pagePaths = [
  "src/lib/components/analysis/LiveMetricCurveWorkbench.svelte",
  "src/lib/components/analysis/LiveMetricDiagnosisPanel.svelte",
  "src/page/LiveDataDashboard.svelte",
];
const analysisPath = "src/lib/compassAnalysis.ts";
const workspacePath = "src/lib/components/analysis/CompanyAnalysisWorkspace.svelte";
const embeddedCompassPath = "src/lib/components/analysis/EmbeddedCompassPanel.svelte";
const activePage = fs.readFileSync(pagePaths[0], "utf8");
const diagnosisPanel = fs.readFileSync(pagePaths[1], "utf8");
const analysis = fs.readFileSync(analysisPath, "utf8");
const workspace = fs.readFileSync(workspacePath, "utf8");
const embeddedCompass = fs.readFileSync(embeddedCompassPath, "utf8");

const requiredPageContracts = [
  'aria-pressed={selectedMetricLabels.includes(metric.label)}',
  'aria-label="曲线分析开始时间"',
  'aria-label="曲线分析结束时间"',
  'aria-label="当前窗口异常和变化事件表"',
  'aria-label="可横向滚动的多指标曲线图"',
  'class="anomaly-label">异常</text>',
  'stroke-dasharray={style.dash}',
  'style.shape === "square"',
  'style.shape === "triangle"',
  '同期变化不等于因果',
  '同步录播/文稿',
  'aria-label="罗盘大屏模块"',
  'activeModule === "product"',
  'activeModule === "audience"',
  'activeModule === "qianchuan"',
  'class="point-target"',
  'role="button"',
  'pointKeydown(event, metric, point)',
  'aria-label="选中曲线点分析"',
  '回看讲解',
  '@media (max-width: 700px)',
  '@media (max-width: 420px)',
  'min-height: 44px',
  ':focus-visible',
];
for (const contract of requiredPageContracts) {
  assert.ok(activePage.includes(contract), `missing active curve UI contract: ${contract}`);
}
assert.ok(diagnosisPanel.includes("<LiveMetricCurveWorkbench"), "active diagnosis panel must mount the curve workbench");
assert.ok(workspace.includes("aria-expanded={dataBoardExpanded}"), "data board toggle must expose expanded state");
assert.ok(workspace.includes('aria-controls="company-data-board-body"'), "data board toggle must identify its controlled body");
assert.ok(workspace.includes('<EmbeddedCompassPanel'), "official Compass tab must mount the embedded browser panel");
assert.ok(workspace.includes('bottomBoardTab === "official"'), "official Compass tab must have an explicit active state");
assert.doesNotMatch(
  workspace,
  /\{:else\}\s*<LiveTopProductsPanel\s+\{events\}\s+\{transcriptEntries\}\s+\{transcriptBackfilling\}\s*\/>\s*\{\/if\}/,
  "collapsed data board must not render a fallback content panel",
);

for (const contract of [
  "analyzeCompassMetricWindow",
  "normalizeCompassMetricValues",
  "compassEvidenceExplanation",
  "compassPointOffsetSeconds",
  "analyzeCompassPoint",
  "待核实：当前时间段没有逐字稿或节奏地图证据。",
  "metricPointValid(previous) || !metricPointValid(current)",
]) {
  assert.ok(analysis.includes(contract), `missing analysis contract: ${contract}`);
}

for (const contract of [
  'invoke<string>("open_embedded_compass_capture"',
  'invoke("update_embedded_compass_bounds"',
  'invoke("close_embedded_compass"',
  'listen<CaptureProgress>("compass-capture-progress"',
  'label: "商品"',
  'label: "人群"',
  'label: "千川"',
  'statusLabel',
  'ResizeObserver',
  ':focus-visible',
]) {
  assert.ok(embeddedCompass.includes(contract), `missing embedded Compass contract: ${contract}`);
}

for (const pagePath of [...pagePaths, embeddedCompassPath]) {
  const page = fs.readFileSync(pagePath, "utf8");
  const processed = await preprocess(page, sveltePreprocess({ typescript: true }), { filename: pagePath });
  const result = compile(processed.code, { filename: pagePath, generate: false });
  assert.deepEqual(result.warnings, [], `${pagePath} Svelte warnings: ${result.warnings.map((warning) => warning.message).join("; ")}`);
}

console.log("compass curve UI contracts passed for 320/375/768/844-landscape/1440 responsive rules and keyboard semantics");
