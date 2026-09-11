import assert from "node:assert/strict";
import { mkdtemp, readFile, rm, writeFile } from "node:fs/promises";
import { spawnSync } from "node:child_process";
import os from "node:os";
import path from "node:path";
import http from "node:http";
import { fileURLToPath } from "node:url";
import {
  validateReleaseEnvironment,
  auditCapabilities,
  auditClientFiles,
} from "./private-distribution-config.mjs";
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

const validEndpoints = [
  "https://updates.example.com",
  "https://updates.example.com/",
  "https://updates.example.com:8443/",
  "https://[2001:db8::1]/",
  "https://[2001:db8::1]:8443/",
];
const invalidEndpoints = [
  "http://updates.example.com/",
  " https://updates.example.com/",
  "https://updates.example.com/ ",
  "https://:/",
  "https://updates.example.com:bad/",
  "https://updates.example.com:70000/",
  "https://2001:db8::1/",
  "https://user@updates.example.com/",
  "https://:password@updates.example.com/",
  "https://updates.example.com/v1",
  "https://updates.example.com/.",
  "https://updates.example.com/%2e",
  "https://updates.example.com/?channel=prod",
  "https://updates.example.com/#prod",
  "https://updates.example.com\\escape",
];
for (const endpoint of validEndpoints) {
  assert.doesNotThrow(() => validateReleaseEnvironment({ ...valid, DIANDIAN_UPDATE_ENDPOINT: endpoint }));
}
for (const endpoint of invalidEndpoints) {
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
for (const name of ["LEASE_PRIVATE_JWK", "R2_SECRET_ACCESS_KEY", "secretAccessKey", "serviceTokenSecret"]) {
  assert.throws(() => auditClientFiles([{ path: "dist/app.js", text: `const ${name} = 'x'` }]), new RegExp(name));
}
assert.throws(() => auditClientFiles([{ path: "src/app.ts", text: "deviceTokenFixture" }]), /deviceTokenFixture/);
assert.throws(
  () => auditClientFiles([{ path: "dist/app.js", text: "window.value='diandian-build-secret-canary-123'" }], {
    forbiddenValues: ["diandian-build-secret-canary-123"],
  }),
  /secret value/i,
);
assert.doesNotThrow(() => auditClientFiles([{ path: "dist/app.js", text: "const publicKey = 'public'" }]));
assert.doesNotThrow(() => auditClientFiles([{ path: "dist/app.js", text: "const serviceToken = schema.string(); const publicJwk = {};" }]));

const aboutSource = await readFile(new URL("../src/page/About.svelte", import.meta.url), "utf8");
assert.doesNotThrow(
  () => auditClientFiles([{ path: "src/page/About.svelte", text: aboutSource }]),
  "About 页面不得包含公网 GitHub Releases 入口",
);

assert.throws(() => auditCapabilities({ permissions: ["updater:default"] }), /updater:default/);
assert.throws(() => auditCapabilities({ permissions: [{ identifier: "updater:allow-check" }] }), /updater:allow-check/);
assert.throws(
  () => auditCapabilities({ nested: { permissions: [[{ identifier: "UPDATER:allow-download" }]] } }),
  /UPDATER:allow-download/,
);
assert.doesNotThrow(() => auditCapabilities({ permissions: ["core:default", { identifier: "fs:scope", allow: ["**"] }] }));

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
assert.doesNotThrow(() => auditCapabilities(capability), "前端 updater capability 必须全部移除");

const aggregateSource = await readFile(new URL("./test-private-distribution.mjs", import.meta.url), "utf8");
assert.match(aggregateSource, /scripts\/test-license-ui\.mjs/, "总测试必须执行真实 Chromium 激活门");
assert.match(aggregateSource, /handlers::license/, "总测试必须执行 Rust license handler 目标");
assert.match(aggregateSource, /PRIVATE_DISTRIBUTION_SECRET_CANARY/, "总测试必须用构建 canary 验证秘密不会进入 dist");
assert.match(aggregateSource, /scripts\/audit-private-client\.mjs/, "总测试必须审计真实 Vite dist");

const temp = await mkdtemp(path.join(os.tmpdir(), "diandian-build-config-"));
try {
  const projectRoot = fileURLToPath(new URL("..", import.meta.url));
  const buildConfigPath = path.join(projectRoot, "src-tauri", "build_config.rs").replaceAll("\\", "/");
  await writeFile(path.join(temp, "Cargo.toml"), `[package]\nname = "diandian-build-guard-test"\nversion = "0.0.0"\nedition = "2021"\nbuild = "build.rs"\n\n[build-dependencies]\nurl = "2.5.4"\n\n[lib]\npath = "lib.rs"\n`);
  await writeFile(path.join(temp, "lib.rs"), "");
  await writeFile(path.join(temp, "build.rs"), `#[path = r"${buildConfigPath}"]\nmod build_config;\nfn main() { build_config::configure(); }\n`);

  const runGuard = (profile, values = {}) => {
    const environment = { ...process.env, ...values, CARGO_TARGET_DIR: path.join(temp, "target") };
    delete environment.DIANDIAN_UPDATE_ENDPOINT;
    delete environment.DIANDIAN_LICENSE_PUBLIC_KEY;
    Object.assign(environment, values);
    const args = ["check", "--manifest-path", path.join(temp, "Cargo.toml")];
    if (profile === "release") args.push("--release");
    return spawnSync("cargo", args, { env: environment, encoding: "utf8" });
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
  for (const endpoint of invalidEndpoints) {
    const invalid = runGuard("release", { ...valid, DIANDIAN_UPDATE_ENDPOINT: endpoint });
    assert.notEqual(invalid.status, 0, `Rust build guard accepted ${endpoint}`);
    assert.match(invalid.stderr, /DIANDIAN_UPDATE_ENDPOINT/);
  }
  for (const endpoint of validEndpoints) {
    const accepted = runGuard("release", { ...valid, DIANDIAN_UPDATE_ENDPOINT: endpoint });
    assert.equal(accepted.status, 0, `Rust build guard rejected ${endpoint}: ${accepted.stderr}`);
  }
  const debug = runGuard("debug");
  assert.equal(debug.status, 0);
} finally {
  await rm(temp, { recursive: true, force: true });
}

console.log("private distribution config tests passed");
