import assert from "node:assert/strict";
import { isMissingArchiveError } from "./taskNavigation.js";

assert.equal(isMissingArchiveError("Database error: Entry not found"), true);
assert.equal(
  isMissingArchiveError(
    "Database error: DB error: no rows returned by a query that expected to return at least one row",
  ),
  true,
);
assert.equal(isMissingArchiveError("network timeout"), false);
