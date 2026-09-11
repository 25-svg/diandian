import { pathToFileURL } from "node:url";
import { resolve } from "node:path";
import { loadCredentials, credentialValues, redact, validateCredentials, type PublisherCredentials } from "./credentials.js";
import { validateRelease, type ReleaseInput, type ReleaseManifest } from "./manifest.js";
import { uploadBundle } from "./upload.js";

const HELP = "Usage: node dist/publish.js --version 2.21.1 --bundle <exe> --signature <exe.sig> --notes <txt>\nFirst run Set-PublisherCredential.ps1 to store Windows user DPAPI credentials.\nUploads a private artifact and creates a draft only. No testing/production promotion.\n";
export function parseArgs(args: string[]): ReleaseInput | { help: true } {
  if (args.length === 1 && args[0] === "--help") return { help: true };
  const result: Record<string, string> = {};
  if (args.length !== 8) throw new Error("ARGUMENTS_INVALID: use --help; credentials are never command-line arguments.");
  for (let i = 0; i < args.length; i += 2) {
    const flag = args[i]!, value = args[i + 1];
    if (!["--version", "--bundle", "--signature", "--notes"].includes(flag) || Object.hasOwn(result, flag.slice(2)) || !value || value.startsWith("--") || /[\0\r\n]/.test(value)) throw new Error("ARGUMENTS_INVALID");
    result[flag.slice(2)] = value;
  }
  return result as unknown as ReleaseInput;
}
interface PublishAdapter {
  upload?: typeof uploadBundle;
  fetch?: (url: string, init: RequestInit) => Promise<Response>;
}
export async function publishRelease(path: string, manifest: ReleaseManifest, credentials: PublisherCredentials, adapter: PublishAdapter = {}): Promise<{ id: string; status: "draft" }> {
  validateCredentials(credentials);
  // Explicit allowlist keeps runtime extra fields (especially status) out of the request.
  const { version, platform, arch, objectKey, size, sha256, signature, notes, pubDate } = manifest;
  const payload = JSON.stringify({ version, platform, arch, objectKey, size, sha256, signature, notes, pubDate });
  if (Buffer.byteLength(payload) > 8192) throw new Error("MANIFEST_TOO_LARGE");
  try { await (adapter.upload ?? uploadBundle)(path, manifest, credentials); }
  catch { throw new Error("UPLOAD_FAILED: no release was created; retry after checking the bundle and connection."); }
  let response: Response;
  try {
    response = await (adapter.fetch ?? fetch)(`${new URL(credentials.apiBase).origin}/api/publisher/releases`, {
      method: "POST", redirect: "error", signal: AbortSignal.timeout(30000),
      headers: { "Content-Type": "application/json", "CF-Access-Client-Id": credentials.serviceTokenId, "CF-Access-Client-Secret": credentials.serviceTokenSecret },
      body: payload,
    });
  } catch { throw new Error("RELEASE_NETWORK_FAILED: upload completed; check the draft list before retrying."); }
  if (!response.ok) {
    try { await response.body?.cancel(); } catch {}
    if (response.status === 409) throw new Error("VERSION_CONFLICT: version already exists; check the draft list.");
    if (response.status === 400) throw new Error("RELEASE_REJECTED: version may already exist or metadata is invalid; check the draft list.");
    throw new Error(`RELEASE_HTTP_${response.status}: upload completed but release registration failed.`);
  }
  try {
    // Bound untrusted responses and never echo their fields or messages.
    const reader = response.body?.getReader(); if (!reader) throw new Error();
    const chunks: Uint8Array[] = []; let bytes = 0;
    try { while (true) { const part = await reader.read(); if (part.done) break; bytes += part.value.length; if (bytes > 8192) { await reader.cancel(); throw new Error(); } chunks.push(part.value); } }
    finally { reader.releaseLock(); }
    const data = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(Buffer.concat(chunks)));
    if (data.status !== "draft" || typeof data.id !== "string" || !/^[A-Za-z0-9_-]{1,128}$/.test(data.id)) throw new Error();
    return { id: data.id, status: "draft" };
  } catch { throw new Error("RELEASE_RESPONSE_INVALID: check the draft list before retrying."); }
}
export async function main(args = process.argv.slice(2)): Promise<number> {
  let known: string[] = [];
  try {
    const input = parseArgs(args);
    if ("help" in input) { process.stdout.write(HELP); return 0; }
    const manifest = await validateRelease(input);
    const credentials = await loadCredentials(); known = credentialValues(credentials);
    const result = await publishRelease(input.bundle, manifest, credentials);
    process.stdout.write(JSON.stringify(redact({ message: "Draft created", version: manifest.version, ...result }, known)) + "\n");
    return 0;
  } catch (error) {
    process.stderr.write(JSON.stringify(redact({ error: error instanceof Error ? error.message : "PUBLISH_FAILED" }, known)) + "\n");
    return 1;
  }
}
if (process.argv[1] && pathToFileURL(resolve(process.argv[1])).href === import.meta.url) process.exitCode = await main();
