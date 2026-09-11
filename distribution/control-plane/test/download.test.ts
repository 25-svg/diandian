import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "vitest";

import worker from "../src/update-worker";
import { sha256Hex } from "../src/crypto";

const migration = readFileSync(new URL("../migrations/0001_initial.sql", import.meta.url), "utf8");
const context = { waitUntil() {}, passThroughOnException() {} } as unknown as ExecutionContext;

function makeD1(database: DatabaseSync, afterDeviceAuthentication?: () => void): D1Database {
  const statement = (sql: string, values: unknown[] = []) => ({
    sql,
    values,
    bind: (...bound: unknown[]) => statement(sql, bound),
    first: async () => {
      const result = database.prepare(sql).get(...(values as never[])) ?? null;
      if (sql === "SELECT id, status, test_group FROM devices WHERE token_hash = ?") afterDeviceAuthentication?.();
      return result;
    },
    run: async () => ({ success: true, meta: { changes: Number(database.prepare(sql).run(...(values as never[])).changes) }, results: [] }),
    all: async () => ({ success: true, meta: { changes: 0 }, results: database.prepare(sql).all(...(values as never[])) }),
  });
  return {
    prepare: (sql: string) => statement(sql) as unknown as D1PreparedStatement,
    batch: async (statements: D1PreparedStatement[]) => {
      database.exec("BEGIN IMMEDIATE");
      try {
        const results = (statements as unknown as Array<{ sql: string; values: unknown[] }>).map(({ sql, values }) => {
          const result = database.prepare(sql).run(...(values as never[]));
          return { success: true, meta: { changes: Number(result.changes) }, results: [] };
        });
        database.exec("COMMIT");
        return results as unknown as D1Result[];
      } catch (cause) {
        database.exec("ROLLBACK");
        throw cause;
      }
    },
  } as unknown as D1Database;
}

function streamObject(bytes: Uint8Array): R2ObjectBody {
  return {
    body: new ReadableStream<Uint8Array>({ start(controller) { controller.enqueue(bytes); controller.close(); } }),
    size: bytes.byteLength,
  } as unknown as R2ObjectBody;
}

describe("private update download and events", () => {
  const databases: DatabaseSync[] = [];
  afterEach(() => databases.splice(0).forEach((database) => database.close()));

  async function fixture(afterDeviceAuthentication?: () => void) {
    const database = new DatabaseSync(":memory:");
    database.exec(migration);
    databases.push(database);
    const objects = new Map<string, R2ObjectBody>();
    const env = {
      DB: makeD1(database, afterDeviceAuthentication),
      ARTIFACTS: { get: async (key: string) => objects.get(key) ?? null } as unknown as R2Bucket,
      LEASE_PRIVATE_JWK: "{}",
    };
    return { database, env, objects };
  }

  async function seedDevice(database: DatabaseSync, id: string, token: string, testGroup = false) {
    database.prepare("INSERT INTO devices (id, fingerprint_hash, token_hash, status, test_group, activated_at, created_at) VALUES (?, ?, ?, 'active', ?, 1, 1)")
      .run(id, `fp-${id}`, await sha256Hex(token), testGroup ? "1" : null);
  }

  function seedRelease(database: DatabaseSync, status = "production", id = "release-1", version = "2.22.0") {
    database.prepare(`INSERT INTO releases
      (id, version, status, notes, pub_date, object_key, size, sha256, platform, arch, signature, created_at, updated_at)
      VALUES (?, ?, ?, 'notes', '2026-09-11T00:00:00Z', 'private/release.exe', 3, 'sha', 'windows', 'x86_64', 'sig', 1, 1)`)
      .run(id, version, status);
  }

  function seedHistoryTicket(database: DatabaseSync, id: string, deviceId: string, releaseId: string) {
    database.prepare("INSERT INTO download_tickets (id, token_hash, device_id, release_id, purpose, expires_at, created_at) VALUES (?, ?, ?, ?, 'update', 1, 1)")
      .run(id, `hash-${id}`, deviceId, releaseId);
  }

  async function update(env: Record<string, unknown>, token: string, currentVersion = "2.21.0") {
    return worker.fetch(new Request(`https://control.example/v1/update/windows/x86_64/${encodeURIComponent(currentVersion)}`, { headers: { authorization: `Bearer ${token}` } }), env as never, context);
  }

  it("streams a ticket-bound private artifact with non-public safe download headers", async () => {
    const { database, env, objects } = await fixture();
    const token = "a".repeat(43);
    await seedDevice(database, "device-1", token);
    seedRelease(database);
    objects.set("private/release.exe", streamObject(new TextEncoder().encode("abc")));

    const check = await update(env, token);
    const body = await check.json() as { url: string };
    const ticket = new URL(body.url).pathname.split("/").at(-1)!;
    const download = await worker.fetch(new Request(`https://control.example/v1/download/${ticket}`, { headers: { authorization: `Bearer ${token}` } }), env, context);

    expect(check.status).toBe(200);
    expect(download.status).toBe(200);
    expect(await download.text()).toBe("abc");
    expect(download.headers.get("content-length")).toBe("3");
    expect(download.headers.get("content-type")).toBe("application/octet-stream");
    expect(download.headers.get("cache-control")).toBe("private, no-store");
    expect(download.headers.get("content-disposition")).toMatch(/^attachment; filename="diandian-update\.exe"; filename\*=UTF-8''diandian-update\.exe$/);
    expect(download.headers.get("content-disposition")).not.toContain("private/release.exe");
  });

  it("rejects expired, revoked, cross-device, halted, and missing-object update downloads", async () => {
    const { database, env, objects } = await fixture();
    const token1 = "a".repeat(43);
    const token2 = "b".repeat(43);
    await seedDevice(database, "device-1", token1);
    await seedDevice(database, "device-2", token2);
    seedRelease(database);
    objects.set("private/release.exe", streamObject(new TextEncoder().encode("abc")));
    const normal = await update(env, token1);
    const validTicket = new URL((await normal.json() as { url: string }).url).pathname.split("/").at(-1)!;
    const ticketHash = await sha256Hex(validTicket);

    database.prepare("UPDATE download_tickets SET expires_at = 1 WHERE token_hash = ?").run(ticketHash);
    expect((await worker.fetch(new Request(`https://control.example/v1/download/${validTicket}`, { headers: { authorization: `Bearer ${token1}` } }), env, context)).status).toBe(403);
    database.prepare("UPDATE download_tickets SET expires_at = 4102444800, revoked_at = NULL WHERE token_hash = ?").run(ticketHash);
    expect((await worker.fetch(new Request(`https://control.example/v1/download/${validTicket}`, { headers: { authorization: `Bearer ${token2}` } }), env, context)).status).toBe(403);
    database.prepare("UPDATE download_tickets SET revoked_at = 2 WHERE token_hash = ?").run(ticketHash);
    expect((await worker.fetch(new Request(`https://control.example/v1/download/${validTicket}`, { headers: { authorization: `Bearer ${token1}` } }), env, context)).status).toBe(403);
    database.prepare("UPDATE download_tickets SET revoked_at = NULL WHERE token_hash = ?").run(ticketHash);
    database.prepare("UPDATE releases SET status = 'halted'").run();
    expect((await worker.fetch(new Request(`https://control.example/v1/download/${validTicket}`, { headers: { authorization: `Bearer ${token1}` } }), env, context)).status).toBe(403);
    database.prepare("UPDATE releases SET status = 'production'").run();
    objects.clear();
    expect((await worker.fetch(new Request(`https://control.example/v1/download/${validTicket}`, { headers: { authorization: `Bearer ${token1}` } }), env, context)).status).toBe(404);
  });

  it("rejects malformed update paths and records only authenticated allowlisted update events", async () => {
    const { database, env } = await fixture();
    const token = "a".repeat(43);
    await seedDevice(database, "device-1", token);
    seedRelease(database);
    seedHistoryTicket(database, "ticket-device-1-event", "device-1", "release-1");

    expect((await worker.fetch(new Request("https://control.example/v1/update/windows/x86_64/%ZZ", { headers: { authorization: `Bearer ${token}` } }), env, context)).status).toBe(400);
    expect((await worker.fetch(new Request("https://control.example/v1/update/windows/x86_64/2.21.0", { headers: { authorization: "Bearer short" } }), env, context)).status).toBe(401);

    const forged = await worker.fetch(new Request("https://control.example/v1/update-events", {
      method: "POST", headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
      body: JSON.stringify({ deviceId: "device-2", releaseId: "release-1", currentVersion: "2.22.0", eventType: "install_succeeded" }),
    }), env, context);
    const accepted = await worker.fetch(new Request("https://control.example/v1/update-events", {
      method: "POST", headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
      body: JSON.stringify({ releaseId: "release-1", currentVersion: "2.22.0", eventType: "install_succeeded" }),
    }), env, context);
    const invalidEvent = await worker.fetch(new Request("https://control.example/v1/update-events", {
      method: "POST", headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
      body: JSON.stringify({ currentVersion: "2.22.0", eventType: "anything" }),
    }), env, context);

    expect(forged.status).toBe(400);
    expect(accepted.status).toBe(204);
    expect(invalidEvent.status).toBe(400);
    expect(database.prepare("SELECT device_id, release_id, current_version, event_type FROM update_events").get()).toEqual({ device_id: "device-1", release_id: "release-1", current_version: "2.22.0", event_type: "install_succeeded" });
    expect(database.prepare("SELECT current_version FROM devices WHERE id = 'device-1'").get()).toEqual({ current_version: "2.22.0" });
  });

  it("requires issued device-specific tickets for result events while preserving halted history", async () => {
    const { database, env } = await fixture();
    const token1 = "a".repeat(43);
    const token2 = "b".repeat(43);
    await seedDevice(database, "device-1", token1);
    await seedDevice(database, "device-2", token2);
    seedRelease(database);
    const post = (body: unknown) => worker.fetch(new Request("https://control.example/v1/update-events", {
      method: "POST", headers: { authorization: `Bearer ${token1}`, "content-type": "application/json" }, body: JSON.stringify(body),
    }), env, context);

    expect((await post({ currentVersion: "2.21.0", eventType: "check", releaseId: null })).status).toBe(204);
    expect((await post({ currentVersion: "2.21.0", eventType: "download_started", releaseId: null })).status).toBe(400);
    expect((await post({ currentVersion: "2.22.0", eventType: "install_succeeded", releaseId: "release-1" })).status).toBe(403);
    seedHistoryTicket(database, "ticket-device-2", "device-2", "release-1");
    expect((await post({ currentVersion: "2.22.0", eventType: "install_succeeded", releaseId: "release-1" })).status).toBe(403);
    seedHistoryTicket(database, "ticket-device-1", "device-1", "release-1");
    database.prepare("UPDATE releases SET status = 'halted' WHERE id = 'release-1'").run();
    expect((await post({ currentVersion: "2.22.0", eventType: "install_succeeded", releaseId: "release-1" })).status).toBe(204);
    database.prepare("UPDATE releases SET status = 'draft' WHERE id = 'release-1'").run();
    expect((await post({ currentVersion: "2.22.0", eventType: "install_succeeded", releaseId: "release-1" })).status).toBe(403);
    database.prepare("UPDATE releases SET status = 'testing' WHERE id = 'release-1'").run();
    expect((await post({ currentVersion: "2.22.0", eventType: "install_succeeded", releaseId: "release-1" })).status).toBe(403);
  });

  it("keeps device current_version monotonic when installation success events arrive out of order", async () => {
    const { database, env } = await fixture();
    const token = "a".repeat(43);
    await seedDevice(database, "device-1", token);
    seedRelease(database, "production", "release-22", "2.22.0");
    seedRelease(database, "production", "release-23", "2.23.0");
    seedHistoryTicket(database, "ticket-22", "device-1", "release-22");
    seedHistoryTicket(database, "ticket-23", "device-1", "release-23");
    const installed = (releaseId: string, currentVersion: string) => worker.fetch(new Request("https://control.example/v1/update-events", {
      method: "POST", headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
      body: JSON.stringify({ releaseId, currentVersion, eventType: "install_succeeded" }),
    }), env, context);

    expect((await installed("release-23", "2.23.0")).status).toBe(204);
    expect((await installed("release-22", "2.22.0")).status).toBe(204);
    expect((await installed("release-23", "2.23.0")).status).toBe(204);
    expect(database.prepare("SELECT current_version FROM devices WHERE id = 'device-1'").get()).toEqual({ current_version: "2.23.0" });
  });

  it("rechecks authorization after the initial Bearer lookup before update, download, and event writes", async () => {
    let database: DatabaseSync;
    const fixtureResult = await fixture(() => database.prepare("UPDATE devices SET status = 'revoked'").run());
    ({ database } = fixtureResult);
    const token = "a".repeat(43);
    await seedDevice(database, "device-1", token);
    seedRelease(database);
    seedHistoryTicket(database, "ticket-device-1", "device-1", "release-1");
    const ticket = "t".repeat(43);
    database.prepare("UPDATE download_tickets SET token_hash = ?, expires_at = 4102444800 WHERE id = 'ticket-device-1'").run(await sha256Hex(ticket));

    const updateResponse = await update(fixtureResult.env, token);
    database.prepare("UPDATE devices SET status = 'active'").run();
    const downloadResponse = await worker.fetch(new Request(`https://control.example/v1/download/${ticket}`, { headers: { authorization: `Bearer ${token}` } }), fixtureResult.env, context);
    database.prepare("UPDATE devices SET status = 'active'").run();
    const eventResponse = await worker.fetch(new Request("https://control.example/v1/update-events", {
      method: "POST", headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
      body: JSON.stringify({ currentVersion: "2.21.0", eventType: "check", releaseId: null }),
    }), fixtureResult.env, context);

    expect(updateResponse.status).toBe(204);
    expect(downloadResponse.status).toBe(403);
    expect(eventResponse.status).toBe(403);
    expect(database.prepare("SELECT count(*) AS count FROM update_events").get()).toEqual({ count: 0 });
  });

  it("rechecks testing membership after the initial Bearer lookup before issuing a testing ticket", async () => {
    let database: DatabaseSync;
    const fixtureResult = await fixture(() => database.prepare("UPDATE devices SET test_group = NULL").run());
    ({ database } = fixtureResult);
    const token = "a".repeat(43);
    await seedDevice(database, "device-1", token, true);
    seedRelease(database, "testing");

    const response = await update(fixtureResult.env, token);

    expect(response.status).toBe(204);
    expect(database.prepare("SELECT count(*) AS count FROM download_tickets").get()).toEqual({ count: 0 });
  });
});
