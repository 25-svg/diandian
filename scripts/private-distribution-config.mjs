import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { pathToFileURL } from "node:url";

const REQUIRED = ["DIANDIAN_UPDATE_ENDPOINT", "DIANDIAN_LICENSE_PUBLIC_KEY"];
const FORBIDDEN_CLIENT_NAMES = ["R2_SECRET", "LICENSE_PRIVATE_KEY", "deviceTokenFixture"];

function validateEndpoint(value) {
  let url;
  try {
    url = new URL(value);
  } catch {
    throw new Error("DIANDIAN_UPDATE_ENDPOINT must be an HTTPS root origin");
  }
  if (
    url.protocol !== "https:" ||
    !url.hostname ||
    url.username ||
    url.password ||
    url.pathname !== "/" ||
    url.search ||
    url.hash
  ) {
    throw new Error("DIANDIAN_UPDATE_ENDPOINT must be an HTTPS root origin without credentials, path, query, or fragment");
  }
}

function validatePublicKey(value) {
  if (!/^[A-Za-z0-9_-]+$/.test(value)) {
    throw new Error("DIANDIAN_LICENSE_PUBLIC_KEY must be unpadded base64url");
  }
  const bytes = Buffer.from(value, "base64url");
  if (bytes.length !== 65 || bytes[0] !== 0x04 || bytes.toString("base64url") !== value) {
    throw new Error("DIANDIAN_LICENSE_PUBLIC_KEY must encode one uncompressed 65-byte P-256 public key");
  }
}

export function validateReleaseEnvironment(environment = process.env) {
  const missing = REQUIRED.filter((name) => !environment[name]);
  if (missing.length) throw new Error(`Missing required release variables: ${missing.join(", ")}`);
  const values = {
    DIANDIAN_UPDATE_ENDPOINT: environment.DIANDIAN_UPDATE_ENDPOINT,
    DIANDIAN_LICENSE_PUBLIC_KEY: environment.DIANDIAN_LICENSE_PUBLIC_KEY,
  };
  validateEndpoint(values.DIANDIAN_UPDATE_ENDPOINT);
  validatePublicKey(values.DIANDIAN_LICENSE_PUBLIC_KEY);
  return values;
}

export function auditClientFiles(files) {
  for (const file of files) {
    for (const name of FORBIDDEN_CLIENT_NAMES) {
      if (file.text.includes(name)) throw new Error(`${file.path} contains forbidden client identifier ${name}`);
    }
  }
}

async function collectDirectory(directory, files) {
  for (const entry of await readdir(directory, { withFileTypes: true })) {
    const absolute = path.join(directory, entry.name);
    if (entry.isDirectory()) await collectDirectory(absolute, files);
    else if (!/\.test\.[cm]?[jt]sx?$/.test(entry.name) && /\.(?:html|css|js|mjs|cjs|ts|svelte)$/.test(entry.name)) {
      files.push({ path: absolute, text: await readFile(absolute, "utf8") });
    }
  }
}

export async function auditClientDirectories(directories) {
  const files = [];
  for (const directory of directories) await collectDirectory(directory, files);
  auditClientFiles(files);
}

if (process.argv[1] && pathToFileURL(path.resolve(process.argv[1])).href === import.meta.url) {
  try {
    validateReleaseEnvironment();
    console.log("Private distribution release configuration is valid.");
  } catch (error) {
    console.error(error instanceof Error ? error.message : "Private distribution release configuration is invalid.");
    process.exitCode = 1;
  }
}
