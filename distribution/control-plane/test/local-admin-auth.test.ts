import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "vitest";

import { createAdminWorker } from "../src/admin-worker";
import { hashAppPassword } from "../src/app-auth";

const migration = ["0001_initial.sql", "0002_app_auth.sql", "0003_local_admin_auth.sql"].map((name) => readFileSync(new URL(`../migrations/${name}`, import.meta.url), "utf8")).join("\n");
const databases: DatabaseSync[] = [];
afterEach(() => databases.splice(0).forEach((database) => database.close()));
type Statement = { sql: string; values: unknown[] };

function makeD1(database: DatabaseSync): D1Database {
  const statement = (sql: string, values: unknown[] = []) => ({ sql, values, bind: (...bound: unknown[]) => statement(sql, bound), first: async () => database.prepare(sql).get(...(values as never[])) ?? null, all: async () => ({ success: true, meta: { changes: 0 }, results: database.prepare(sql).all(...(values as never[])) }), run: async () => ({ success: true, meta: { changes: Number(database.prepare(sql).run(...(values as never[])).changes) }, results: [] }) });
  return { prepare: (sql: string) => statement(sql) as unknown as D1PreparedStatement, batch: async (statements: D1PreparedStatement[]) => {
    database.exec("BEGIN IMMEDIATE"); try { const results = (statements as unknown as Statement[]).map(({ sql, values }) => ({ success: true, meta: { changes: Number(database.prepare(sql).run(...(values as never[])).changes) }, results: [] })); database.exec("COMMIT"); return results as unknown as D1Result[]; } catch (error) { database.exec("ROLLBACK"); throw error; }
  } } as unknown as D1Database;
}

async function fixture() {
  const database = new DatabaseSync(":memory:"); database.exec(migration); databases.push(database);
  database.prepare("INSERT INTO admins (id,email,password_hash,role,created_at,must_change_password,updated_at) VALUES (?,?,?,?,?,?,?)").run("owner-1", "owner@example.com", await hashAppPassword("Initial-Password-1"), "owner", 1, 1, 1);
  const env = { DB: makeD1(database), ARTIFACTS: {} as R2Bucket, UPDATE_API_BASE_URL: "https://updates.example", PUBLISHER_API_TOKEN: "publisher-secret" };
  return { database, env, worker: createAdminWorker({ clock: () => 1_700_000_000 }) };
}

describe("card-free local admin authentication", () => {
  it("requires first-login password change before opening the admin API", async () => {
    const { worker, env } = await fixture();
    const login = await worker.fetch(new Request("https://admin.example/api/auth/login", { method: "POST", headers: { origin: "https://admin.example", "content-type": "application/json" }, body: JSON.stringify({ email: "owner@example.com", password: "Initial-Password-1" }) }), env, {} as ExecutionContext);
    expect(login.status).toBe(200); const token = (await login.json() as { token: string }).token; const headers = { authorization: `Bearer ${token}` };
    expect((await worker.fetch(new Request("https://admin.example/api/admin/me", { headers }), env, {} as ExecutionContext)).status).toBe(403);
    const changed = await worker.fetch(new Request("https://admin.example/api/auth/password", { method: "POST", headers: { ...headers, origin: "https://admin.example", "content-type": "application/json" }, body: JSON.stringify({ currentPassword: "Initial-Password-1", newPassword: "Changed-Password-2" }) }), env, {} as ExecutionContext);
    expect(changed.status).toBe(200);
    expect((await worker.fetch(new Request("https://admin.example/api/admin/me", { headers }), env, {} as ExecutionContext)).status).toBe(200);
  });

  it("accepts only the configured publisher bearer token", async () => {
    const { worker, env } = await fixture();
    const body = JSON.stringify({ version: "2.0.0", platform: "windows", arch: "x86_64", objectKey: "private/2.0.0.exe", size: 1, sha256: "a".repeat(64), signature: "invalid", notes: "notes", pubDate: "2026-09-11T00:00:00Z" });
    const unauthorized = await worker.fetch(new Request("https://admin.example/api/publisher/releases", { method: "POST", headers: { "content-type": "application/json" }, body }), env, {} as ExecutionContext);
    expect(unauthorized.status).toBe(401);
    const authorized = await worker.fetch(new Request("https://admin.example/api/publisher/releases", { method: "POST", headers: { authorization: "Bearer publisher-secret", "content-type": "application/json" }, body }), env, {} as ExecutionContext);
    expect(authorized.status).toBe(400);
  });
});
