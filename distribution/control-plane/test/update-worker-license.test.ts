import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it, vi } from "vitest";

import worker from "../src/update-worker";
import { sha256Hex } from "../src/crypto";

const migration = readFileSync(new URL("../migrations/0001_initial.sql", import.meta.url), "utf8");

type Statement = { sql: string; values: unknown[] };

function makeD1(database: DatabaseSync): D1Database {
  const statement = (sql: string, values: unknown[] = []) => ({
    sql,
    values,
    bind: (...bound: unknown[]) => statement(sql, bound),
    first: async () => database.prepare(sql).get(...(values as never[])) ?? null,
    run: async () => ({ success: true, meta: { changes: Number(database.prepare(sql).run(...(values as never[])).changes) }, results: [] }),
    all: async () => ({ success: true, meta: { changes: 0 }, results: database.prepare(sql).all(...(values as never[])) }),
  });
  return {
    prepare: (sql: string) => statement(sql) as unknown as D1PreparedStatement,
    batch: async (statements: D1PreparedStatement[]) => {
      database.exec("BEGIN IMMEDIATE");
      try {
        const results = (statements as unknown as Statement[]).map(({ sql, values }) => {
          const result = database.prepare(sql).run(...(values as never[]));
          return { success: true, meta: { changes: Number(result.changes) }, results: [] };
        });
        database.exec("COMMIT");
        return results as unknown as D1Result[];
      } catch (error) {
        database.exec("ROLLBACK");
        throw error;
      }
    },
  } as unknown as D1Database;
}

async function privateJwk(): Promise<string> {
  const pair = await crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, true, ["sign", "verify"]);
  return JSON.stringify(await crypto.subtle.exportKey("jwk", pair.privateKey));
}

describe("license worker endpoints", () => {
  const databases: DatabaseSync[] = [];
  afterEach(() => databases.splice(0).forEach((database) => database.close()));

  async function createFixture() {
    const database = new DatabaseSync(":memory:");
    database.exec(migration);
    databases.push(database);
    const now = Math.floor(Date.now() / 1000);
    const env = { DB: makeD1(database), ARTIFACTS: {} as R2Bucket, LEASE_PRIVATE_JWK: await privateJwk() };
    return { database, env, now };
  }

  async function seedCode(database: DatabaseSync, code: string, expiresAt: number) {
    database.prepare("INSERT INTO activation_codes (id, code_hash, status, expires_at, created_at) VALUES (?, ?, 'unused', ?, ?)")
      .run(crypto.randomUUID(), await sha256Hex(code), expiresAt, expiresAt - 10);
  }

  it("activates once and returns stable token-free renewal errors without logging credentials", async () => {
    const { database, env, now } = await createFixture();
    await seedCode(database, "CODE-SECRET", now + 60);
    const logs = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const request = new Request("https://control.example/v1/activate", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ code: "CODE-SECRET", installId: "install-a", label: "主播A" }),
    });

    const activation = await worker.fetch(request, env, { waitUntil() {}, passThroughOnException() {} } as unknown as ExecutionContext);
    const activationBody = await activation.json() as { deviceToken: string; lease: string };
    const renewal = await worker.fetch(new Request("https://control.example/v1/lease/renew", {
      method: "POST", headers: { authorization: `Bearer ${activationBody.deviceToken}` },
    }), env, {} as ExecutionContext);
    const malformed = await worker.fetch(new Request("https://control.example/v1/lease/renew", {
      method: "POST", headers: { authorization: `Basic ${activationBody.deviceToken}` },
    }), env, {} as ExecutionContext);

    expect(activation.status).toBe(201);
    expect(activationBody.deviceToken).toMatch(/^[A-Za-z0-9_-]{43}$/);
    expect(renewal.status).toBe(200);
    expect(JSON.stringify(await renewal.json())).not.toContain(activationBody.deviceToken);
    expect(malformed.status).toBe(401);
    expect(await malformed.json()).toEqual({ error: { code: "DEVICE_TOKEN_INVALID", message: "A valid Bearer device token is required." } });
    expect(JSON.stringify(logs.mock.calls)).not.toContain("CODE-SECRET");
    expect(JSON.stringify(logs.mock.calls)).not.toContain(activationBody.deviceToken);
    logs.mockRestore();
  });

  it("returns revoked status but refuses lease renewal", async () => {
    const { database, env, now } = await createFixture();
    await seedCode(database, "CODE", now + 60);
    const activation = await worker.fetch(new Request("https://control.example/v1/activate", {
      method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ code: "CODE", installId: "install-a", label: "A" }),
    }), env, {} as ExecutionContext);
    const { deviceToken } = await activation.json() as { deviceToken: string };
    database.prepare("UPDATE devices SET status = 'revoked', revoked_at = ?").run(now);

    const status = await worker.fetch(new Request("https://control.example/v1/license/status", {
      headers: { authorization: `Bearer ${deviceToken}` },
    }), env, {} as ExecutionContext);
    const renewal = await worker.fetch(new Request("https://control.example/v1/lease/renew", {
      method: "POST", headers: { authorization: `Bearer ${deviceToken}` },
    }), env, {} as ExecutionContext);

    expect(await status.json()).toMatchObject({ status: "revoked" });
    expect(renewal.status).toBe(403);
    expect(await renewal.json()).toEqual({ error: { code: "DEVICE_REVOKED", message: "This device has been revoked." } });
  });
});
