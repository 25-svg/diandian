import { spawnSync } from "node:child_process";
import { auditClientDirectories, validateReleaseEnvironment } from "./private-distribution-config.mjs";

try {
  validateReleaseEnvironment();
} catch (error) {
  console.error(error instanceof Error ? error.message : "Private distribution release configuration is invalid.");
  process.exit(1);
}

const npm = process.platform === "win32" ? "npm.cmd" : "npm";
function run(args) {
  console.log(`> npm ${args.join(" ")}`);
  const result = spawnSync(npm, args, { stdio: "inherit", shell: process.platform === "win32" });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

run(["--prefix", "distribution/control-plane", "run", "build"]);
run(["--prefix", "distribution/publisher", "run", "build"]);
run(["run", "build"]);
await auditClientDirectories(["src", "dist"]);
console.log("Private distribution build and client audit passed.");
