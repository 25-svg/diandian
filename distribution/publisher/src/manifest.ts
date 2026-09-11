import { createHash } from "node:crypto";
import { createReadStream } from "node:fs";
import { lstat, open } from "node:fs/promises";

export interface ReleaseInput { version: string; bundle: string; signature: string; notes: string }
export interface ReleaseManifest {
  version: string; platform: "windows"; arch: "x86_64"; objectKey: string;
  size: number; sha256: string; signature: string; notes: string; pubDate: string;
}
export const MAX_BUNDLE_BYTES = 4 * 1024 ** 3;
// Keep aligned with control-plane/update-service parseVersion (including arbitrary-size core numbers).
const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-([0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;
function requireVersion(version: string) {
  const match = typeof version === "string" && version.length <= 256 && !/[\r\n]/.test(version) ? SEMVER.exec(version) : null;
  if (!match || match[4]?.split(".").some(id => /^0\d+$/.test(id))) throw new Error("SEMVER_INVALID");
}
async function fileSize(path: string, label: string, limit: number) {
  let stat; try { stat = await lstat(path); } catch { throw new Error(`${label}_UNREADABLE`); }
  if (!stat.isFile()) throw new Error(`${label}_NOT_FILE`);
  if (stat.size === 0) throw new Error(`${label}_EMPTY`);
  if (stat.size > limit) throw new Error(`${label}_TOO_LARGE`);
  return stat.size;
}
async function readText(path: string, label: string, limit: number) {
  await fileSize(path, label, limit);
  // A bounded read also handles a file growing between stat and read.
  let bytes: Buffer;
  try {
    const handle = await open(path, "r");
    try { const buffer = Buffer.alloc(limit + 1); const { bytesRead } = await handle.read(buffer); bytes = buffer.subarray(0, bytesRead); }
    finally { await handle.close(); }
  } catch { throw new Error(`${label}_UNREADABLE`); }
  if (bytes.length > limit) throw new Error(`${label}_TOO_LARGE`);
  if (!bytes.length) throw new Error(`${label}_EMPTY`);
  try { return new TextDecoder("utf-8", { fatal: true, ignoreBOM: true }).decode(bytes); }
  catch { throw new Error(`${label}_UTF8_INVALID`); }
}
function decodeBase64(value: string): Buffer | null {
  if (!value || !/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(value) || /[\r\n]/.test(value)) return null;
  return Buffer.from(value, "base64");
}
function validSignature(value: string): boolean {
  const outer = decodeBase64(value);
  if (!outer || outer.length < 180 || outer.length > 3072) return false;
  let box: string; try { box = new TextDecoder("utf-8", { fatal: true }).decode(outer); } catch { return false; }
  if (/[\x00-\x08\x0b-\x1f\x7f-\x9f]/.test(box)) return false;
  if (box.endsWith("\n")) box = box.slice(0, -1);
  const lines = box.split("\n");
  const comment = (line: string | undefined, prefix: string) => line?.startsWith(prefix) && line.length > prefix.length && line.length <= prefix.length + 512;
  if (lines.length !== 4 || !comment(lines[0], "untrusted comment: ") || !comment(lines[2], "trusted comment: ")) return false;
  const packet = decodeBase64(lines[1]!), global = decodeBase64(lines[3]!);
  return packet?.length === 74 && packet[0] === 0x45 && (packet[1] === 0x44 || packet[1] === 0x64) && global?.length === 64;
}
export async function validateRelease(input: ReleaseInput): Promise<ReleaseManifest> {
  requireVersion(input.version);
  const size = await fileSize(input.bundle, "BUNDLE", MAX_BUNDLE_BYTES);
  const signature = await readText(input.signature, "SIGNATURE", 4096);
  if (!validSignature(signature)) throw new Error("SIGNATURE_INVALID");
  const notes = await readText(input.notes, "NOTES", 16384);
  const hash = createHash("sha256"); let readSize = 0;
  try { for await (const chunk of createReadStream(input.bundle)) { readSize += chunk.length; if (readSize > MAX_BUNDLE_BYTES) throw new Error(); hash.update(chunk); } }
  catch { throw new Error("BUNDLE_READ_FAILED"); }
  if (readSize !== size) throw new Error("BUNDLE_CHANGED");
  const sha256 = hash.digest("hex");
  const result: ReleaseManifest = {
    version: input.version, platform: "windows", arch: "x86_64",
    objectKey: `releases/${input.version}/windows-x86_64/${sha256}.exe`,
    size, sha256, signature, notes, pubDate: new Date().toISOString(),
  };
  if (Buffer.byteLength(JSON.stringify(result), "utf8") > 8192) throw new Error("MANIFEST_TOO_LARGE");
  return result;
}
