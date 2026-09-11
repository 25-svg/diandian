import { spawnSync } from "node:child_process";

const npm = process.platform === "win32" ? "npm.cmd" : "npm";
const cargoEnvironment = { ...process.env, CARGO_BUILD_JOBS: "1" };

function run(command, args, environment = process.env, shell = false) {
  console.log(`> ${command} ${args.join(" ")}`);
  const result = spawnSync(command, args, { stdio: "inherit", env: environment, shell });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
}

run(process.execPath, ["scripts/test-private-distribution-config.mjs"]);
run(npm, ["--prefix", "distribution/control-plane", "test"], process.env, process.platform === "win32");
run(npm, ["--prefix", "distribution/publisher", "test"], process.env, process.platform === "win32");
run(npm, ["run", "test:license"], process.env, process.platform === "win32");
run(process.execPath, ["--loader", "ts-node/esm", "src/lib/appUpdater.test.ts"]);
run("cargo", ["test", "--manifest-path", "src-tauri/Cargo.toml", "--test", "private_distribution", "--no-default-features", "--features", "gui"], cargoEnvironment);
run("cargo", ["test", "--manifest-path", "src-tauri/Cargo.toml", "private_distribution::updater", "--no-default-features", "--features", "gui"], cargoEnvironment);
