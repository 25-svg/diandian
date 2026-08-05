import assert from "node:assert/strict";
import { normalizeAnalysisZoom, stepAnalysisZoom } from "./analysisZoom.js";

assert.equal(normalizeAnalysisZoom(undefined), 100);
assert.equal(normalizeAnalysisZoom(null), 100);
assert.equal(normalizeAnalysisZoom("bad"), 100);
assert.equal(normalizeAnalysisZoom("120"), 120);
assert.equal(normalizeAnalysisZoom(79), 80);
assert.equal(normalizeAnalysisZoom(151), 150);
assert.equal(normalizeAnalysisZoom(124), 120);
assert.equal(stepAnalysisZoom(100, 1), 110);
assert.equal(stepAnalysisZoom(100, -1), 90);
assert.equal(stepAnalysisZoom(150, 1), 150);
assert.equal(stepAnalysisZoom(80, -1), 80);
