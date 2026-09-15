import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "vitest";

import { createAdminWorker } from "../src/admin-worker";
import { appAuthFailure, handleAppAuth } from "../src/app-auth";

const migration = ["0001_initial.sql", "0002_app_auth.sql"].map((name) => readFileSync(new URL(`../migrations/${name}`, import.meta.url), "utf8")).join("\n");
const databases: DatabaseSync[] = [];
afterEach(() => databases.splice(0).forEach((database) => database.close()));
type Statement = { sql: string; values: unknown[] };
function makeD1(database: DatabaseSync): D1Database {
  const statement = (sql: string, values: unknown[] = []) => ({ sql, values, bind: (...bound: unknown[]) => statement(sql, bound), first: async () => database.prepare(sql).get(...(values as never[])) ?? null, all: async () => ({ success: true, meta: { changes: 0 }, results: database.prepare(sql).all(...(values as never[])) }), run: async () => ({ success: true, meta: { changes: Number(database.prepare(sql).run(...(values as never[])).changes) }, results: [] }) });
  return { prepare: (sql: string) => statement(sql) as unknown as D1PreparedStatement, batch: async (statements: D1PreparedStatement[]) => {
    database.exec("BEGIN IMMEDIATE"); try { const results = (statements as unknown as Statement[]).map(({ sql, values }) => ({ success: true, meta: { changes: Number(database.prepare(sql).run(...(values as never[])).changes) }, results: [] })); database.exec("COMMIT"); return results as unknown as D1Result[]; } catch (error) { database.exec("ROLLBACK"); throw error; }
  } } as unknown as D1Database;
}

function fixture(role: "owner" | "operator") {
  const database = new DatabaseSync(":memory:"); database.exec(migration); databases.push(database);
  database.prepare("INSERT INTO admins (id,email,password_hash,role,created_at) VALUES ('admin-1','admin@example.com','',?,1)").run(role);
  const env = { DB: makeD1(database), ARTIFACTS: {} as R2Bucket, UPDATE_API_BASE_URL: "https://updates.example" };
  const worker = createAdminWorker({ adminAccess: { verify: async () => ({ email: "admin@example.com" }) }, publisherAccess: { verify: async () => null }, clock: () => 1_700_000_000 });
  return { database, env, worker };
}

function adminRequest(worker: ReturnType<typeof createAdminWorker>, env: ReturnType<typeof fixture>["env"], path: string, body: unknown) {
  return worker.fetch(new Request(`https://admin.example${path}`, { method: "POST", headers: { origin: "https://admin.example", "content-type": "application/json" }, body: JSON.stringify(body) }), env, {} as ExecutionContext);
}

describe("business account administration", () => {
  it("lets only an owner create an account without storing its initial password", async () => {
    const { database, env, worker } = fixture("owner");
    const response = await adminRequest(worker, env, "/api/admin/app-users", { username: "manager.a", displayName: "运营负责人", password: "Initial-Password-1", role: "operations_manager", anchorId: null, managedTeamIds: ["team-a"] });
    expect(response.status).toBe(201);
    const stored = database.prepare("SELECT id,password_hash,role FROM app_users WHERE username='manager.a'").get() as { id: string; password_hash: string; role: string };
    expect(stored.password_hash).toMatch(/^pbkdf2_sha256\$210000\$/);
    expect(stored.password_hash).not.toContain("Initial-Password-1");
    let login: Response;
    try { login = (await handleAppAuth(new Request("https://updates.example/v1/app-auth/login", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ username: "manager.a", password: "Initial-Password-1" }) }), env, 1_700_000_000))!; } catch (cause) { login = appAuthFailure(cause); }
    expect(login.status).toBe(200);
    expect((await login.json() as { user: { role: string } }).user.role).toBe("operations_manager");
  });

  it("rejects distribution operators from managing business accounts", async () => {
    const { env, worker } = fixture("operator");
    const response = await adminRequest(worker, env, "/api/admin/app-users", { username: "anchor.a", displayName: "主播甲", password: "Initial-Password-1", role: "anchor", anchorId: "anchor-a", managedTeamIds: [] });
    expect(response.status).toBe(403);
  });
});
