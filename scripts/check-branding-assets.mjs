import fs from "node:fs";
import path from "node:path";
import process from "node:process";

const root = process.cwd();
const expectedName = "典典直播切片";
const failures = [];

function read(relativePath) {
  return fs.readFileSync(path.join(root, relativePath), "utf8");
}

function readJson(relativePath) {
  return JSON.parse(read(relativePath));
}

function expectEqual(actual, expected, label) {
  if (actual !== expected) {
    failures.push(`${label}: expected ${JSON.stringify(expected)}, got ${JSON.stringify(actual)}`);
  }
}

function expectNoLegacyBrand(relativePath) {
  const text = read(relativePath);
  const legacy = text.match(/BiliBili ShadowReplay|BiliShadowReplay/g);
  if (legacy) {
    failures.push(`${relativePath}: contains legacy user-visible brand ${legacy[0]}`);
  }
}

const baseConfig = readJson("src-tauri/tauri.conf.json");
expectEqual(baseConfig.productName, expectedName, "base productName");

for (const relativePath of [
  "src-tauri/tauri.windows.conf.json",
  "src-tauri/tauri.windows.cuda.conf.json",
  "src-tauri/tauri.macos.conf.json",
  "src-tauri/tauri.linux.conf.json",
]) {
  const config = readJson(relativePath);
  if (config.productName !== undefined) {
    expectEqual(config.productName, expectedName, `${relativePath} productName`);
  }
  for (const window of config.app?.windows ?? []) {
    expectEqual(window.title, expectedName, `${relativePath} window title`);
  }
}

for (const relativePath of [
  "README.md",
  ".github/workflows/main.yml",
  "index.html",
  "src/page/About.svelte",
  "src/lib/agent/agent.ts",
  "src/lib/agent/prompts.ts",
  "src/lib/components/MarkerPanel.svelte",
  "src-tauri/src/handlers/video.rs",
  "src-tauri/src/recorder_manager.rs",
  "src-tauri/Cargo.toml",
  "docs/.vitepress/config.ts",
  "docs/index.md",
  "docs/development/architecture/overview.md",
  "docs/development/backend/database.md",
  "docs/development/frontend/agent.md",
  "docs/development/frontend/stores.md",
  "docs/development/platforms/implementation-guide.md",
  "docs/getting-started/installation/docker.md",
]) {
  expectNoLegacyBrand(relativePath);
}

const iconPng = fs.readFileSync(path.join(root, "src-tauri/icons/icon.png"));
const pngSignature = "89504e470d0a1a0a";
expectEqual(iconPng.subarray(0, 8).toString("hex"), pngSignature, "icon.png signature");
expectEqual(iconPng.readUInt32BE(16), 1024, "icon.png width");
expectEqual(iconPng.readUInt32BE(20), 1024, "icon.png height");

const iconIco = fs.readFileSync(path.join(root, "src-tauri/icons/icon.ico"));
expectEqual(iconIco.subarray(0, 4).toString("hex"), "00000100", "icon.ico signature");

const iconIcns = fs.readFileSync(path.join(root, "src-tauri/icons/icon.icns"));
expectEqual(iconIcns.subarray(0, 4).toString("ascii"), "icns", "icon.icns signature");

const buildScript = read("src-tauri/build.rs");
if (!buildScript.includes('cargo:rerun-if-changed=icons/icon.ico')) {
  failures.push("src-tauri/build.rs: does not rebuild the Windows executable when icon.ico changes");
}

if (failures.length > 0) {
  console.error(`Branding verification failed (${failures.length}):`);
  for (const failure of failures) console.error(`- ${failure}`);
  process.exit(1);
}

console.log("Branding verification passed.");
