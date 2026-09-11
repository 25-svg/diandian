import { createHash } from "node:crypto";
import { mkdir, mkdtemp, writeFile, rm, readFile } from "node:fs/promises";
import { join } from "node:path";
import { fileURLToPath } from "node:url";
import { spawnSync } from "node:child_process";
import { afterEach, expect, it, vi } from "vitest";
import { S3Client } from "@aws-sdk/client-s3";
import { uploadBundle } from "../src/upload.js";
import { parseArgs, publishRelease } from "../src/publish.js";
import { loadCredentials, redact } from "../src/credentials.js";

const fake = { bucket: "private-test-bucket", endpoint: "https://test.r2.cloudflarestorage.com", region: "auto", accessKeyId: "TEST_ONLY_ACCESS_ID", secretAccessKey: "TEST_ONLY_S3_SECRET", serviceTokenId: "TEST_ONLY_TOKEN_ID", serviceTokenSecret: "TEST_ONLY_TOKEN_SECRET", apiBase: "https://publisher.example.test" };
const secrets = [fake.accessKeyId, fake.secretAccessKey, fake.serviceTokenId, fake.serviceTokenSecret];
const dirs: string[] = [];
afterEach(async () => { await Promise.all(dirs.splice(0).map(path => rm(path, { recursive: true, force: true }))); });
async function bundle(bytes = Buffer.from("abc")) {
  const base = fileURLToPath(new URL("../.test-tmp/", import.meta.url)); await mkdir(base, { recursive: true });
  const dir = await mkdtemp(join(base, "upload-")); dirs.push(dir); const path = join(dir, "app.exe"); await writeFile(path, bytes);
  return { path, manifest: { version: "2.21.1", platform: "windows" as const, arch: "x86_64" as const, objectKey: "releases/2.21.1/windows-x86_64/test.exe", size: bytes.length, sha256: createHash("sha256").update(bytes).digest("hex"), signature: "test", notes: "notes", pubDate: "2026-09-11T00:00:00Z" } };
}
it("sends 16 MiB multipart upload with cleanup and SHA metadata", async () => {
  const { path, manifest } = await bundle(); let captured: any;
  await uploadBundle(path, manifest, fake, { createUpload: (options: any) => { captured = options; return { done: async () => { for await (const _chunk of options.params.Body) {} } }; } });
  expect(captured).toMatchObject({ partSize: 16777216, leavePartsOnError: false, params: { Bucket: fake.bucket, Key: manifest.objectKey, Metadata: { sha256: manifest.sha256 } } });
});
it("rejects a bundle changed after validation and hides upload exception details", async () => {
  const { path, manifest } = await bundle(); await writeFile(path, "xyz");
  await expect(uploadBundle(path, manifest, fake, { createUpload: (options: any) => ({ done: async () => { for await (const _chunk of options.params.Body) {} } }) })).rejects.toThrow("UPLOAD_FAILED");
  const error = await uploadBundle(path, manifest, fake, { createUpload: () => ({ done: async () => { throw new Error(fake.secretAccessKey); } }) }).catch(error => error);
  expect(error.message).toContain("UPLOAD_FAILED"); expect(error.message).not.toContain(fake.secretAccessKey);
});
it("uploads before creating a draft, omits status and sends Access headers", async () => {
  const { path, manifest } = await bundle(); const events: string[] = [];
  const result = await publishRelease(path, manifest, fake, {
    upload: async () => { events.push("upload"); },
    fetch: async (url: string, init: RequestInit) => { events.push("create"); expect(url).toBe("https://publisher.example.test/api/publisher/releases"); expect(JSON.parse(init.body as string)).not.toHaveProperty("status"); expect(init.headers).toMatchObject({ "CF-Access-Client-Id": fake.serviceTokenId, "CF-Access-Client-Secret": fake.serviceTokenSecret }); expect(init.redirect).toBe("error"); return new Response(JSON.stringify({ id: "release-1", status: "draft" }), { status: 201 }); },
  });
  expect(events).toEqual(["upload", "create"]); expect(result).toEqual({ id: "release-1", status: "draft" });
});
it.each([400, 401, 409, 500])("safely rejects HTTP %s without echoing its body", async status => {
  const { path, manifest } = await bundle();
  await expect(publishRelease(path, manifest, fake, { upload: async () => {}, fetch: async () => new Response(fake.serviceTokenSecret, { status }) })).rejects.toThrow(status === 409 ? "VERSION_CONFLICT" : status === 400 ? "RELEASE_REJECTED" : `RELEASE_HTTP_${status}`);
});
it("rejects network errors and an unexpected non-draft success", async () => {
  const { path, manifest } = await bundle();
  await expect(publishRelease(path, manifest, fake, { upload: async () => {}, fetch: async () => { throw new Error(fake.secretAccessKey); } })).rejects.toThrow("RELEASE_NETWORK_FAILED");
  await expect(publishRelease(path, manifest, fake, { upload: async () => {}, fetch: async () => new Response(JSON.stringify({ id: "r", status: "production" })) })).rejects.toThrow("RELEASE_RESPONSE_INVALID");
});
it("uses real SDK retry for part 3, aborts final failure, and never creates release", async () => {
  const { path, manifest } = await bundle(Buffer.alloc(33 * 1024 * 1024));
  let part3Attempts = 0, aborted = 0, apiCalls = 0;
  const client = new S3Client({ endpoint: fake.endpoint, region: "auto", credentials: { accessKeyId: fake.accessKeyId, secretAccessKey: fake.secretAccessKey }, maxAttempts: 3, retryMode: "standard", requestHandler: { handle: async (request: any) => {
    if (request.method === "DELETE") { aborted++; return { response: { statusCode: 204, headers: {}, body: Buffer.from("") } }; }
    if (request.query?.uploads !== undefined) return { response: { statusCode: 200, headers: {}, body: Buffer.from("<InitiateMultipartUploadResult><UploadId>test-upload</UploadId></InitiateMultipartUploadResult>") } };
    if (String(request.query?.partNumber) === "3") { part3Attempts++; return { response: { statusCode: 503, headers: {}, body: Buffer.from("<Error><Code>SlowDown</Code><Message>TEST_ONLY_S3_SECRET</Message></Error>") } }; }
    return { response: { statusCode: 200, headers: { etag: '"part-etag"' }, body: Buffer.from("") } };
  } } });
  await expect(publishRelease(path, manifest, fake, { upload: (p: any, m: any, c: any) => uploadBundle(p, m, c, { client }), fetch: async () => { apiCalls++; return new Response(); } })).rejects.toThrow("UPLOAD_FAILED");
  expect(part3Attempts).toBe(3); expect(aborted).toBe(1); expect(apiCalls).toBe(0); client.destroy();
}, 20000);
it("redacts recursive fields, error properties, cycles and known values", () => {
  const object: any = { Secret: "x", nested: [{ Authorization: "y", "CF-Access-Client-Id": "z", message: `failed ${fake.secretAccessKey}` }], error: Object.assign(new Error(fake.serviceTokenSecret), { credential: "x" }) }; object.self = object;
  const result = redact(object, secrets); const text = JSON.stringify(result);
  expect(result.Secret).toBe("[REDACTED]"); expect(result.nested[0].Authorization).toBe("[REDACTED]");
  for (const secret of secrets) expect(text).not.toContain(secret);
});
it("loads DPAPI through argument array without secrets, captures output", async () => {
  let invocation: any;
  expect(await loadCredentials({ platform: "win32", run: async (...args: any[]) => { invocation = args; return { stdout: JSON.stringify(fake), stderr: "" }; } })).toEqual(fake);
  expect(invocation[1]).toContain("-Read"); expect(invocation[2]).toMatchObject({ shell: false, windowsHide: true });
  for (const secret of secrets) expect(JSON.stringify(invocation)).not.toContain(secret);
});
it("fails closed on non-Windows, invalid credential JSON, and subprocess failure", async () => {
  const run = vi.fn(); await expect(loadCredentials({ platform: "linux", run })).rejects.toThrow("WINDOWS_REQUIRED"); expect(run).not.toHaveBeenCalled();
  await expect(loadCredentials({ platform: "win32", run: async () => ({ stdout: "{}" }) })).rejects.toThrow("CREDENTIALS_INVALID");
  await expect(loadCredentials({ platform: "win32", run: async () => { throw new Error(fake.serviceTokenSecret); } })).rejects.toThrow("CREDENTIALS_UNAVAILABLE");
});
it.each(["http://publisher.example.test", "https://user:pass@publisher.example.test", "https://publisher.example.test/path", "https://publisher.example.test/?token=bad"])("rejects unsafe API credential destination %s", async apiBase => {
  await expect(loadCredentials({ platform: "win32", run: async () => ({ stdout: JSON.stringify({ ...fake, apiBase }) }) })).rejects.toThrow("CREDENTIALS_INVALID");
});
it.each([[], ["--version", "2.21.1"], ["--secret", "hidden"], ["--help", "--secret", "hidden"], ["--version", "1", "--version", "2"], ["--bundle"], ["--bundle=app.exe"], ["--notes", "--bundle", "app.exe"], ["--version", "1.2.3", "--version", "2.0.0", "--notes", "n", "--bundle", "b"]].map(args => [args]))("rejects invalid CLI arguments %j", args => { expect(() => parseArgs(args)).toThrow("ARGUMENTS_INVALID"); });
it("parses four exact flags and help without interpolation", () => {
  expect(parseArgs(["--version", "2.21.1", "--bundle", "C:\\a & b.exe", "--signature", "a.sig", "--notes", "notes.txt"])).toEqual({ version: "2.21.1", bundle: "C:\\a & b.exe", signature: "a.sig", notes: "notes.txt" });
  expect(parseArgs(["--help"])).toEqual({ help: true });
});
it("ships an ASCII CMD and PowerShell 5 parses and validates without writes", async () => {
  const cmd = await readFile(new URL("../发布点点更新.cmd", import.meta.url)); expect([...cmd].every(byte => byte < 128)).toBe(true);
  const ps = fileURLToPath(new URL("../Set-PublisherCredential.ps1", import.meta.url));
  if (process.platform !== "win32") return;
  const result = spawnSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-File", ps, "-ValidateOnly"], { encoding: "utf8", timeout: 15000 });
  expect(result.status, result.stderr).toBe(0);
  const bad = spawnSync("powershell.exe", ["-NoProfile", "-NonInteractive", "-File", ps, "-Secret", "TEST_ONLY_SECRET_ARGUMENT"], { encoding: "utf8", timeout: 15000 });
  expect(bad.status).not.toBe(0); expect(bad.stdout + bad.stderr).not.toContain("TEST_ONLY_SECRET_ARGUMENT");
}, 40000);
