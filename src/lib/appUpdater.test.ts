import assert from "node:assert/strict";
import { readPrivateUpdateStatus, type InvokeUpdateStatus } from "./appUpdater.js";

const calls: Array<{ command: string; args?: Record<string, unknown> }> = [];
const invoke: InvokeUpdateStatus = async (command, args) => {
  calls.push({ command, args });
  return { status: "waiting_for_idle", version: "2.21.1", message: "等待软件空闲后自动更新。" };
};

assert.deepEqual(await readPrivateUpdateStatus(invoke), {
  status: "waiting_for_idle",
  version: "2.21.1",
  message: "等待软件空闲后自动更新。",
});
assert.deepEqual(calls, [{ command: "get_private_update_status", args: undefined }]);

const source = await import("node:fs/promises").then(({ readFile }) => readFile(new URL("./appUpdater.ts", import.meta.url), "utf8"));
assert.doesNotMatch(source, /@tauri-apps\/plugin-updater|downloadAndInstall|\bcheck\s*\(/, "前端不得触发公共更新器");
assert.doesNotMatch(source, /deviceToken|Authorization|Bearer/, "前端不得接触设备凭据");
const appSource = await import("node:fs/promises").then(({ readFile }) => readFile(new URL("../App.svelte", import.meta.url), "utf8"));
assert.doesNotMatch(appSource, /@tauri-apps\/plugin-updater|runStartupUpdate|立即更新|稍后/, "App 不得保留公共更新或人工确认入口");
const config = JSON.parse(await import("node:fs/promises").then(({ readFile }) => readFile(new URL("../../src-tauri/tauri.conf.json", import.meta.url), "utf8")));
assert.deepEqual(config.plugins.updater.endpoints, [], "静态更新入口必须 fail-closed");

console.log("app updater tests passed");
