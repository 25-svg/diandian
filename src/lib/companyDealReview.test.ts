import assert from "node:assert/strict";
import { canOpenCompanyDealReview } from "./companyDealReview.js";

assert.equal(canOpenCompanyDealReview("company"), true);
assert.equal(canOpenCompanyDealReview("competitor"), false);
assert.equal(canOpenCompanyDealReview(undefined), false);

console.log("company deal review tests passed");
