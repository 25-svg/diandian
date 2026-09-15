import { randomToken, sha256Hex } from "./crypto";
import { artifactStoreFromEnv } from "./artifact-store";
import { type AccessIdentityAdapter, type ServiceIdentityAdapter, type VerifiedAccessIdentity, type VerifiedServiceIdentity } from "./admin-auth";
import { parseVersion } from "./update-service";
import { hashAppPassword, validPassword, validUsername, verifyAppPassword, type AppRole } from "./app-auth";

export type { VerifiedAccessIdentity, VerifiedServiceIdentity } from "./admin-auth";
type Role = "owner" | "operator";
type Admin = { id: string; email: string; role: Role; must_change_password?: number };
type StoredAdmin = Admin & { password_hash: string; failed_attempts: number; locked_until: number | null; disabled_at: number | null };
type SessionAdmin = StoredAdmin & { session_id: string };
type Env = { DB: D1Database; ARTIFACTS: R2Bucket; UPDATE_API_BASE_URL?: string; PUBLISHER_API_TOKEN?: string };
type Options = { adminAccess?: AccessIdentityAdapter; publisherAccess?: ServiceIdentityAdapter; clock?: () => number };
const MAX_BODY = 8 * 1024, MAX_PAGE = 100, TOKEN_BYTES = 32;
const ADMIN_SESSION_SECONDS = 12 * 60 * 60, MAX_FAILED_ATTEMPTS = 5, LOCK_SECONDS = 15 * 60;
class HttpError extends Error { constructor(readonly code: string, message: string, readonly status: number) { super(message); } }
const json = (value: unknown, status = 200) => new Response(JSON.stringify(value), { status, headers: { "content-type": "application/json; charset=utf-8", "cache-control": "no-store" } });
const fail = (code: string, message: string, status: number) => json({ error: { code, message } }, status);
const id = () => crypto.randomUUID();
const now = (clock: () => number) => { const value = clock(); if (!Number.isSafeInteger(value)) throw new Error("Invalid clock"); return value; };

async function readBody(request: Request): Promise<Record<string, unknown>> {
  const contentType = request.headers.get("content-type") ?? "";
  if (!/^application\/json(?:\s*;|$)/i.test(contentType)) throw new HttpError("UNSUPPORTED_MEDIA_TYPE", "JSON content is required.", 415);
  const length = request.headers.get("content-length"); if (length && (!/^\d+$/.test(length) || Number(length) > MAX_BODY)) throw new HttpError("PAYLOAD_TOO_LARGE", "Request body is too large.", 413);
  if (!request.body) throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400);
  const reader = request.body.getReader(), chunks: Uint8Array[] = []; let total = 0;
  try { while (true) { const { done, value } = await reader.read(); if (done) break; total += value.byteLength; if (total > MAX_BODY) { try { await reader.cancel(); } catch {} throw new HttpError("PAYLOAD_TOO_LARGE", "Request body is too large.", 413); } chunks.push(value); } } finally { reader.releaseLock(); }
  const bytes = new Uint8Array(total); let offset = 0; for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
  try { const parsed: unknown = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes)); if (!parsed || typeof parsed !== "object" || Array.isArray(parsed)) throw new Error(); return parsed as Record<string, unknown>; } catch { throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400); }
}
function text(input: Record<string, unknown>, key: string, max: number, pattern?: RegExp) { const value = input[key]; if (typeof value !== "string" || !value || value.length > max || (pattern && !pattern.test(value))) throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400); return value; }
function assertKeys(input: Record<string, unknown>, keys: string[]) { if (Object.keys(input).some((key) => !keys.includes(key))) throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400); }
function page(request: Request) { const url = new URL(request.url); const limitRaw = url.searchParams.get("limit") ?? "50"; const cursorRaw = url.searchParams.get("cursor") ?? "0"; if (!/^\d+$/.test(limitRaw) || !/^\d+$/.test(cursorRaw)) throw new HttpError("INVALID_REQUEST", "Pagination is invalid.", 400); const limit = Number(limitRaw), cursor = Number(cursorRaw); if (!Number.isSafeInteger(limit) || limit < 1 || limit > MAX_PAGE || !Number.isSafeInteger(cursor) || cursor > 1_000_000) throw new HttpError("INVALID_REQUEST", "Pagination is invalid.", 400); return { limit, cursor }; }
async function list(db: D1Database, request: Request, sql: string) { const { limit, cursor } = page(request); const { results } = await db.prepare(`${sql} LIMIT ? OFFSET ?`).bind(limit + 1, cursor).all(); return json({ items: results.slice(0, limit), nextCursor: results.length > limit ? String(cursor + limit) : null }); }
async function findAdmin(db: D1Database, identity: VerifiedAccessIdentity) { return db.prepare("SELECT id, email, role FROM admins WHERE email = ? AND disabled_at IS NULL").bind(identity.email).first<Admin>(); }

function bearer(request: Request): string {
  const match = /^Bearer ([A-Za-z0-9_-]{43})$/.exec(request.headers.get("authorization") ?? "");
  if (!match) throw new HttpError("UNAUTHORIZED", "Authentication is required.", 401);
  return match[1]!;
}

async function localAdmin(request: Request, env: Env, timestamp: number): Promise<SessionAdmin> {
  const tokenHash = await sha256Hex(bearer(request));
  const admin = await env.DB.prepare(`SELECT a.*,s.id AS session_id FROM admin_sessions s JOIN admins a ON a.id=s.admin_id
    WHERE s.token_hash=? AND s.revoked_at IS NULL AND s.expires_at>? AND a.disabled_at IS NULL`).bind(tokenHash, timestamp).first<SessionAdmin>();
  if (!admin) throw new HttpError("UNAUTHORIZED", "Authentication is required.", 401);
  await env.DB.prepare("UPDATE admin_sessions SET last_seen_at=? WHERE id=?").bind(timestamp, admin.session_id).run();
  return admin;
}

function publicAdmin(admin: Admin) { return { id: admin.id, email: admin.email, role: admin.role, mustChangePassword: admin.must_change_password === 1 }; }

async function localAdminAuth(request: Request, env: Env, path: string, timestamp: number): Promise<Response> {
  if (path === "/login" && request.method === "POST") {
    csrf(request);
    const input = await readBody(request); assertKeys(input, ["email", "password"]);
    const email = typeof input.email === "string" ? input.email.trim().toLowerCase() : "";
    const password = typeof input.password === "string" ? input.password : "";
    const admin = /^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email) ? await env.DB.prepare("SELECT * FROM admins WHERE email=?").bind(email).first<StoredAdmin>() : null;
    const locked = Boolean(admin && admin.disabled_at === null && admin.locked_until && admin.locked_until > timestamp);
    const passwordMatches = admin?.password_hash ? await verifyAppPassword(password, admin.password_hash) : false;
    if (!admin || admin.disabled_at !== null || locked || !passwordMatches) {
      if (admin && admin.disabled_at === null && !locked) await env.DB.prepare(`UPDATE admins SET
        locked_until=CASE WHEN failed_attempts+1>=? THEN ? ELSE locked_until END,
        failed_attempts=CASE WHEN failed_attempts+1>=? THEN 0 ELSE failed_attempts+1 END,updated_at=? WHERE id=?`)
        .bind(MAX_FAILED_ATTEMPTS, timestamp + LOCK_SECONDS, MAX_FAILED_ATTEMPTS, timestamp, admin.id).run();
      throw new HttpError("INVALID_CREDENTIALS", "Email or password is incorrect.", 401);
    }
    const token = randomToken(TOKEN_BYTES);
    await env.DB.batch([
      env.DB.prepare("UPDATE admins SET failed_attempts=0,locked_until=NULL,updated_at=? WHERE id=?").bind(timestamp, admin.id),
      env.DB.prepare("INSERT INTO admin_sessions (id,admin_id,token_hash,expires_at,created_at,last_seen_at) VALUES (?,?,?,?,?,?)")
        .bind(id(), admin.id, await sha256Hex(token), timestamp + ADMIN_SESSION_SECONDS, timestamp, timestamp),
    ]);
    return json({ token, admin: publicAdmin(admin), expiresAt: timestamp + ADMIN_SESSION_SECONDS });
  }
  const admin = await localAdmin(request, env, timestamp);
  if (path === "/me" && request.method === "GET") return json({ admin: publicAdmin(admin) });
  if (path === "/logout" && request.method === "POST") { csrf(request); await env.DB.prepare("UPDATE admin_sessions SET revoked_at=? WHERE id=? AND revoked_at IS NULL").bind(timestamp, admin.session_id).run(); return new Response(null, { status: 204, headers: { "cache-control": "no-store" } }); }
  if (path === "/password" && request.method === "POST") {
    csrf(request); const input = await readBody(request); assertKeys(input, ["currentPassword", "newPassword"]);
    if (typeof input.currentPassword !== "string" || !validPassword(input.newPassword) || !await verifyAppPassword(input.currentPassword, admin.password_hash)) throw new HttpError("INVALID_PASSWORD", "Current password is incorrect or the new password is invalid.", 400);
    await env.DB.batch([
      env.DB.prepare("UPDATE admins SET password_hash=?,must_change_password=0,failed_attempts=0,locked_until=NULL,updated_at=? WHERE id=?").bind(await hashAppPassword(input.newPassword), timestamp, admin.id),
      env.DB.prepare("UPDATE admin_sessions SET revoked_at=? WHERE admin_id=? AND id!=? AND revoked_at IS NULL").bind(timestamp, admin.id, admin.session_id),
    ]);
    return json({ admin: { ...publicAdmin(admin), mustChangePassword: false } });
  }
  return fail("METHOD_NOT_ALLOWED", "Method not allowed.", 405);
}
async function audit(db: D1Database, actor: string | null, action: string, targetType: string, targetId: string | null, result: "started" | "success" | "failure", timestamp: number, details: Record<string, unknown> = {}, auditId = id()) {
  const detailsJson = JSON.stringify({ result, ...details });
  if (result === "started") await db.prepare("INSERT INTO audit_logs (id, admin_id, action, target_type, target_id, details_json, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)").bind(auditId, actor, action, targetType, targetId, detailsJson, timestamp).run();
  else await db.prepare("UPDATE audit_logs SET details_json = ? WHERE id = ?").bind(detailsJson, auditId).run();
  return auditId;
}
async function withAudit<T>(db: D1Database, admin: Admin, action: string, targetType: string, targetId: string, timestamp: number, run: () => Promise<T>, details: Record<string, unknown> = {}) {
  const auditId = await audit(db, admin.id, action, targetType, targetId, "started", timestamp, details);
  let output: T;
  try { output = await run(); } catch (cause) { try { await audit(db, admin.id, action, targetType, targetId, "failure", timestamp, details, auditId); } catch {} throw cause; }
  try { await audit(db, admin.id, action, targetType, targetId, "success", timestamp, details, auditId); } catch { /* business success remains successful; started row is durable evidence */ }
  return output!;
}
function csrf(request: Request) { const origin = request.headers.get("origin"); if (origin !== new URL(request.url).origin || request.headers.get("sec-fetch-site") === "cross-site") throw new HttpError("CSRF_REJECTED", "Cross-site write request rejected.", 403); }
function releaseInput(input: Record<string, unknown>) {
  assertKeys(input, ["version", "platform", "arch", "objectKey", "size", "sha256", "signature", "notes", "pubDate"]);
  const version = text(input, "version", 256); if (!parseVersion(version)) throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400);
  const platform = text(input, "platform", 32), arch = text(input, "arch", 32); if (platform !== "windows" || arch !== "x86_64") throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400);
  const objectKey = text(input, "objectKey", 512, /^(?!\/)(?!.*\.\.)(?!.*[\r\n\0]).+$/), sha256 = text(input, "sha256", 64, /^[a-fA-F0-9]{64}$/).toLowerCase(), signature = text(input, "signature", 4096), notes = text(input, "notes", 16_384), pubDate = text(input, "pubDate", 64);
  if (!isTauriSignature(signature)) throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400);
  if (Number.isNaN(Date.parse(pubDate)) || !Number.isSafeInteger(input.size) || (input.size as number) < 0) throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400);
  return { version, platform, arch, objectKey, size: input.size as number, sha256, signature, notes, pubDate };
}
function decodeBase64(value: string): Uint8Array | null { try { if (!/^(?:[A-Za-z0-9+/]{4})*(?:[A-Za-z0-9+/]{2}==|[A-Za-z0-9+/]{3}=)?$/.test(value)) return null; return Uint8Array.from(atob(value), (byte) => byte.charCodeAt(0)); } catch { return null; } }
/** Tauri updater .sig is base64 of a four-line minisign SignatureBox, not raw Ed25519 bytes. */
function isTauriSignature(value: string): boolean {
  const outer = decodeBase64(value); if (!outer || outer.byteLength < 180 || outer.byteLength > 3072) return false;
  let box: string; try { box = new TextDecoder("utf-8", { fatal: true }).decode(outer); } catch { return false; }
  for (const char of box) { const code = char.codePointAt(0)!; if (char === "\r" || char === "\0" || (code < 0x20 && char !== "\t" && char !== "\n") || (code >= 0x7f && code <= 0x9f)) return false; }
  // SignatureBox serialization emits one terminal LF; remove only that LF.
  if (box.endsWith("\n")) box = box.slice(0, -1);
  const lines = box.split("\n");
  const untrusted = "untrusted comment: ", trusted = "trusted comment: "; const untrustedText = lines[0]?.startsWith(untrusted) ? lines[0].slice(untrusted.length) : ""; const trustedText = lines[2]?.startsWith(trusted) ? lines[2].slice(trusted.length) : "";
  if (lines.length !== 4 || untrustedText.length < 1 || untrustedText.length > 512 || trustedText.length < 1 || trustedText.length > 512) return false;
  const packet = decodeBase64(lines[1]!); const global = decodeBase64(lines[3]!);
  return packet?.byteLength === 74 && packet[0] === 0x45 && (packet[1] === 0x44 || packet[1] === 0x64) && global?.byteLength === 64;
}
async function requireOwner(env: Env, admin: Admin, action: string, timestamp: number) { if (admin.role === "owner") return; const auditId = await audit(env.DB, admin.id, action, "authorization", admin.id, "started", timestamp); await audit(env.DB, admin.id, action, "authorization", admin.id, "failure", timestamp, {}, auditId); throw new HttpError("FORBIDDEN", "Owner access is required.", 403); }

async function adminRoute(request: Request, env: Env, admin: Admin, path: string, timestamp: number): Promise<Response> {
  if (path === "/me" && request.method === "GET") return json(admin);
  if (path === "/summary") path = "/overview";
  const write = request.method === "POST" || request.method === "DELETE";
  if (write) { try { csrf(request); } catch (cause) { const auditId = await audit(env.DB, admin.id, "csrf.reject", "request", path, "started", timestamp); await audit(env.DB, admin.id, "csrf.reject", "request", path, "failure", timestamp, {}, auditId); throw cause; } if (!/^application\/json(?:\s*;|$)/i.test(request.headers.get("content-type") ?? "")) throw new HttpError("UNSUPPORTED_MEDIA_TYPE", "JSON content is required.", 415); }
  if (path === "/overview" && request.method === "GET") { const [d, r, c] = await Promise.all([env.DB.prepare("SELECT count(*) AS count FROM devices WHERE status = 'active'").first<{ count: number }>(), env.DB.prepare("SELECT count(*) AS count FROM releases WHERE status IN ('testing','production')").first<{ count: number }>(), env.DB.prepare("SELECT count(*) AS count FROM activation_codes WHERE status='unused' AND expires_at > ?").bind(timestamp).first<{ count: number }>()]); return json({ devices: d?.count ?? 0, releases: r?.count ?? 0, activationCodes: c?.count ?? 0 }); }
  if (path === "/devices" && request.method === "GET") return list(env.DB, request, "SELECT id,status,streamer_note,current_version,test_group,activated_at,revoked_at,last_seen_at FROM devices ORDER BY created_at DESC");
  let match = /^\/devices\/([A-Za-z0-9_-]{1,256})\/(revoke|unbind|test-group)$/.exec(path);
  if (match && request.method === "POST") { const deviceId = match[1]!, action = match[2]!; return withAudit(env.DB, admin, `device.${action}`, "device", deviceId, timestamp, async () => { if (action === "revoke") await env.DB.prepare("UPDATE devices SET status='revoked', revoked_at=? WHERE id=?").bind(timestamp, deviceId).run(); else if (action === "unbind") await env.DB.prepare("UPDATE devices SET status='revoked', revoked_at=?, fingerprint_hash=?, token_hash=? WHERE id=?").bind(timestamp, `unbound-${id()}`, `unbound-${id()}`, deviceId).run(); else { const input = await readBody(request); assertKeys(input, ["enabled"]); if (typeof input.enabled !== "boolean") throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400); await env.DB.prepare("UPDATE devices SET test_group=? WHERE id=?").bind(input.enabled ? "1" : null, deviceId).run(); } return json({ ok: true }); }); }
  if (path === "/activation-codes" && request.method === "POST") { const operationId = id(), auditId = await audit(env.DB, admin.id, "activation_code.create", "activation_code_batch", operationId, "started", timestamp); try { const input = await readBody(request); assertKeys(input, ["count", "days"]); const count = input.count; if (!Number.isInteger(count) || (count as number) < 1 || (count as number) > 20 || (input.days !== undefined && input.days !== 7)) throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400); const values = await Promise.all(Array.from({ length: count as number }, async () => { const code = randomToken(TOKEN_BYTES), codeId = id(); return { code, codeId, hash: await sha256Hex(code) }; })); const details = { createdIds: values.map((value) => value.codeId) }; await env.DB.prepare("UPDATE audit_logs SET details_json=? WHERE id=?").bind(JSON.stringify({ result: "started", ...details }), auditId).run(); const expiry = timestamp + 7 * 86400; await env.DB.batch(values.map((value) => env.DB.prepare("INSERT INTO activation_codes (id,code_hash,status,expires_at,created_at) VALUES (?,?,'unused',?,?)").bind(value.codeId, value.hash, expiry, timestamp))); try { await audit(env.DB, admin.id, "activation_code.create", "activation_code_batch", operationId, "success", timestamp, details, auditId); } catch {} return json({ codes: values.map(({ codeId, code }) => ({ id: codeId, code, expiresAt: expiry })) }, 201); } catch (cause) { try { await audit(env.DB, admin.id, "activation_code.create", "activation_code_batch", operationId, "failure", timestamp, {}, auditId); } catch {} throw cause; } }
  if (path === "/activation-codes" && request.method === "GET") return list(env.DB, request, "SELECT id,status,used_by_device_id,used_at,expires_at,created_at FROM activation_codes ORDER BY created_at DESC");
  match = /^\/activation-codes\/([A-Za-z0-9_-]{1,256})\/revoke$/.exec(path); if (match && request.method === "POST") return withAudit(env.DB, admin, "activation_code.revoke", "activation_code", match[1]!, timestamp, async () => { await env.DB.prepare("UPDATE activation_codes SET status='revoked' WHERE id=? AND status='unused'").bind(match![1]).run(); return json({ ok: true }); });
  if (path === "/download-links" && request.method === "POST") { const ticketId = id(); return withAudit(env.DB, admin, "download_link.create", "download_ticket", ticketId, timestamp, async () => { const input = await readBody(request); assertKeys(input, ["releaseId"]); const releaseId = text(input, "releaseId", 256, /^[A-Za-z0-9_-]+$/), base = env.UPDATE_API_BASE_URL; if (!base) throw new HttpError("CONFIGURATION_ERROR", "Download links are unavailable.", 503); let origin: URL; try { origin = new URL(base); } catch { throw new HttpError("CONFIGURATION_ERROR", "Download links are unavailable.", 503); } if (origin.protocol !== "https:") throw new HttpError("CONFIGURATION_ERROR", "Download links are unavailable.", 503); const ticket = randomToken(TOKEN_BYTES), expiresAt = timestamp + 86400; const saved = await env.DB.prepare("INSERT INTO download_tickets (id,token_hash,device_id,release_id,purpose,expires_at,created_at) SELECT ?,?,NULL,id,'initial',?,? FROM releases WHERE id=? AND status='production'").bind(ticketId, await sha256Hex(ticket), expiresAt, timestamp, releaseId).run(); if (saved.meta.changes !== 1) throw new HttpError("RELEASE_UNAVAILABLE", "A production release is required.", 409); return json({ id: ticketId, expiresAt, url: new URL(`/v1/initial-download/${ticket}`, origin).toString() }, 201); }); }
  if (path === "/download-links" && request.method === "GET") return list(env.DB, request, "SELECT id,release_id,expires_at,revoked_at,created_at FROM download_tickets WHERE purpose='initial' ORDER BY created_at DESC");
  match = /^\/download-links\/([A-Za-z0-9_-]{1,256})\/revoke$/.exec(path); if (match && request.method === "POST") return withAudit(env.DB, admin, "download_link.revoke", "download_ticket", match[1]!, timestamp, async () => { await env.DB.prepare("UPDATE download_tickets SET revoked_at=? WHERE id=? AND purpose='initial'").bind(timestamp, match![1]).run(); return json({ ok: true }); });
  if (path === "/releases" && request.method === "GET") return list(env.DB, request, "SELECT id,version,status,notes,pub_date,size,sha256,platform,arch,created_at,updated_at FROM releases ORDER BY created_at DESC");
  match = /^\/releases\/([A-Za-z0-9_-]{1,256})\/(testing|production|halted)$/.exec(path); if (match && request.method === "POST") { const releaseId = match[1]!, status = match[2]!; await requireOwner(env, admin, `release.${status}`, timestamp); return withAudit(env.DB, admin, `release.${status}`, "release", releaseId, timestamp, async () => { if (status === "testing") { const release = await env.DB.prepare("SELECT object_key,size,sha256,platform,arch,version,pub_date,signature FROM releases WHERE id=? AND status='draft'").bind(releaseId).first<{ object_key:string;size:number;sha256:string;platform:string;arch:string;version:string;pub_date:string;signature:string }>(); if (!release || release.platform !== "windows" || release.arch !== "x86_64" || !parseVersion(release.version) || Number.isNaN(Date.parse(release.pub_date)) || !isTauriSignature(release.signature)) throw new HttpError("INVALID_RELEASE_TRANSITION", "Release transition is not allowed.", 409); const head = await env.ARTIFACTS.head(release.object_key); if (!head || head.size !== release.size || head.customMetadata?.sha256?.toLowerCase() !== release.sha256.toLowerCase()) throw new HttpError("ARTIFACT_INVALID", "Release artifact is unavailable.", 409); } const prior = status === "testing" ? "draft" : status === "production" ? "testing" : null; const statement = status === "halted" ? env.DB.prepare("UPDATE releases SET status='halted',updated_at=? WHERE id=? AND status IN ('testing','production')").bind(timestamp, releaseId) : env.DB.prepare("UPDATE releases SET status=?,updated_at=? WHERE id=? AND status=?").bind(status, timestamp, releaseId, prior); const changed = await statement.run(); if (changed.meta.changes !== 1) throw new HttpError("INVALID_RELEASE_TRANSITION", "Release transition is not allowed.", 409); return json({ ok: true }); }); }
  if (path === "/admins" && request.method === "GET") { await requireOwner(env, admin, "admin.list", timestamp); return list(env.DB, request, "SELECT id,email,role,created_at,disabled_at FROM admins WHERE disabled_at IS NULL ORDER BY created_at DESC"); }
  if (path === "/admins" && request.method === "POST") { await requireOwner(env, admin, "admin.create", timestamp); const operationId = id(), auditId = await audit(env.DB, admin.id, "admin.create", "admin_operation", operationId, "started", timestamp, { operationId }); try { const input = await readBody(request); assertKeys(input, ["email", "role", "password"]); const email = text(input, "email", 320, /^[^\s@]+@[^\s@]+\.[^\s@]+$/).toLowerCase(), role = input.role; if ((role !== "owner" && role !== "operator") || !validPassword(input.password)) throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400); const candidateId = id(); const passwordHash = await hashAppPassword(input.password as string); const saved = await env.DB.prepare("INSERT INTO admins (id,email,password_hash,role,created_at,disabled_at,must_change_password,failed_attempts,locked_until,updated_at) VALUES (?,?,?,?,?,NULL,1,0,NULL,?) ON CONFLICT(email) DO UPDATE SET password_hash=excluded.password_hash,role=excluded.role,disabled_at=NULL,must_change_password=1,failed_attempts=0,locked_until=NULL,updated_at=excluded.updated_at WHERE admins.disabled_at IS NOT NULL RETURNING id,email,role,must_change_password").bind(candidateId, email, passwordHash, role, timestamp, timestamp).first<Admin>(); if (!saved) throw new HttpError("ADMIN_EXISTS", "Administrator already exists.", 409); try { await env.DB.prepare("UPDATE audit_logs SET target_type='admin', target_id=?, details_json=? WHERE id=?").bind(saved.id, JSON.stringify({ result: "success", operationId }), auditId).run(); } catch {} return json(publicAdmin(saved), 201); } catch (cause) { try { await audit(env.DB, admin.id, "admin.create", "admin_operation", operationId, "failure", timestamp, { operationId }, auditId); } catch {} throw cause; } }
  match = /^\/admins\/([A-Za-z0-9_-]{1,256})$/.exec(path); if (match && request.method === "DELETE") { await requireOwner(env, admin, "admin.disable", timestamp); const targetId = match[1]!; return withAudit(env.DB, admin, "admin.disable", "admin", targetId, timestamp, async () => { const changed = await env.DB.prepare("UPDATE admins SET disabled_at=? WHERE id=? AND disabled_at IS NULL AND (role != 'owner' OR (SELECT count(*) FROM admins WHERE role='owner' AND disabled_at IS NULL) > 1)").bind(timestamp, targetId).run(); if (changed.meta.changes !== 1) throw new HttpError("LAST_OWNER", "At least one owner is required.", 409); return json({ ok: true }); }); }
  if (path === "/app-users" && request.method === "GET") { await requireOwner(env, admin, "app_user.list", timestamp); return list(env.DB, request, "SELECT id,username,display_name,role,anchor_id,managed_team_ids_json,must_change_password,created_at,updated_at,disabled_at FROM app_users ORDER BY created_at DESC"); }
  if (path === "/app-users" && request.method === "POST") {
    await requireOwner(env, admin, "app_user.create", timestamp);
    const input = await readBody(request); assertKeys(input, ["username", "displayName", "password", "role", "anchorId", "managedTeamIds"]);
    const username = typeof input.username === "string" ? input.username.trim().toLowerCase() : "";
    const displayName = typeof input.displayName === "string" ? input.displayName.trim() : "";
    const role = input.role as AppRole; const anchorId = input.anchorId; const managedTeamIds = input.managedTeamIds;
    if (!validUsername(username) || !validPassword(input.password) || !displayName || displayName.length > 80 || !["operations_manager", "anchor"].includes(role)
      || (role === "anchor" ? typeof anchorId !== "string" || !/^[A-Za-z0-9_-]{1,128}$/.test(anchorId) : anchorId !== null)
      || !Array.isArray(managedTeamIds) || managedTeamIds.length > 100 || managedTeamIds.some((team) => typeof team !== "string" || !/^[A-Za-z0-9_-]{1,128}$/.test(team))) throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400);
    const userId = id();
    return withAudit(env.DB, admin, "app_user.create", "app_user", userId, timestamp, async () => {
      try {
        await env.DB.prepare("INSERT INTO app_users (id,username,display_name,password_hash,role,anchor_id,managed_team_ids_json,must_change_password,created_at,updated_at) VALUES (?,?,?,?,?,?,?,1,?,?)")
          .bind(userId, username, displayName, await hashAppPassword(input.password as string), role, role === "anchor" ? anchorId : null, JSON.stringify(managedTeamIds), timestamp, timestamp).run();
      } catch { throw new HttpError("APP_USER_EXISTS", "Business account already exists.", 409); }
      return json({ id: userId, username, displayName, role, anchorId: role === "anchor" ? anchorId : null, managedTeamIds, mustChangePassword: true }, 201);
    });
  }
  match = /^\/app-users\/([A-Za-z0-9_-]{1,256})\/(disable|reset-password)$/.exec(path);
  if (match && request.method === "POST") {
    await requireOwner(env, admin, `app_user.${match[2]}`, timestamp); const userId = match[1]!, action = match[2]!;
    return withAudit(env.DB, admin, `app_user.${action}`, "app_user", userId, timestamp, async () => {
      if (action === "disable") {
        const changed = await env.DB.prepare("UPDATE app_users SET disabled_at=?,updated_at=? WHERE id=? AND disabled_at IS NULL").bind(timestamp, timestamp, userId).run();
        if (changed.meta.changes !== 1) throw new HttpError("APP_USER_NOT_FOUND", "Business account was not found.", 404);
      } else {
        const input = await readBody(request); assertKeys(input, ["password"]); if (!validPassword(input.password)) throw new HttpError("INVALID_REQUEST", "Request body is invalid.", 400);
        const changed = await env.DB.prepare("UPDATE app_users SET password_hash=?,must_change_password=1,failed_attempts=0,locked_until=NULL,updated_at=? WHERE id=? AND disabled_at IS NULL").bind(await hashAppPassword(input.password), timestamp, userId).run();
        if (changed.meta.changes !== 1) throw new HttpError("APP_USER_NOT_FOUND", "Business account was not found.", 404);
      }
      await env.DB.prepare("UPDATE app_user_sessions SET revoked_at=? WHERE user_id=? AND revoked_at IS NULL").bind(timestamp, userId).run();
      return json({ ok: true });
    });
  }
  if (path === "/audit" && request.method === "GET") { await requireOwner(env, admin, "audit.list", timestamp); return list(env.DB, request, "SELECT id,admin_id,action,target_type,target_id,details_json,created_at FROM audit_logs ORDER BY created_at DESC"); }
  return fail("NOT_FOUND", "Route not found.", 404);
}

async function publisher(request: Request, env: Env, access: ServiceIdentityAdapter | undefined, timestamp: number) {
  if (request.method !== "POST") return fail("METHOD_NOT_ALLOWED", "Method not allowed.", 405);
  let machineId = "publisher-token";
  if (access) { const machine = await access.verify(request); if (!machine) return fail("UNAUTHORIZED", "Publisher authentication is required.", 401); machineId = machine.serviceTokenId; }
  else { const configured = env.PUBLISHER_API_TOKEN ?? "", supplied = /^Bearer (.+)$/.exec(request.headers.get("authorization") ?? "")?.[1] ?? ""; if (!configured || !supplied || await sha256Hex(configured) !== await sha256Hex(supplied)) return fail("UNAUTHORIZED", "Publisher authentication is required.", 401); }
  const targetId = id(), auditId = await audit(env.DB, null, "publisher.release.create", "release", targetId, "started", timestamp, { machineId });
  try { const input = releaseInput(await readBody(request)); await env.DB.prepare("INSERT INTO releases (id,version,status,notes,pub_date,object_key,size,sha256,platform,arch,signature,created_at,updated_at) VALUES (?,?,'draft',?,?,?,?,?,?,?,?,?,?)").bind(targetId,input.version,input.notes,input.pubDate,input.objectKey,input.size,input.sha256,input.platform,input.arch,input.signature,timestamp,timestamp).run(); try { await audit(env.DB,null,"publisher.release.create","release",targetId,"success",timestamp,{ machineId },auditId); } catch {} return json({ id: targetId,status:"draft" },201); } catch (cause) { try { await audit(env.DB,null,"publisher.release.create","release",targetId,"failure",timestamp,{ machineId },auditId); } catch {} if (cause instanceof HttpError) return fail(cause.code,cause.message,cause.status); return fail("INVALID_REQUEST","Unable to create release.",400); }
}
export function createAdminWorker(options: Options = {}) { const clock = options.clock ?? (() => Math.floor(Date.now()/1000)); return { async fetch(request: Request, env: Env, _context: ExecutionContext): Promise<Response> { try { const path = new URL(request.url).pathname, timestamp = now(clock); if (path === "/api/publisher/releases") return await publisher(request,env,options.publisherAccess,timestamp); if (path.startsWith("/api/auth/")) return await localAdminAuth(request,env,path.slice("/api/auth".length),timestamp); if (!path.startsWith("/api/admin/")) return fail("NOT_FOUND","Route not found.",404); let admin: Admin | null; if (options.adminAccess) { const identity = await options.adminAccess.verify(request); if (!identity) return fail("UNAUTHORIZED","Authentication is required.",401); admin = await findAdmin(env.DB,identity); } else admin = await localAdmin(request,env,timestamp); if (!admin) return fail("FORBIDDEN","Administrator access is required.",403); if (admin.must_change_password === 1) return fail("PASSWORD_CHANGE_REQUIRED","Password change is required.",403); return await adminRoute(request,env,admin,path.slice("/api/admin".length),timestamp); } catch (cause) { if (cause instanceof HttpError) return fail(cause.code,cause.message,cause.status); return fail("INTERNAL_ERROR","Unable to process this request.",500); } } }; }
export default { fetch(request: Request, env: Env, context: ExecutionContext) { return createAdminWorker().fetch(request,{ ...env, ARTIFACTS: artifactStoreFromEnv(env) as R2Bucket },context); } };
