import { mkdtemp, writeFile, rm, mkdir } from "node:fs/promises";
import { join } from "node:path";
import { afterEach, expect, it } from "vitest";
import { validateRelease } from "../src/manifest.js";

const dirs: string[] = [];
afterEach(async () => { await Promise.all(dirs.splice(0).map(path => rm(path, { recursive: true, force: true }))); });
export function signatureBox(options: { algorithm?: number; trusted?: string; suffix?: string; crlf?: boolean; packetSize?: number } = {}) {
  const packet = Buffer.alloc(options.packetSize ?? 74); packet[0] = 0x45; packet[1] = options.algorithm ?? 0x44;
  let box = `untrusted comment: signature from minisign secret key\n${packet.toString("base64")}\ntrusted comment: ${options.trusted ?? "timestamp: 1700000000\t典典\u2028直播\u2029.exe"}\n${Buffer.alloc(64).toString("base64")}${options.suffix ?? "\n"}`;
  if (options.crlf) box = box.replaceAll("\n", "\r\n");
  return Buffer.from(box).toString("base64");
}
async function files(signature = signatureBox()) {
  await mkdir(new URL("../.test-tmp", import.meta.url), { recursive: true });
  const dir = await mkdtemp(new URL("../.test-tmp/manifest-", import.meta.url).pathname.replace(/^\/([A-Z]:)/, "$1")); dirs.push(dir);
  const bundle = join(dir, "installer & name.exe"), sig = join(dir, "installer.sig"), notes = join(dir, "notes.txt");
  await Promise.all([writeFile(bundle, "abc"), writeFile(sig, signature), writeFile(notes, "Release notes")]);
  return { version: "2.21.1", bundle, signature: sig, notes };
}
it.each(["v2.21", "01.2.3", "1.2.3-01", "1.2.3/../../", "1.2.3\n", "1.2.3+", "1.2.3-", "1.2.3-rc..1"])("rejects invalid SemVer %j before upload", async version => {
  await expect(validateRelease({ ...await files(), version })).rejects.toThrow("SEMVER_INVALID");
});
it.each(["2.21.1", "2.21.1-rc.1+build.7", "9007199254740993.0.0"])("hashes real bytes and builds safe object key for %s", async version => {
  const result = await validateRelease({ ...await files(), version });
  expect(result).toMatchObject({ version, size: 3, sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad", notes: "Release notes", platform: "windows", arch: "x86_64" });
  expect(result.objectKey).toMatch(/^releases\/[0-9A-Za-z.+-]+\/windows-x86_64\/[a-f0-9]{64}-[a-f0-9]{8}-[a-f0-9]{4}-4[a-f0-9]{3}-[89ab][a-f0-9]{3}-[a-f0-9]{12}\.exe$/);
  expect(result).not.toHaveProperty("status");
});
it("assigns a fresh object key to identical release inputs on every attempt", async () => {
  const input = await files();
  const first = await validateRelease(input), second = await validateRelease(input);
  expect(second.sha256).toBe(first.sha256);
  expect(second.objectKey).not.toBe(first.objectKey);
  expect(second.objectKey).toContain(`/windows-x86_64/${second.sha256}-`);
});
it("rejects an empty signature", async () => { await expect(validateRelease(await files(""))).rejects.toThrow("SIGNATURE_EMPTY"); });
it.each([{}, { suffix: "" }, { algorithm: 0x64 }, { trusted: "中文\t文件.exe" }])("accepts official SignatureBox variant %j", async options => {
  await expect(validateRelease(await files(signatureBox(options)))).resolves.toHaveProperty("signature", signatureBox(options));
});
it.each([{ suffix: "\n\n" }, { crlf: true }, { algorithm: 0x99 }, { trusted: "bad\0name" }, { trusted: "bad\x85name" }, { trusted: "" }, { trusted: "a".repeat(513) }, { packetSize: 73 }])("rejects malformed SignatureBox %j", async options => {
  await expect(validateRelease(await files(signatureBox(options)))).rejects.toThrow("SIGNATURE_INVALID");
});
it.each(["not base64", Buffer.alloc(64).toString("base64"), "AAAA===="])("rejects non-SignatureBox %j", async sig => { await expect(validateRelease(await files(sig))).rejects.toThrow("SIGNATURE_INVALID"); });
it("rejects empty and non-file bundles", async () => {
  const input = await files(); await writeFile(input.bundle, "");
  await expect(validateRelease(input)).rejects.toThrow("BUNDLE_EMPTY");
  await expect(validateRelease({ ...input, bundle: join(input.bundle, "..") })).rejects.toThrow("BUNDLE_NOT_FILE");
});
it("rejects invalid UTF-8 and oversized notes before upload", async () => {
  const input = await files(); await writeFile(input.notes, Buffer.from([0xff]));
  await expect(validateRelease(input)).rejects.toThrow("NOTES_UTF8_INVALID");
  await writeFile(input.notes, "x".repeat(16385)); await expect(validateRelease(input)).rejects.toThrow("NOTES_TOO_LARGE");
  await writeFile(input.notes, "中".repeat(2700)); await expect(validateRelease(input)).rejects.toThrow("MANIFEST_TOO_LARGE");
});
