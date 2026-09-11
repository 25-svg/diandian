import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "vitest";

import { sha256Hex } from "../src/crypto";
import { LicenseService } from "../src/license-service";
import { LicenseRepository } from "../src/repository";

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

async function keyPair(): Promise<JsonWebKey> {
  const pair = await crypto.subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, true, ["sign", "verify"]);
  return crypto.subtle.exportKey("jwk", pair.privateKey);
}

describe("LicenseService", () => {
  const databases: DatabaseSync[] = [];
  afterEach(() => databases.splice(0).forEach((database) => database.close()));

  async function createFixture() {
    const database = new DatabaseSync(":memory:");
    database.exec(migration);
    databases.push(database);
    const repository = new LicenseRepository(makeD1(database));
    const now = 1_700_000_000;
    const service = new LicenseService(repository, await keyPair(), () => now);
    return { database, repository, service, now };
  }

  async function seedCode(database: DatabaseSync, code: string, expiresAt: number) {
    database.prepare("INSERT INTO activation_codes (id, code_hash, status, expires_at, created_at) VALUES (?, ?, 'unused', ?, ?)")
      .run(crypto.randomUUID(), await sha256Hex(code), expiresAt, expiresAt - 10);
  }

  it("consumes one activation code once", async () => {
    const { database, service, now } = await createFixture();
    await seedCode(database, "CODE", now + 60);

    const first = await service.activate({ code: "CODE", installId: "install-a", label: "主播A" });
    expect(first.deviceToken).toMatch(/^[A-Za-z0-9_-]{43}$/);
    await expect(service.activate({ code: "CODE", installId: "install-b", label: "主播B" }))
      .rejects.toMatchObject({ code: "ACTIVATION_CODE_USED" });
    expect(database.prepare("SELECT COUNT(*) AS count FROM devices").get()).toMatchObject({ count: 1 });
  });

  it("atomically permits only one concurrent claim without an orphan device", async () => {
    const { database, service, now } = await createFixture();
    await seedCode(database, "RACE", now + 60);

    const results = await Promise.allSettled([
      service.activate({ code: "RACE", installId: "install-a", label: "A" }),
      service.activate({ code: "RACE", installId: "install-b", label: "B" }),
    ]);

    expect(results.filter((result) => result.status === "fulfilled")).toHaveLength(1);
    expect(results.filter((result) => result.status === "rejected")).toHaveLength(1);
    expect(database.prepare("SELECT COUNT(*) AS count FROM devices").get()).toMatchObject({ count: 1 });
    expect(database.prepare("SELECT status, used_by_device_id FROM activation_codes").get()).toMatchObject({ status: "used" });
  });

  it("does not consume a second code for an existing installation", async () => {
    const { database, service, now } = await createFixture();
    await seedCode(database, "ONE", now + 60);
    await seedCode(database, "TWO", now + 60);
    await service.activate({ code: "ONE", installId: "install-a", label: "A" });

    await expect(service.activate({ code: "TWO", installId: "install-a", label: "A" }))
      .rejects.toMatchObject({ code: "INSTALLATION_ALREADY_ACTIVATED" });
    expect(database.prepare("SELECT status FROM activation_codes WHERE code_hash = ?").get(await sha256Hex("TWO"))).toMatchObject({ status: "unused" });
  });

  it("rejects a revoked device immediately while status reports the business state", async () => {
    const { database, repository, service, now } = await createFixture();
    await seedCode(database, "CODE", now + 60);
    const { deviceToken } = await service.activate({ code: "CODE", installId: "install-a", label: "主播A" });
    const device = await repository.findDeviceByTokenHash(await sha256Hex(deviceToken));
    await repository.revokeDevice(device!.id, now + 1);

    await expect(service.renew(deviceToken)).rejects.toMatchObject({ code: "DEVICE_REVOKED" });
    await expect(service.status(deviceToken)).resolves.toEqual({ status: "revoked", serverTime: now });
  });
});
