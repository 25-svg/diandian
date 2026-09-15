import { randomToken, sha256Hex } from "./crypto";

export type AppRole = "operations_manager" | "anchor";
export type AppUser = {
  id: string;
  username: string;
  displayName: string;
  role: AppRole;
  anchorId: string | null;
  managedTeamIds: string[];
  mustChangePassword: boolean;
};

type Env = { DB: D1Database };
type StoredUser = {
  id: string; username: string; display_name: string; password_hash: string; role: AppRole;
  anchor_id: string | null; managed_team_ids_json: string; must_change_password: number;
  failed_attempts: number; locked_until: number | null; disabled_at: number | null;
};
type SessionUser = StoredUser & { session_id: string };

const MAX_BODY = 4 * 1024;
const SESSION_SECONDS = 12 * 60 * 60;
// Cloudflare Workers WebCrypto currently caps PBKDF2 at 100,000 iterations.
const PBKDF2_ITERATIONS = 100_000;
const MAX_FAILED_ATTEMPTS = 5;
const LOCK_SECONDS = 15 * 60;
const DUMMY_PASSWORD_HASH = "pbkdf2_sha256$100000$ZGlhbmRpYW4tZHVtbXktdjE$rjmUmRCndQCooEL7RfHs2iPrC3jeoYIv3wqArF-z-_0";
const encoder = new TextEncoder();

export class AppAuthError extends Error {
  constructor(readonly code: string, message: string, readonly status: number) { super(message); }
}

function base64url(bytes: Uint8Array): string {
  return btoa(String.fromCharCode(...bytes)).replace(/\+/g, "-").replace(/\//g, "_").replace(/=+$/g, "");
}

function decodeBase64url(value: string): Uint8Array | null {
  if (!/^[A-Za-z0-9_-]+$/.test(value)) return null;
  try {
    const padded = value.replace(/-/g, "+").replace(/_/g, "/") + "===".slice((value.length + 3) % 4);
    return Uint8Array.from(atob(padded), (char) => char.charCodeAt(0));
  } catch { return null; }
}

function sameBytes(left: Uint8Array, right: Uint8Array): boolean {
  if (left.byteLength !== right.byteLength) return false;
  let difference = 0;
  for (let index = 0; index < left.byteLength; index += 1) difference |= left[index]! ^ right[index]!;
  return difference === 0;
}

async function derivePassword(password: string, salt: Uint8Array, iterations: number): Promise<Uint8Array> {
  const key = await crypto.subtle.importKey("raw", encoder.encode(password), "PBKDF2", false, ["deriveBits"]);
  const stableSalt = Uint8Array.from(salt);
  return new Uint8Array(await crypto.subtle.deriveBits({ name: "PBKDF2", hash: "SHA-256", salt: stableSalt.buffer, iterations }, key, 256));
}

export function validUsername(value: unknown): value is string {
  return typeof value === "string" && /^[a-z0-9][a-z0-9._-]{2,63}$/.test(value);
}

export function validPassword(value: unknown): value is string {
  return typeof value === "string" && value.length >= 10 && value.length <= 128;
}

export async function hashAppPassword(password: string): Promise<string> {
  if (!validPassword(password)) throw new AppAuthError("INVALID_PASSWORD", "Password must contain 10 to 128 characters.", 400);
  const salt = crypto.getRandomValues(new Uint8Array(16));
  const hash = await derivePassword(password, salt, PBKDF2_ITERATIONS);
  return `pbkdf2_sha256$${PBKDF2_ITERATIONS}$${base64url(salt)}$${base64url(hash)}`;
}

export async function verifyAppPassword(password: string, encoded: string): Promise<boolean> {
  const parts = encoded.split("$");
  const iterations = Number(parts[1]);
  const salt = parts[2] ? decodeBase64url(parts[2]) : null;
  const expected = parts[3] ? decodeBase64url(parts[3]) : null;
  if (parts.length !== 4 || parts[0] !== "pbkdf2_sha256" || iterations !== PBKDF2_ITERATIONS || !salt || salt.byteLength !== 16 || !expected || expected.byteLength !== 32 || typeof password !== "string" || password.length < 1 || password.length > 128) return false;
  return sameBytes(await derivePassword(password, salt, iterations), expected);
}

async function readBody(request: Request): Promise<Record<string, unknown>> {
  if (!/^application\/json(?:\s*;|$)/i.test(request.headers.get("content-type") ?? "")) throw new AppAuthError("UNSUPPORTED_MEDIA_TYPE", "JSON content is required.", 415);
  const declared = request.headers.get("content-length");
  if (declared && (!/^\d+$/.test(declared) || Number(declared) > MAX_BODY)) throw new AppAuthError("PAYLOAD_TOO_LARGE", "Request body is too large.", 413);
  const text = await request.text();
  if (encoder.encode(text).byteLength > MAX_BODY) throw new AppAuthError("PAYLOAD_TOO_LARGE", "Request body is too large.", 413);
  try {
    const value: unknown = JSON.parse(text);
    if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error();
    return value as Record<string, unknown>;
  } catch { throw new AppAuthError("INVALID_REQUEST", "Request body is invalid.", 400); }
}

function bearer(request: Request): string {
  const match = /^Bearer ([A-Za-z0-9_-]{43})$/.exec(request.headers.get("authorization") ?? "");
  if (!match) throw new AppAuthError("UNAUTHORIZED", "Authentication is required.", 401);
  return match[1]!;
}

function teams(json: string): string[] {
  try {
    const value: unknown = JSON.parse(json);
    return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
  } catch { return []; }
}

function publicUser(user: StoredUser): AppUser {
  return { id: user.id, username: user.username, displayName: user.display_name, role: user.role, anchorId: user.anchor_id, managedTeamIds: teams(user.managed_team_ids_json), mustChangePassword: user.must_change_password === 1 };
}

async function sessionUser(request: Request, env: Env, now: number): Promise<SessionUser> {
  const hash = await sha256Hex(bearer(request));
  const user = await env.DB.prepare(`SELECT u.*, s.id AS session_id FROM app_user_sessions s JOIN app_users u ON u.id=s.user_id
    WHERE s.token_hash=? AND s.revoked_at IS NULL AND s.expires_at>? AND u.disabled_at IS NULL`).bind(hash, now).first<SessionUser>();
  if (!user) throw new AppAuthError("UNAUTHORIZED", "Authentication is required.", 401);
  await env.DB.prepare("UPDATE app_user_sessions SET last_seen_at=? WHERE id=?").bind(now, user.session_id).run();
  return user;
}

function response(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), { status, headers: { "content-type": "application/json; charset=utf-8", "cache-control": "no-store" } });
}

export async function handleAppAuth(request: Request, env: Env, now = Math.floor(Date.now() / 1000)): Promise<Response | null> {
  const path = new URL(request.url).pathname;
  if (!path.startsWith("/v1/app-auth/")) return null;
  if (path === "/v1/app-auth/login" && request.method === "POST") {
    const input = await readBody(request);
    if (Object.keys(input).some((key) => !["username", "password"].includes(key)) || typeof input.username !== "string" || typeof input.password !== "string") throw new AppAuthError("INVALID_REQUEST", "Request body is invalid.", 400);
    const username = input.username.trim().toLowerCase();
    const user = validUsername(username) ? await env.DB.prepare("SELECT * FROM app_users WHERE username=?").bind(username).first<StoredUser>() : null;
    const locked = Boolean(user && user.disabled_at === null && user.locked_until && user.locked_until > now);
    // Always perform the same expensive verification so unknown/disabled/locked accounts
    // cannot be distinguished from ordinary invalid credentials by response timing.
    const passwordMatches = await verifyAppPassword(input.password, user?.password_hash ?? DUMMY_PASSWORD_HASH);
    const valid = user && user.disabled_at === null && !locked && passwordMatches;
    if (!valid) {
      if (user && user.disabled_at === null && !locked) {
        await env.DB.prepare(`UPDATE app_users SET
          locked_until=CASE WHEN failed_attempts+1>=? THEN ? ELSE locked_until END,
          failed_attempts=CASE WHEN failed_attempts+1>=? THEN 0 ELSE failed_attempts+1 END,
          updated_at=? WHERE id=? AND disabled_at IS NULL`)
          .bind(MAX_FAILED_ATTEMPTS, now + LOCK_SECONDS, MAX_FAILED_ATTEMPTS, now, user.id).run();
      }
      throw new AppAuthError("INVALID_CREDENTIALS", "Username or password is incorrect.", 401);
    }
    const token = randomToken(32);
    await env.DB.batch([
      env.DB.prepare("UPDATE app_users SET failed_attempts=0,locked_until=NULL,updated_at=? WHERE id=?").bind(now, user.id),
      env.DB.prepare("INSERT INTO app_user_sessions (id,user_id,token_hash,expires_at,created_at,last_seen_at) VALUES (?,?,?,?,?,?)")
        .bind(crypto.randomUUID(), user.id, await sha256Hex(token), now + SESSION_SECONDS, now, now),
    ]);
    return response({ token, user: publicUser(user), expiresAt: now + SESSION_SECONDS });
  }
  if (path === "/v1/app-auth/me" && request.method === "GET") return response({ user: publicUser(await sessionUser(request, env, now)) });
  if (path === "/v1/app-auth/logout" && request.method === "POST") {
    const user = await sessionUser(request, env, now);
    await env.DB.prepare("UPDATE app_user_sessions SET revoked_at=? WHERE id=? AND revoked_at IS NULL").bind(now, user.session_id).run();
    return new Response(null, { status: 204, headers: { "cache-control": "no-store" } });
  }
  if (path === "/v1/app-auth/password" && request.method === "POST") {
    const user = await sessionUser(request, env, now);
    const input = await readBody(request);
    if (Object.keys(input).some((key) => !["currentPassword", "newPassword"].includes(key)) || typeof input.currentPassword !== "string" || !validPassword(input.newPassword) || !await verifyAppPassword(input.currentPassword, user.password_hash)) throw new AppAuthError("INVALID_PASSWORD", "Current password is incorrect or the new password is invalid.", 400);
    const passwordHash = await hashAppPassword(input.newPassword);
    await env.DB.batch([
      env.DB.prepare("UPDATE app_users SET password_hash=?,must_change_password=0,updated_at=? WHERE id=?").bind(passwordHash, now, user.id),
      env.DB.prepare("UPDATE app_user_sessions SET revoked_at=? WHERE user_id=? AND id!=? AND revoked_at IS NULL").bind(now, user.id, user.session_id),
    ]);
    return response({ user: { ...publicUser(user), mustChangePassword: false } });
  }
  return response({ error: { code: "METHOD_NOT_ALLOWED", message: "Method not allowed." } }, 405);
}

export function appAuthFailure(cause: unknown): Response {
  if (cause instanceof AppAuthError) return response({ error: { code: cause.code, message: cause.message } }, cause.status);
  return response({ error: { code: "INTERNAL_ERROR", message: "Unable to process this request." } }, 500);
}
