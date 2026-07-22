import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

const component = readFileSync("src/lib/components/settings/KnowledgeVaultSettings.svelte", "utf8");
const setting = readFileSync("src/page/Setting.svelte", "utf8");

for (const command of [
  "get_knowledge_status", "inspect_knowledge_vault", "connect_knowledge_vault",
  "sync_knowledge_vault", "open_knowledge_vault",
]) assert.match(component, new RegExp(`invoke(?:<[^>]+>)?\\(\\s*["']${command}["']`));

for (const label of ["Obsidian 知识库", "选择知识库", "重新同步", "打开文件夹"])
  assert.match(component, new RegExp(label));

assert.match(setting, /<KnowledgeVaultSettings\s*\/>/);
console.log("knowledge settings contract passed");
