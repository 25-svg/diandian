import { execFile, type ExecFileOptions } from "node:child_process";
import { fileURLToPath } from "node:url";
import { win32 } from "node:path";

export interface PublisherCredentials {
  bucket: string; endpoint: string; region: string; accessKeyId: string; secretAccessKey: string;
  serviceTokenId: string; serviceTokenSecret: string; apiBase: string;
}
export function credentialValues(value: PublisherCredentials): string[] {
  return [value.accessKeyId, value.secretAccessKey, value.serviceTokenId, value.serviceTokenSecret];
}
export function redact(value: unknown, known: string[] = []): unknown {
  const seen = new WeakSet<object>();
  const clean = (text: string) => known.filter(Boolean).sort((a, b) => b.length - a.length).reduce((result, secret) => result.split(secret).join("[REDACTED]"), text);
  const visit = (item: unknown): unknown => {
    if (typeof item === "string") return clean(item);
    if (typeof item === "bigint") return String(item);
    if (!item || typeof item !== "object") return item;
    if (seen.has(item)) return "[Circular]";
    seen.add(item);
    if (Array.isArray(item)) return item.map(visit);
    const entries = Object.entries(item instanceof Error ? { ...item, message: item.message } : item);
    return Object.fromEntries(entries.map(([key, child]) => [clean(key), /secret|token|credential|authorization|cf-access-client/i.test(key) ? "[REDACTED]" : visit(child)]));
  };
  return visit(value);
}
export function validateCredentials(value: unknown): PublisherCredentials {
  const keys = ["bucket", "endpoint", "region", "accessKeyId", "secretAccessKey", "serviceTokenId", "serviceTokenSecret", "apiBase"] as const;
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("CREDENTIALS_INVALID");
  const record = value as Record<string, unknown>;
  if (Object.keys(record).length !== keys.length || keys.some(key => typeof record[key] !== "string" || !(record[key] as string).length || (record[key] as string).length > 4096 || /[\x00-\x20\x7f-\x9f]/.test(record[key] as string))) throw new Error("CREDENTIALS_INVALID");
  const result = record as unknown as PublisherCredentials;
  try {
    if (!/^[a-z0-9][a-z0-9-]{1,61}[a-z0-9]$/.test(result.bucket) || !/^[a-z0-9-]+$/.test(result.region)) throw new Error();
    for (const raw of [result.endpoint, result.apiBase]) {
      const url = new URL(raw);
      if (url.protocol !== "https:" || url.username || url.password || url.search || url.hash || url.pathname !== "/" || url.port) throw new Error();
    }
    if (!/^[a-z0-9.-]+\.r2\.cloudflarestorage\.com$/.test(new URL(result.endpoint).hostname)) throw new Error();
  } catch { throw new Error("CREDENTIALS_INVALID"); }
  return result;
}
type Runner = (file: string, args: string[], options: ExecFileOptions) => Promise<{ stdout: string; stderr?: string }>;
const run: Runner = (file, args, options) => new Promise((resolve, reject) => {
  execFile(file, args, options, (error, stdout, stderr) => error ? reject(new Error("CREDENTIALS_UNAVAILABLE")) : resolve({ stdout: String(stdout), stderr: String(stderr) }));
});
export async function loadCredentials(options: { platform?: string; run?: Runner } = {}): Promise<PublisherCredentials> {
  if ((options.platform ?? process.platform) !== "win32") throw new Error("WINDOWS_REQUIRED");
  const script = fileURLToPath(new URL("../Set-PublisherCredential.ps1", import.meta.url));
  const powershell = win32.join(process.env.SystemRoot ?? "C:\\Windows", "System32", "WindowsPowerShell", "v1.0", "powershell.exe");
  let stdout: string;
  try {
    ({ stdout } = await (options.run ?? run)(powershell, ["-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", script, "-Read"], { encoding: "utf8", shell: false, windowsHide: true, maxBuffer: 65536, timeout: 30000 }));
  } catch { throw new Error("CREDENTIALS_UNAVAILABLE: run Set-PublisherCredential.ps1 as the current Windows user."); }
  try { return validateCredentials(JSON.parse(stdout.replace(/^\uFEFF/, ""))); }
  catch { throw new Error("CREDENTIALS_INVALID"); }
}
