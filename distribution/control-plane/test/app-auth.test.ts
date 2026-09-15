import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "vitest";

import { appAuthFailure, handleAppAuth, hashAppPassword } from "../src/app-auth";

const migration = ["0001_initial.sql", "0002_app_auth.sql"].map((name) => readFileSync(new URL(`../migrations/${name}`, import.meta.url), "utf8")).join("\n");
const databases: DatabaseSync[] = [];
afterEach(() => databases.splice(0).forEach((database) => database.close()));

type Statement = { sql: string; values: unknown[] };
function makeD1(database: DatabaseSync): D1Database {
  const statement = (sql: string, values: unknown[] = []) => ({
    sql, values,
    bind: (...bound: unknown[]) => statement(sql, bound),
    first: async () => database.prepare(sql).get(...(values as never[])) ?? null,
    run: async () => ({ success: true, meta: { changes: Number(database.prepare(sql).run(...(values as never[])).changes) }, results: [] }),
    all: async () => ({ success: true, meta: { changes: 0 }, results: database.prepare(sql).all(...(values as never[])) }),
  });
  return { prepare: (sql: string) => statement(sql) as unknown as D1PreparedStatement, batch: async (statements: D1PreparedStatement[]) => {
    database.exec("BEGIN IMMEDIATE");
    try {
      const results = (statements as unknown as Statement[]).map(({ sql, values }) => ({ success: true, meta: { changes: Number(database.prepare(sql).run(...(values as never[])).changes) }, results: [] }));
      database.exec("COMMIT"); return results as unknown as D1Result[];
    } catch (error) { database.exec("ROLLBACK"); throw error; }
  } } as unknown as D1Database;
}

async function fixture(role: "operations_manager" | "anchor" = "anchor") {
  const database = new DatabaseSync(":memory:"); database.exec(migration); databases.push(database);
  database.prepare("INSERT INTO app_users (id,username,display_name,password_hash,role,anchor_id,managed_team_ids_json,must_change_password,created_at,updated_at) VALUES (?,?,?,?,?,?,?,?,?,?)")
    .run("user-1", role === "anchor" ? "anchor.a" : "manager.a", role === "anchor" ? "主播甲" : "运营负责人", await hashAppPassword("Initial-Password-1"), role, role === "anchor" ? "anchor-a" : null, role === "anchor" ? "[]" : '["team-a"]', 1, 1, 1);
  return { database, env: { DB: makeD1(database) }, username: role === "anchor" ? "anchor.a" : "manager.a" };
}

async function call(request: Request, env: { DB: D1Database }, now = 1_700_000_000) {
  try { return (await handleAppAuth(request, env, now))!; } catch (cause) { return appAuthFailure(cause); }
}

async function login(env: { DB: D1Database }, username: string, password = "Initial-Password-1", now = 1_700_000_000) {
  const response = await call(new Request("https://updates.example/v1/app-auth/login", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ username, password }) }), env, now);
  return { response, body: await response.clone().json() as { token: string; user: { role: string; anchorId: string | null; managedTeamIds: string[]; mustChangePassword: boolean } } };
}

describe("business account authentication", () => {
  it("does not ship a fixed default business password", async () => {
    const database = new DatabaseSync(":memory:"); database.exec(migration); databases.push(database);
    expect(database.prepare("SELECT count(*) AS count FROM app_users").get()).toEqual({ count: 0 });
  });

  it.each(["anchor", "operations_manager"] as const)("returns the server-owned %s role and scope", async (role) => {
    const { env, username } = await fixture(role);
    const { response, body } = await login(env, username);
    expect(response.status).toBe(200);
    expect(body.token).toMatch(/^[A-Za-z0-9_-]{43}$/);
    expect(body.user.role).toBe(role);
    expect(body.user.anchorId).toBe(role === "anchor" ? "anchor-a" : null);
    expect(body.user.managedTeamIds).toEqual(role === "anchor" ? [] : ["team-a"]);
    expect(body.user.mustChangePassword).toBe(true);
  });

  it("changes an initial password, keeps the current session, and supports logout", async () => {
    const { env, username } = await fixture(); const { body } = await login(env, username);
    const auth = { authorization: `Bearer ${body.token}` };
    const changed = await call(new Request("https://updates.example/v1/app-auth/password", { method: "POST", headers: { ...auth, "content-type": "application/json" }, body: JSON.stringify({ currentPassword: "Initial-Password-1", newPassword: "Changed-Password-2" }) }), env);
    expect(changed.status).toBe(200); expect((await changed.json() as { user: { mustChangePassword: boolean } }).user.mustChangePassword).toBe(false);
    expect((await call(new Request("https://updates.example/v1/app-auth/me", { headers: auth }), env)).status).toBe(200);
    expect((await call(new Request("https://updates.example/v1/app-auth/logout", { method: "POST", headers: auth }), env)).status).toBe(204);
    expect((await call(new Request("https://updates.example/v1/app-auth/me", { headers: auth }), env)).status).toBe(401);
    expect((await login(env, username, "Changed-Password-2")).response.status).toBe(200);
  });

  it("uses generic errors and locks an account after repeated failures", async () => {
    const { database, env, username } = await fixture();
    for (let attempt = 0; attempt < 5; attempt += 1) expect((await login(env, username, "Wrong-Password-1")).response.status).toBe(401);
    expect(database.prepare("SELECT locked_until FROM app_users WHERE id='user-1'").get()).toEqual({ locked_until: 1_700_000_900 });
    const locked = await login(env, username);
    expect(locked.response.status).toBe(401);
    expect(await locked.response.clone().json()).toEqual({ error: { code: "INVALID_CREDENTIALS", message: "Username or password is incorrect." } });
  });

  it("rejects disabled accounts and expired sessions", async () => {
    const { database, env, username } = await fixture(); const { body } = await login(env, username);
    const request = () => new Request("https://updates.example/v1/app-auth/me", { headers: { authorization: `Bearer ${body.token}` } });
    expect((await call(request(), env, 1_700_043_199)).status).toBe(200);
    expect((await call(request(), env, 1_700_043_200)).status).toBe(401);
    database.prepare("UPDATE app_users SET disabled_at=2 WHERE id='user-1'").run();
    expect((await call(request(), env)).status).toBe(401);
  });
});
