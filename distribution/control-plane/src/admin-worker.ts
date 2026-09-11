import { randomToken, sha256Hex } from "./crypto";
import { createAccessIdentityAdapter, type AccessIdentityAdapter, type VerifiedAccessIdentity } from "./admin-auth";

export type { VerifiedAccessIdentity } from "./admin-auth";

type Role = "owner" | "operator";
type Admin = { id: string; email: string; role: Role };
type AdminEnv = { DB: D1Database; ARTIFACTS: R2Bucket; UPDATE_API_BASE_URL?: string; ADMIN_ACCESS_ISSUER?: string; ADMIN_ACCESS_AUDIENCE?: string; ADMIN_ACCESS_JWKS_URL?: string; PUBLISHER_ACCESS_ISSUER?: string; PUBLISHER_ACCESS_AUDIENCE?: string; PUBLISHER_ACCESS_JWKS_URL?: string };
type WorkerOptions = { adminAccess: AccessIdentityAdapter; publisherAccess: AccessIdentityAdapter; clock?: () => number };

const BODY_LIMIT = 8 * 1024;
const PAGE_LIMIT = 100;
const TOKEN_BYTES = 32;

function response(value: unknown, status = 200): Response {
  return new Response(JSON.stringify(value), { status, headers: { "content-type": "application/json; charset=utf-8", "cache-control": "no-store" } });
}
function failure(code: string, message: string, status: number): Response { return response({ error: { code, message } }, status); }
function id(): string { return crypto.randomUUID(); }
function safeId(value: string): boolean { return /^[A-Za-z0-9_-]{1,256}$/.test(value); }
function nowFrom(clock: () => number): number { const now = clock(); if (!Number.isSafeInteger(now)) throw new Error("Invalid clock"); return now; }

async function body(request: Request): Promise<Record<string, unknown>> {
  const contentLength = request.headers.get("content-length");
  if (contentLength && (!/^\d+$/.test(contentLength) || Number(contentLength) > BODY_LIMIT)) throw new HttpError("PAYLOAD_TOO_LARGE", "Request body is too large.", 413);
  if (!request.body) throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400);
  const reader = request.body.getReader(); const chunks: Uint8Array[] = []; let length = 0;
  try { while (true) { const { done, value } = await reader.read(); if (done) break; length += value.byteLength; if (length > BODY_LIMIT) { try { await reader.cancel(); } catch { /* preserve cap response */ } throw new HttpError("PAYLOAD_TOO_LARGE", "Request body is too large.", 413); } chunks.push(value); } }
  finally { reader.releaseLock(); }
  const bytes = new Uint8Array(length); let offset = 0; for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
  try { const parsed: unknown = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes)); if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) throw new Error(); return parsed as Record<string, unknown>; }
  catch { throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400); }
}
function stringField(input: Record<string, unknown>, name: string, max: number, pattern?: RegExp): string {
  const value = input[name];
  if (typeof value !== "string" || value.length === 0 || value.length > max || (pattern && !pattern.test(value))) throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400);
  return value;
}
function optionalString(input: Record<string, unknown>, name: string, max: number): string {
  const value = input[name]; if (value === undefined) return ""; return stringField(input, name, max);
}
class HttpError extends Error { constructor(readonly code: string, message: string, readonly status: number) { super(message); } }

async function findAdmin(db: D1Database, identity: VerifiedAccessIdentity): Promise<Admin | null> {
  return db.prepare("SELECT id, email, role FROM admins WHERE email = ?").bind(identity.email).first<Admin>();
}
async function audit(db: D1Database, adminId: string | null, action: string, targetType: string, targetId: string | null, result: "started" | "success" | "failure", now: number, auditId = id()): Promise<string> {
  const details = JSON.stringify({ result });
  if (result === "started") await db.prepare("INSERT INTO audit_logs (id, admin_id, action, target_type, target_id, details_json, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)").bind(auditId, adminId, action, targetType, targetId, details, now).run();
  else await db.prepare("UPDATE audit_logs SET details_json = ? WHERE id = ?").bind(details, auditId).run();
  return auditId;
}

async function withAudit<T>(db: D1Database, identity: Admin, action: string, targetType: string, targetId: string | null, now: number, run: () => Promise<T>): Promise<T> {
  const auditId = await audit(db, identity.id, action, targetType, targetId, "started", now);
  try { const output = await run(); await audit(db, identity.id, action, targetType, targetId, "success", now, auditId); return output; }
  catch (cause) { try { await audit(db, identity.id, action, targetType, targetId, "failure", now, auditId); } catch { /* started row remains forensic evidence. */ } throw cause; }
}

function releaseInput(input: Record<string, unknown>) {
  const allowed = new Set(["version", "platform", "arch", "objectKey", "size", "sha256", "signature", "notes", "pubDate"]);
  if (Object.keys(input).some((key) => !allowed.has(key))) throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400);
  const version = stringField(input, "version", 128, /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:[-+][0-9A-Za-z.-]+)?$/);
  const platform = stringField(input, "platform", 32, /^[a-z0-9_-]+$/);
  const arch = stringField(input, "arch", 32, /^[a-z0-9_-]+$/);
  const objectKey = stringField(input, "objectKey", 512, /^(?!\/)(?!.*\.\.)(?!.*[\r\n\0]).+$/);
  const size = input.size; if (!Number.isSafeInteger(size) || (size as number) < 0 || (size as number) > 2 ** 53 - 1) throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400);
  const sha256 = stringField(input, "sha256", 64, /^[a-fA-F0-9]{64}$/).toLowerCase();
  const signature = stringField(input, "signature", 16_384);
  const notes = stringField(input, "notes", 16_384);
  const pubDate = stringField(input, "pubDate", 64); if (Number.isNaN(Date.parse(pubDate))) throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400);
  return { version, platform, arch, objectKey, size: size as number, sha256, signature, notes, pubDate };
}

function routes(options: WorkerOptions) {
  const clock = options.clock ?? (() => Math.floor(Date.now() / 1000));
  return async (request: Request, env: AdminEnv): Promise<Response> => {
    const path = new URL(request.url).pathname;
    const now = nowFrom(clock);
    if (path === "/api/publisher/releases") return publisher(request, env, options.publisherAccess, now);
    if (!path.startsWith("/api/admin/")) return failure("NOT_FOUND", "Route not found.", 404);
    const verified = await options.adminAccess.verify(request);
    if (!verified) return failure("UNAUTHORIZED", "Authentication is required.", 401);
    const admin = await findAdmin(env.DB, verified);
    if (!admin) return failure("FORBIDDEN", "Administrator access is required.", 403);
    return adminRoute(request, env, admin, path.slice("/api/admin".length), now);
  };
}

async function adminRoute(request: Request, env: AdminEnv, admin: Admin, path: string, now: number): Promise<Response> {
  const ownerOnly = async (action: string, target: string) => {
    if (admin.role === "owner") return true;
    const auditId = await audit(env.DB, admin.id, action, target, null, "started", now);
    await audit(env.DB, admin.id, action, target, null, "failure", now, auditId);
    return false;
  };
  if (path === "/overview" && request.method === "GET") {
    const [devices, releases, codes] = await Promise.all([env.DB.prepare("SELECT count(*) AS count FROM devices WHERE status = 'active'").first(), env.DB.prepare("SELECT count(*) AS count FROM releases WHERE status IN ('testing', 'production')").first(), env.DB.prepare("SELECT count(*) AS count FROM activation_codes WHERE status = 'unused' AND expires_at > ?").bind(now).first()]);
    return response({ devices: (devices as { count: number }).count, releases: (releases as { count: number }).count, activationCodes: (codes as { count: number }).count });
  }
  if (path === "/devices" && request.method === "GET") return response((await env.DB.prepare("SELECT id, status, streamer_note, current_version, test_group, activated_at, revoked_at, last_seen_at FROM devices ORDER BY created_at DESC LIMIT ?").bind(PAGE_LIMIT).all()).results);
  let match = /^\/devices\/([A-Za-z0-9_-]{1,256})\/(revoke|unbind|test-group)$/.exec(path);
  if (match && request.method === "POST") {
    const deviceId = match[1]!; const action = match[2]!;
    return withAudit(env.DB, admin, `device.${action}`, "device", deviceId, now, async () => {
      if (action === "revoke") await env.DB.prepare("UPDATE devices SET status = 'revoked', revoked_at = ? WHERE id = ?").bind(now, deviceId).run();
      else if (action === "unbind") await env.DB.prepare("UPDATE devices SET status = 'revoked', revoked_at = ?, fingerprint_hash = ?, token_hash = ? WHERE id = ?").bind(now, `unbound-${id()}`, `unbound-${id()}`, deviceId).run();
      else { const input = await body(request); if (typeof input.enabled !== "boolean") throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400); await env.DB.prepare("UPDATE devices SET test_group = ? WHERE id = ?").bind(input.enabled ? "1" : null, deviceId).run(); }
      return response({ ok: true });
    });
  }
  if (path === "/activation-codes" && request.method === "POST") return withAudit(env.DB, admin, "activation_code.create", "activation_code", null, now, async () => {
    const input = await body(request); const count = input.count === undefined ? 1 : input.count; const days = input.days === undefined ? 7 : input.days;
    if (typeof count !== "number" || !Number.isInteger(count) || count < 1 || count > 20 || typeof days !== "number" || !Number.isInteger(days) || days < 1 || days > 30) throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400);
    const codes: Array<{ id: string; code: string; expiresAt: number }> = [];
    for (let index = 0; index < count; index += 1) { const code = randomToken(TOKEN_BYTES); const codeId = id(); const expiresAt = now + days * 86400; await env.DB.prepare("INSERT INTO activation_codes (id, code_hash, status, expires_at, created_at) VALUES (?, ?, 'unused', ?, ?)").bind(codeId, await sha256Hex(code), expiresAt, now).run(); codes.push({ id: codeId, code, expiresAt }); }
    return response({ codes }, 201);
  });
  if (path === "/activation-codes" && request.method === "GET") return response((await env.DB.prepare("SELECT id, status, used_by_device_id, used_at, expires_at, created_at FROM activation_codes ORDER BY created_at DESC LIMIT ?").bind(PAGE_LIMIT).all()).results);
  match = /^\/activation-codes\/([A-Za-z0-9_-]{1,256})\/revoke$/.exec(path);
  if (match && request.method === "POST") return withAudit(env.DB, admin, "activation_code.revoke", "activation_code", match[1]!, now, async () => { await env.DB.prepare("UPDATE activation_codes SET status = 'revoked' WHERE id = ? AND status = 'unused'").bind(match![1]).run(); return response({ ok: true }); });
  if (path === "/download-links" && request.method === "POST") return withAudit(env.DB, admin, "download_link.create", "release", null, now, async () => {
    const input = await body(request); const releaseId = stringField(input, "releaseId", 256, /^[A-Za-z0-9_-]+$/);
    const base = env.UPDATE_API_BASE_URL; if (!base) throw new HttpError("CONFIGURATION_ERROR", "Download links are unavailable.", 503);
    let origin: URL; try { origin = new URL(base); } catch { throw new HttpError("CONFIGURATION_ERROR", "Download links are unavailable.", 503); }
    if (origin.protocol !== "https:") throw new HttpError("CONFIGURATION_ERROR", "Download links are unavailable.", 503);
    const ticket = randomToken(TOKEN_BYTES); const ticketId = id(); const expiresAt = now + 86400;
    const saved = await env.DB.prepare("INSERT INTO download_tickets (id, token_hash, device_id, release_id, purpose, expires_at, created_at) SELECT ?, ?, NULL, id, 'initial', ?, ? FROM releases WHERE id = ? AND status = 'production'").bind(ticketId, await sha256Hex(ticket), expiresAt, now, releaseId).run();
    if (saved.meta.changes !== 1) throw new HttpError("RELEASE_UNAVAILABLE", "A production release is required.", 409);
    const url = new URL(`/v1/initial-download/${ticket}`, origin).toString();
    return response({ id: ticketId, expiresAt, url }, 201);
  });
  if (path === "/download-links" && request.method === "GET") return response((await env.DB.prepare("SELECT id, release_id, expires_at, revoked_at, created_at FROM download_tickets WHERE purpose = 'initial' ORDER BY created_at DESC LIMIT ?").bind(PAGE_LIMIT).all()).results);
  match = /^\/download-links\/([A-Za-z0-9_-]{1,256})\/revoke$/.exec(path);
  if (match && request.method === "POST") return withAudit(env.DB, admin, "download_link.revoke", "download_ticket", match[1]!, now, async () => { await env.DB.prepare("UPDATE download_tickets SET revoked_at = ? WHERE id = ? AND purpose = 'initial'").bind(now, match![1]).run(); return response({ ok: true }); });
  if (path === "/releases" && request.method === "GET") return response((await env.DB.prepare("SELECT id, version, status, notes, pub_date, size, sha256, platform, arch, created_at, updated_at FROM releases ORDER BY created_at DESC LIMIT ?").bind(PAGE_LIMIT).all()).results);
  match = /^\/releases\/([A-Za-z0-9_-]{1,256})\/(testing|production|halted)$/.exec(path);
  if (match && request.method === "POST") { if (!(await ownerOnly(`release.${match[2]}`, "release"))) return failure("FORBIDDEN", "Owner access is required.", 403); const [releaseId, status] = [match[1]!, match[2]!]; return withAudit(env.DB, admin, `release.${status}`, "release", releaseId, now, async () => { const prior = status === "testing" ? "draft" : status === "production" ? "testing" : "testing', 'production"; const sql = status === "halted" ? "UPDATE releases SET status = 'halted', updated_at = ? WHERE id = ? AND status IN ('testing', 'production')" : `UPDATE releases SET status = '${status}', updated_at = ? WHERE id = ? AND status = '${prior}'`; const changed = await env.DB.prepare(sql).bind(now, releaseId).run(); if (changed.meta.changes !== 1) throw new HttpError("INVALID_RELEASE_TRANSITION", "Release transition is not allowed.", 409); return response({ ok: true }); }); }
  if (path === "/admins" && request.method === "GET") { if (!(await ownerOnly("admin.list", "admin"))) return failure("FORBIDDEN", "Owner access is required.", 403); return response((await env.DB.prepare("SELECT id, email, role, created_at FROM admins ORDER BY created_at DESC LIMIT ?").bind(PAGE_LIMIT).all()).results); }
  if (path === "/admins" && request.method === "POST") { if (!(await ownerOnly("admin.create", "admin"))) return failure("FORBIDDEN", "Owner access is required.", 403); return withAudit(env.DB, admin, "admin.create", "admin", null, now, async () => { const input = await body(request); const email = stringField(input, "email", 320, /^[^\s@]+@[^\s@]+\.[^\s@]+$/).toLowerCase(); const role = input.role; if (role !== "owner" && role !== "operator") throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400); const newId = id(); await env.DB.prepare("INSERT INTO admins (id, email, password_hash, role, created_at) VALUES (?, ?, '', ?, ?)").bind(newId, email, role, now).run(); return response({ id: newId, email, role }, 201); }); }
  match = /^\/admins\/([A-Za-z0-9_-]{1,256})$/.exec(path);
  if (match && request.method === "DELETE") { if (!(await ownerOnly("admin.delete", "admin"))) return failure("FORBIDDEN", "Owner access is required.", 403); const target = match[1]!; return withAudit(env.DB, admin, "admin.delete", "admin", target, now, async () => { const deleted = await env.DB.prepare("DELETE FROM admins WHERE id = ? AND (SELECT count(*) FROM admins WHERE role = 'owner') > 1 OR id = ? AND EXISTS (SELECT 1 FROM admins WHERE id = ? AND role != 'owner')").bind(target, target, target).run(); if (deleted.meta.changes !== 1) throw new HttpError("LAST_OWNER", "At least one owner is required.", 409); return response({ ok: true }); }); }
  if (path === "/audit" && request.method === "GET") { if (!(await ownerOnly("audit.list", "audit"))) return failure("FORBIDDEN", "Owner access is required.", 403); return response((await env.DB.prepare("SELECT id, admin_id, action, target_type, target_id, details_json, created_at FROM audit_logs ORDER BY created_at DESC LIMIT ?").bind(PAGE_LIMIT).all()).results); }
  return failure("NOT_FOUND", "Route not found.", 404);
}

async function publisher(request: Request, env: AdminEnv, access: AccessIdentityAdapter, now: number): Promise<Response> {
  if (request.method !== "POST") return failure("METHOD_NOT_ALLOWED", "Method not allowed.", 405);
  const identity = await access.verify(request); if (!identity) return failure("UNAUTHORIZED", "Publisher authentication is required.", 401);
  const auditId = await audit(env.DB, null, "publisher.release.create", "release", null, "started", now);
  try { const input = releaseInput(await body(request)); const releaseId = id(); await env.DB.prepare("INSERT INTO releases (id, version, status, notes, pub_date, object_key, size, sha256, platform, arch, signature, created_at, updated_at) VALUES (?, ?, 'draft', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)").bind(releaseId, input.version, input.notes, input.pubDate, input.objectKey, input.size, input.sha256, input.platform, input.arch, input.signature, now, now).run(); await audit(env.DB, null, "publisher.release.create", "release", null, "success", now, auditId); return response({ id: releaseId, status: "draft" }, 201); }
  catch (cause) { try { await audit(env.DB, null, "publisher.release.create", "release", null, "failure", now, auditId); } catch { /* started remains */ } if (cause instanceof HttpError) return failure(cause.code, cause.message, cause.status); return failure("INVALID_REQUEST", "Unable to create release.", 400); }
}

export function createAdminWorker(options: WorkerOptions) { return { fetch: async (request: Request, env: AdminEnv, _context: ExecutionContext): Promise<Response> => { try { return await routes(options)(request, env); } catch (cause) { if (cause instanceof HttpError) return failure(cause.code, cause.message, cause.status); return failure("INTERNAL_ERROR", "Unable to process this request.", 500); } } }; }

export default { fetch(request: Request, env: AdminEnv, context: ExecutionContext) { return createAdminWorker({ adminAccess: createAccessIdentityAdapter({ issuer: env.ADMIN_ACCESS_ISSUER ?? "", audience: env.ADMIN_ACCESS_AUDIENCE ?? "", jwksUrl: env.ADMIN_ACCESS_JWKS_URL ?? "" }), publisherAccess: createAccessIdentityAdapter({ issuer: env.PUBLISHER_ACCESS_ISSUER ?? "", audience: env.PUBLISHER_ACCESS_AUDIENCE ?? "", jwksUrl: env.PUBLISHER_ACCESS_JWKS_URL ?? "" }) }).fetch(request, env, context); } };
