import assert from "node:assert/strict";
import {
  pruneArchiveSelection,
  selectArchiveRange,
} from "./archiveSelection.js";

assert.deepEqual(
  [...selectArchiveRange(["a", "b", "c"], new Set(), "c", "a", false)],
  ["a", "b", "c"]
);

assert.deepEqual(
  [
    ...selectArchiveRange(
      ["a", "b", "c"],
      new Set(["outside"]),
      "c",
      "b",
      true
    ),
  ],
  ["outside", "b", "c"]
);

assert.deepEqual(
  [...selectArchiveRange(["a", "b"], new Set(["a"]), "a", null, false)],
  []
);

assert.deepEqual(
  [...pruneArchiveSelection(new Set(["a", "outside"]), ["a", "b"])],
  ["a"]
);

console.log("archive selection tests passed");
