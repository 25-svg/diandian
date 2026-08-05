import assert from "node:assert/strict";
import { scanComplianceRisks } from "./complianceRules.js";

const findings = scanComplianceRisks([
  { start: 12, end: 16, text: "这台机器绝对没问题，是全网最低。" },
  { start: 30, end: 35, text: "这台说是官方在保，需要再查一下。" },
  { start: 50, end: 55, text: "我们给你看检测报告。" },
]);

assert.equal(findings.length, 2);
assert.equal(findings[0].severity, "blocked");
assert.match(findings[0].quote, /全网最低/);
assert.equal(findings[1].severity, "review");
assert.match(findings[1].guidance, /核对商品参数卡/);

console.log("complianceRules tests passed");
