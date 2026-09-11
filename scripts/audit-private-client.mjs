import { readFile } from "node:fs/promises";
import { auditCapabilities, auditClientDirectories } from "./private-distribution-config.mjs";

const secretEnvironmentNames = [
  "LEASE_PRIVATE_JWK",
  "LICENSE_PRIVATE_KEY",
  "R2_SECRET",
  "R2_SECRET_ACCESS_KEY",
  "PRIVATE_DISTRIBUTION_SECRET_CANARY",
  "VITE_PRIVATE_DISTRIBUTION_SECRET_CANARY",
];
const forbiddenValues = secretEnvironmentNames
  .map((name) => process.env[name])
  .filter((value) => typeof value === "string" && value.length > 0);

await auditClientDirectories(["src", "dist"], { forbiddenValues });
const capabilities = JSON.parse(await readFile("src-tauri/capabilities/migrated.json", "utf8"));
auditCapabilities(capabilities);
console.log("Private client source, Vite dist, and capability audit passed.");
