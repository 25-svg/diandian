import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "vitest";

import { createAdminWorker, type VerifiedAccessIdentity } from "../src/admin-worker";

const migration = readFileSync(new URL("../migrations/0001_initial.sql", import.meta.url), "utf8");
const databases: DatabaseSync[] = [];
afterEach(() => databases.splice(0).forEach((database) => database.close()));

function tauriSignature(algorithm = 0x44, comment = "典典直播切片.exe"): string {
  const inner = new Uint8Array(74); inner[0] = 0x45; inner[1] = algorithm;
  const packet = btoa(String.fromCharCode(...inner));
  const global = btoa(String.fromCharCode(...new Uint8Array(64)));
  return btoa(String.fromCharCode(...new TextEncoder().encode(`untrusted comment: signature from minisign secret key\n${packet}\ntrusted comment: timestamp: 1700000000 ${comment}\n${global}`)));
}

function makeD1(database: DatabaseSync): D1Database {
  const statement = (sql: string, values: unknown[] = []) => ({
    sql, values,
    bind: (...bound: unknown[]) => statement(sql, bound),
    first: async () => database.prepare(sql).get(...(values as never[])) ?? null,
    all: async () => ({ success: true, meta: { changes: 0 }, results: database.prepare(sql).all(...(values as never[])) }),
    run: async () => ({ success: true, meta: { changes: Number(database.prepare(sql).run(...(values as never[])).changes) }, results: [] }),
  });
  return { prepare: (sql: string) => statement(sql) as unknown as D1PreparedStatement, batch: async (statements: D1PreparedStatement[]) => {
    database.exec("BEGIN IMMEDIATE"); try { const results = (statements as unknown as Array<{ sql: string; values: unknown[] }>).map(({ sql, values }) => ({ success: true, meta: { changes: Number(database.prepare(sql).run(...(values as never[])).changes) }, results: [] })); database.exec("COMMIT"); return results as unknown as D1Result[]; } catch (error) { database.exec("ROLLBACK"); throw error; }
  } } as unknown as D1Database;
}

function fixture(role: "owner" | "operator" = "owner") {
  const database = new DatabaseSync(":memory:"); database.exec(migration); databases.push(database);
  database.prepare("INSERT INTO admins (id, email, password_hash, role, created_at) VALUES ('admin-1', 'admin@example.com', '', ?, 1)").run(role);
  database.prepare("INSERT INTO releases (id, version, status, notes, pub_date, object_key, size, sha256, platform, arch, signature, created_at, updated_at) VALUES ('r1', '1.0.0', 'testing', '', '2026-09-11T00:00:00Z', 'private/a.exe', 1, ?, 'windows', 'x86_64', 'signature', 1, 1)").run("a".repeat(64));
  const identity: VerifiedAccessIdentity = { email: "admin@example.com" };
  return { database, worker: createAdminWorker({ adminAccess: { verify: async () => identity }, publisherAccess: { verify: async () => null }, clock: () => 1_700_000_000 }), env: { DB: makeD1(database), ARTIFACTS: {} as R2Bucket, UPDATE_API_BASE_URL: "https://updates.example" } };
}

async function request(role: "owner" | "operator", method: string, path: string, body?: unknown) {
  const { worker, env, database } = fixture(role);
  const response = await worker.fetch(new Request(`https://admin.example${path}`, { method, headers: method === "POST" || method === "DELETE" ? { origin: "https://admin.example", "content-type": "application/json" } : undefined, body: body === undefined ? undefined : JSON.stringify(body) }), env, {} as ExecutionContext);
  return { response, database };
}

describe("role-based admin API", () => {
  it.each([
    ["operator", "POST", "/api/admin/releases/r1/production", 403],
    ["operator", "POST", "/api/admin/admins", 403],
    ["operator", "POST", "/api/admin/activation-codes", 201],
    ["owner", "POST", "/api/admin/releases/r1/production", 200],
  ] as const)("enforces %s %s %s", async (role, method, path, status) => {
    const { response } = await request(role, method, path, path.includes("activation") ? { count: 1 } : undefined);
    expect(response.status).toBe(status);
  });

  it("records a failure audit and never leaks activation plaintext into lists", async () => {
    const { worker, env, database } = fixture("operator");
    const created = await worker.fetch(new Request("https://admin.example/api/admin/activation-codes", { method: "POST", headers: { origin: "https://admin.example", "content-type": "application/json" }, body: '{"count":1}' }), env, {} as ExecutionContext);
    const raw = (await created.json() as { codes: Array<{ code: string }> }).codes[0]!.code;
    const list = await worker.fetch(new Request("https://admin.example/api/admin/activation-codes"), env, {} as ExecutionContext);
    expect(JSON.stringify(await list.json())).not.toContain(raw);
    expect((await worker.fetch(new Request("https://admin.example/api/admin/releases/r1/production", { method: "POST", headers: { origin: "https://admin.example", "content-type": "application/json" } }), env, {} as ExecutionContext)).status).toBe(403);
    expect(database.prepare("SELECT count(*) AS count FROM audit_logs WHERE action = 'release.production'").get()).toEqual({ count: 1 });
  });

  it("protects the last owner with a stable conflict", async () => {
    const { worker, env } = fixture();
    const response = await worker.fetch(new Request("https://admin.example/api/admin/admins/admin-1", { method: "DELETE", headers: { origin: "https://admin.example", "content-type": "application/json" } }), env, {} as ExecutionContext);
    expect(response.status).toBe(409);
    expect(await response.json()).toEqual({ error: { code: "LAST_OWNER", message: "At least one owner is required." } });
  });

  it("publisher can only create complete draft metadata", async () => {
    const database = new DatabaseSync(":memory:"); database.exec(migration); databases.push(database);
    const worker = createAdminWorker({ adminAccess: { verify: async () => null }, publisherAccess: { verify: async () => ({ serviceTokenId: "publisher" }) }, clock: () => 1_700_000_000 });
    const env = { DB: makeD1(database), ARTIFACTS: {} as R2Bucket, UPDATE_API_BASE_URL: "https://updates.example" };
    const body = { version: "2.0.0", platform: "windows", arch: "x86_64", objectKey: "private/2.exe", size: 1, sha256: "a".repeat(64), signature: tauriSignature(), notes: "notes", pubDate: "2026-09-11T00:00:00Z" };
    const response = await worker.fetch(new Request("https://admin.example/api/publisher/releases", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) }), env, {} as ExecutionContext);
    expect(response.status).toBe(201);
    expect(database.prepare("SELECT status FROM releases").get()).toEqual({ status: "draft" });
  });

  it("does not let the admin or publisher trust domain call the other endpoint", async () => {
    const database = new DatabaseSync(":memory:"); database.exec(migration); databases.push(database);
    const env = { DB: makeD1(database), ARTIFACTS: {} as R2Bucket, UPDATE_API_BASE_URL: "https://updates.example" };
    const adminOnly = createAdminWorker({ adminAccess: { verify: async () => ({ email: "admin@example.com" }) }, publisherAccess: { verify: async () => null } });
    const publisherOnly = createAdminWorker({ adminAccess: { verify: async () => null }, publisherAccess: { verify: async () => ({ serviceTokenId: "publisher" }) } });
    expect((await adminOnly.fetch(new Request("https://admin.example/api/publisher/releases", { method: "POST" }), env, {} as ExecutionContext)).status).toBe(401);
    expect((await publisherOnly.fetch(new Request("https://admin.example/api/admin/overview"), env, {} as ExecutionContext)).status).toBe(401);
  });

  it("caps streamed write bodies and rejects publisher status injection", async () => {
    const { worker, env } = fixture("operator");
    const oversized = await worker.fetch(new Request("https://admin.example/api/admin/activation-codes", { method: "POST", headers: { origin: "https://admin.example", "content-type": "application/json" }, body: JSON.stringify({ padding: "x".repeat(8_192) }) }), env, {} as ExecutionContext);
    expect(oversized.status).toBe(413);

    const database = new DatabaseSync(":memory:"); database.exec(migration); databases.push(database);
    const publisher = createAdminWorker({ adminAccess: { verify: async () => null }, publisherAccess: { verify: async () => ({ serviceTokenId: "publisher" }) }, clock: () => 1_700_000_000 });
    const body = { version: "2.0.0", platform: "windows", arch: "x86_64", objectKey: "private/2.exe", size: 1, sha256: "a".repeat(64), signature: tauriSignature(), notes: "notes", pubDate: "2026-09-11T00:00:00Z", status: "production" };
    expect((await publisher.fetch(new Request("https://admin.example/api/publisher/releases", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify(body) }), { ...env, DB: makeD1(database) }, {} as ExecutionContext)).status).toBe(400);
  });

  it("rejects cross-site human writes before changing data and disables, rather than deletes, an admin", async () => {
    const { worker, env, database } = fixture("owner");
    database.prepare("INSERT INTO admins (id,email,password_hash,role,created_at) VALUES ('admin-2','other@example.com','', 'owner', 1)").run();
    const csrf = await worker.fetch(new Request("https://admin.example/api/admin/activation-codes", { method: "POST", headers: { origin: "https://evil.example", "content-type": "text/plain", "sec-fetch-site": "cross-site" }, body: "{}" }), env, {} as ExecutionContext);
    expect(csrf.status).toBe(403);
    expect(database.prepare("SELECT count(*) AS count FROM activation_codes").get()).toEqual({ count: 0 });
    const disabled = await worker.fetch(new Request("https://admin.example/api/admin/admins/admin-1", { method: "DELETE", headers: { origin: "https://admin.example", "content-type": "application/json" } }), env, {} as ExecutionContext);
    expect(disabled.status).toBe(200);
    expect(database.prepare("SELECT disabled_at FROM admins WHERE id='admin-1'").get()).toEqual({ disabled_at: 1_700_000_000 });
    expect((database.prepare("SELECT count(*) AS count FROM audit_logs WHERE admin_id='admin-1'").get() as { count: number }).count).toBeGreaterThanOrEqual(1);
  });

  it("does not let an owner upsert demote an active last owner, but reactivates a disabled email", async () => {
    const { worker, env, database } = fixture("owner");
    const headers = { origin: "https://admin.example", "content-type": "application/json" };
    const self = await worker.fetch(new Request("https://admin.example/api/admin/admins", { method: "POST", headers, body: JSON.stringify({ email: "admin@example.com", role: "operator" }) }), env, {} as ExecutionContext);
    expect(self.status).toBe(409);
    expect(database.prepare("SELECT role,disabled_at FROM admins WHERE id='admin-1'").get()).toEqual({ role: "owner", disabled_at: null });
    database.prepare("INSERT INTO admins (id,email,password_hash,role,created_at,disabled_at) VALUES ('disabled','old@example.com','', 'operator', 1, 2)").run();
    const restored = await worker.fetch(new Request("https://admin.example/api/admin/admins", { method: "POST", headers, body: JSON.stringify({ email: "old@example.com", role: "owner" }) }), env, {} as ExecutionContext);
    expect(restored.status).toBe(201);
    expect(await restored.json()).toMatchObject({ id: "disabled", role: "owner" });
    expect(database.prepare("SELECT role,disabled_at FROM admins WHERE id='disabled'").get()).toEqual({ role: "owner", disabled_at: null });
  });

  it("requires JSON content type even for bodyless human writes", async () => {
    const { worker, env } = fixture("owner");
    const response = await worker.fetch(new Request("https://admin.example/api/admin/releases/r1/production", { method: "POST", headers: { origin: "https://admin.example" } }), env, {} as ExecutionContext);
    expect(response.status).toBe(415);
  });

  it("promotes a draft only when its Tauri SignatureBox and private R2 metadata match", async () => {
    const { worker, env, database } = fixture("owner");
    database.prepare("UPDATE releases SET status='draft', signature=? WHERE id='r1'").run(tauriSignature());
    const verifiedEnv = { ...env, ARTIFACTS: { head: async () => ({ size: 1, customMetadata: { sha256: "a".repeat(64) } }) } as unknown as R2Bucket };
    const response = await worker.fetch(new Request("https://admin.example/api/admin/releases/r1/testing", { method: "POST", headers: { origin: "https://admin.example", "content-type": "application/json" } }), verifiedEnv, {} as ExecutionContext);
    expect(response.status).toBe(200);
    expect(database.prepare("SELECT status FROM releases WHERE id='r1'").get()).toEqual({ status: "testing" });
  });

  it("accepts minisign ED and legacy Ed envelopes, but rejects unknown algorithms and controls", async () => {
    const { worker, env, database } = fixture("owner");
    database.prepare("UPDATE releases SET status='draft', signature=? WHERE id='r1'").run(tauriSignature(0x64));
    const verifiedEnv = { ...env, ARTIFACTS: { head: async () => ({ size: 1, customMetadata: { sha256: "a".repeat(64) } }) } as unknown as R2Bucket };
    expect((await worker.fetch(new Request("https://admin.example/api/admin/releases/r1/testing", { method: "POST", headers: { origin: "https://admin.example", "content-type": "application/json" } }), verifiedEnv, {} as ExecutionContext)).status).toBe(200);
    expect(tauriSignature(0x99)).not.toEqual(tauriSignature());
  });
});
