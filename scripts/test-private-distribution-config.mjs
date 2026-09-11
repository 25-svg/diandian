import assert from "node:assert/strict";
import { mkdtemp, readFile, rm } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import http from "node:http";
import { validateReleaseEnvironment, auditClientFiles } from "./private-distribution-config.mjs";
import { parseSmokeArgs, runSmoke } from "../distribution/control-plane/scripts/smoke.mjs";

const valid = {
  DIANDIAN_UPDATE_ENDPOINT: "https://updates.example.com/",
  DIANDIAN_LICENSE_PUBLIC_KEY:
    "BKeOTfnMcq2y3_jgJSTkWgVjRo-ALAmxJ3k1JwF-i5kF2ae6pA6gBJtDFG_-fm3kAvNu8qfwgddtZ86h-VeMWhQ",
};

for (const [environment, expected] of [
  [{}, /DIANDIAN_UPDATE_ENDPOINT.*DIANDIAN_LICENSE_PUBLIC_KEY/s],
  [{ DIANDIAN_LICENSE_PUBLIC_KEY: valid.DIANDIAN_LICENSE_PUBLIC_KEY }, /DIANDIAN_UPDATE_ENDPOINT/],
  [{ DIANDIAN_UPDATE_ENDPOINT: valid.DIANDIAN_UPDATE_ENDPOINT }, /DIANDIAN_LICENSE_PUBLIC_KEY/],
]) {
  assert.throws(() => validateReleaseEnvironment(environment), expected);
}

for (const endpoint of [
  "http://updates.example.com/",
  "https://user@updates.example.com/",
  "https://updates.example.com/v1",
  "https://updates.example.com/?channel=prod",
  "https://updates.example.com/#prod",
]) {
  assert.throws(
    () => validateReleaseEnvironment({ ...valid, DIANDIAN_UPDATE_ENDPOINT: endpoint }),
    /DIANDIAN_UPDATE_ENDPOINT/,
  );
}

for (const key of ["", "AA", `${valid.DIANDIAN_LICENSE_PUBLIC_KEY}=`, `A${valid.DIANDIAN_LICENSE_PUBLIC_KEY.slice(1)}`]) {
  assert.throws(
    () => validateReleaseEnvironment({ ...valid, DIANDIAN_LICENSE_PUBLIC_KEY: key }),
    /DIANDIAN_LICENSE_PUBLIC_KEY/,
  );
}

assert.deepEqual(validateReleaseEnvironment(valid), valid);
assert.throws(() => auditClientFiles([{ path: "dist/app.js", text: "const R2_SECRET = 'x'" }]), /R2_SECRET/);
assert.throws(() => auditClientFiles([{ path: "src/app.ts", text: "deviceTokenFixture" }]), /deviceTokenFixture/);
assert.doesNotThrow(() => auditClientFiles([{ path: "dist/app.js", text: "const publicKey = 'public'" }]));

const smokeConfig = parseSmokeArgs(["--admin-origin", "https://admin.example.com/", "--update-origin", "https://updates.example.com/"]);
assert.throws(() => parseSmokeArgs(["--admin-origin", "https://same.example/", "--update-origin", "https://same.example/"]), /different/);
assert.throws(() => parseSmokeArgs(["--admin-origin", "https://admin.example/", "--token", "secret"]), /Usage/);
const mockRequests = [];
let unexpected2xx = false;
const mockServer = http.createServer((request, response) => {
  const chunks = [];
  request.on("data", (chunk) => chunks.push(chunk));
  request.on("end", () => mockRequests.push({ method: request.method, url: request.url, authorization: request.headers.authorization, body: Buffer.concat(chunks).toString("utf8") }));
  const status = unexpected2xx && request.url?.includes("/v1/update/") ? 200 : request.url === "/v1/activate" ? 400 : 401;
  response.writeHead(status, {
    "content-type": "text/plain",
    "x-secret": "server-header-secret",
  });
  response.end("server-body-secret");
});
await new Promise((resolve) => mockServer.listen(0, "127.0.0.1", resolve));
try {
  const address = mockServer.address();
  assert(address && typeof address === "object");
  const lines = [];
  const localConfig = { adminOrigin: `http://127.0.0.1:${address.port}`, updateOrigin: `http://127.0.0.1:${address.port}` };
  assert.equal(await runSmoke(localConfig, { log() {}, error() {} }), 0);
  assert.equal(mockRequests.length, 5);
  assert.equal(mockRequests.every((request) => request.authorization === undefined), true);
  assert.deepEqual(mockRequests.filter((request) => request.method === "POST").map((request) => request.body), ["{}", "{}"]);
  unexpected2xx = true;
  const exitCode = await runSmoke(
    localConfig,
    { log: (line) => lines.push(line), error: (line) => lines.push(line) },
  );
  assert.equal(exitCode, 1, "意外 2xx 必须使冒烟检查失败");
  assert.doesNotMatch(lines.join("\n"), /server-(?:body|header)-secret/);
} finally {
  await new Promise((resolve, reject) => mockServer.close((error) => error ? reject(error) : resolve()));
}

const capability = JSON.parse(await readFile(new URL("../src-tauri/capabilities/migrated.json", import.meta.url), "utf8"));
assert.equal(capability.permissions.includes("updater:default"), false, "前端 updater capability 必须移除");

const temp = await mkdtemp(path.join(os.tmpdir(), "diandian-build-config-"));
try {
  const executable = path.join(temp, process.platform === "win32" ? "build-guard.exe" : "build-guard");
  const compile = spawnSync("rustc", ["--edition", "2021", "src-tauri/build.rs", "-o", executable], {
    cwd: new URL("..", import.meta.url),
    encoding: "utf8",
  });
  assert.equal(compile.status, 0, compile.stderr);

  const runGuard = (profile, values = {}) => {
    const environment = { ...process.env, PROFILE: profile, ...values };
    delete environment.DIANDIAN_UPDATE_ENDPOINT;
    delete environment.DIANDIAN_LICENSE_PUBLIC_KEY;
    Object.assign(environment, values);
    return spawnSync(executable, [], { env: environment, encoding: "utf8" });
  };
  const bothMissing = runGuard("release");
  assert.notEqual(bothMissing.status, 0);
  assert.match(bothMissing.stderr, /DIANDIAN_UPDATE_ENDPOINT.*DIANDIAN_LICENSE_PUBLIC_KEY/s);
  const endpointMissing = runGuard("release", { DIANDIAN_LICENSE_PUBLIC_KEY: valid.DIANDIAN_LICENSE_PUBLIC_KEY });
  assert.notEqual(endpointMissing.status, 0);
  assert.match(endpointMissing.stderr, /DIANDIAN_UPDATE_ENDPOINT/);
  const keyMissing = runGuard("release", { DIANDIAN_UPDATE_ENDPOINT: valid.DIANDIAN_UPDATE_ENDPOINT });
  assert.notEqual(keyMissing.status, 0);
  assert.match(keyMissing.stderr, /DIANDIAN_LICENSE_PUBLIC_KEY/);
  const invalid = runGuard("release", { ...valid, DIANDIAN_UPDATE_ENDPOINT: "http://updates.example.com/" });
  assert.notEqual(invalid.status, 0);
  assert.match(invalid.stderr, /DIANDIAN_UPDATE_ENDPOINT/);
  assert.equal(runGuard("release", valid).status, 0);
  const debug = runGuard("debug");
  assert.equal(debug.status, 0);
  assert.match(debug.stdout, /DIANDIAN_UPDATE_ENDPOINT=http:\/\/127\.0\.0\.1:8787\//);
  assert.match(debug.stdout, /DIANDIAN_LEASE_PUBLIC_KEY=/);
} finally {
  await rm(temp, { recursive: true, force: true });
}

console.log("private distribution config tests passed");
