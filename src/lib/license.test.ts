import assert from "node:assert/strict";
import { resolveLicenseView, parseLicenseStatus, createLicenseClient } from "./license.js";

async function main() {
  assert.equal(resolveLicenseView({ status: "unactivated" }), "activation");
  assert.equal(resolveLicenseView({ status: "offline_grace", daysRemaining: 2 }), "app-with-warning");
  assert.equal(resolveLicenseView({ status: "expired" }), "blocked");
  assert.equal(resolveLicenseView({ status: "revoked" }), "blocked");
  assert.equal(resolveLicenseView({ status: "valid" }), "app");
  for (const value of [null, {}, { status: "future" }, { status: "clock_invalid" }]) {
    assert.equal(resolveLicenseView(value), "blocked");
  }
  for (const value of [{ status: "valid" }, { status: "valid", daysRemaining: null, message: "secret", deviceToken: "secret" }, { status: "offline_grace", daysRemaining: 8, message: "" }]) {
    assert.equal(resolveLicenseView(parseLicenseStatus(value)), "blocked");
    assert.ok(!JSON.stringify(parseLicenseStatus(value)).includes("secret"));
  }
  assert.equal(parseLicenseStatus({ status: "valid", daysRemaining: null, message: "secret" }).message, "设备已授权。");
  const failed = createLicenseClient(async () => { throw new Error("Bearer secret"); });
  assert.equal((await failed.activate("abc", "name")).message, "授权操作未完成，请检查网络后重试。");
  let finish!: (value: unknown) => void;
  let calls = 0;
  const client = createLicenseClient(() => { calls++; return new Promise(resolve => { finish = resolve; }); });
  const first = client.renew();
  const duplicate = client.renew();
  assert.equal(first, duplicate);
  assert.equal(calls, 1);
  finish({ status: "offline_grace", daysRemaining: 2, message: "" });
  assert.equal((await first).daysRemaining, 2);
  console.log("license state / runtime boundary / concurrency tests passed");
}
void main();
