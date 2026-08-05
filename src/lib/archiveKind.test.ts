import assert from "node:assert/strict";
import { applyLoadedArchiveClassification, classifyArchiveKind, preferAutoClassifiedArchives } from "./archiveKind.js";

assert.deepEqual(classifyArchiveKind({ title: "金典拍拍相机专卖店直播", anchorName: "" }), {
  archiveKind: "company",
  classificationSource: "auto_rule",
});
assert.deepEqual(classifyArchiveKind({ title: "二手镜头专场", anchorName: "金典拍拍主播" }), {
  archiveKind: "company",
  classificationSource: "auto_rule",
});
assert.deepEqual(classifyArchiveKind({ title: "别家相机直播", anchorName: "" }), {
  archiveKind: "competitor",
  classificationSource: "auto_rule",
});
assert.deepEqual(classifyArchiveKind({
  title: "二手相机专场好成色不等人～",
  anchorName: "",
  accountName: "金典拍拍相机专卖店-复古相机专场",
}), {
  archiveKind: "company",
  classificationSource: "auto_rule",
});

assert.deepEqual(
  preferAutoClassifiedArchives(
    [{ live_id: "a", archive_kind: "company" }],
    null,
  ),
  [{ live_id: "a", archive_kind: "company" }],
);

assert.equal(
  classifyArchiveKind({ accountName: "\u91d1\u5178\u62cd\u62cd\u76f8\u673a\u4e13\u5356\u5e97" }).archiveKind,
  "company",
);

assert.equal(
  applyLoadedArchiveClassification(
    { archive_kind: "competitor", classification_source: "auto_rule", title: "富士相机专场", anchor_name: "" },
    "金典拍拍相机专卖店",
  ).archive_kind,
  "company",
);
assert.equal(
  applyLoadedArchiveClassification(
    { archive_kind: "competitor", classification_source: "manual", title: "金典拍拍相机专卖店", anchor_name: "" },
    "金典拍拍相机专卖店",
  ).archive_kind,
  "competitor",
);

console.log("archive kind tests passed");
