import { DatabaseSync } from "node:sqlite";
import { readFileSync } from "node:fs";
import { afterEach, describe, expect, it } from "vitest";

import { UpdateService } from "../src/update-service";

const migration = readFileSync(new URL("../migrations/0001_initial.sql", import.meta.url), "utf8");

function makeD1(database: DatabaseSync): D1Database {
  const statement = (sql: string, values: unknown[] = []) => ({
    bind: (...bound: unknown[]) => statement(sql, bound),
    first: async () => database.prepare(sql).get(...(values as never[])) ?? null,
    run: async () => ({ success: true, meta: { changes: Number(database.prepare(sql).run(...(values as never[])).changes) }, results: [] }),
    all: async () => ({ success: true, meta: { changes: 0 }, results: database.prepare(sql).all(...(values as never[])) }),
  });
  return { prepare: (sql: string) => statement(sql) as unknown as D1PreparedStatement } as D1Database;
}

describe("UpdateService.resolveUpdate", () => {
  const databases: DatabaseSync[] = [];
  afterEach(() => databases.splice(0).forEach((database) => database.close()));

  function fixture() {
    const database = new DatabaseSync(":memory:");
    database.exec(migration);
    database.prepare("INSERT INTO devices (id, fingerprint_hash, token_hash, status, activated_at, created_at) VALUES ('device-1', 'fp-1', 'token-1', 'active', 1, 1)").run();
    databases.push(database);
    return { database, service: new UpdateService(makeD1(database), () => 1_700_000_000) };
  }

  function release(database: DatabaseSync, id: string, version: string, status: string) {
    database.prepare(`INSERT INTO releases
      (id, version, status, notes, pub_date, object_key, size, sha256, platform, arch, signature, created_at, updated_at)
      VALUES (?, ?, ?, ?, ?, ?, ?, ?, 'windows', 'x86_64', ?, 1, 1)`)
      .run(id, version, status, `Notes for ${version}`, "2026-09-11T00:00:00Z", `private/${id}.exe`, 3, "sha", `signature-${id}`);
  }

  it.each([
    ["testing", false, true],
    ["testing", true, false],
    ["production", false, false],
    ["halted", true, true],
    ["draft", true, true],
  ])("keeps %s releases hidden according to test-group visibility", async (status, testGroup, hidden) => {
    const { database, service } = fixture();
    release(database, "release-1", "2.22.0", status);

    const result = await service.resolveUpdate({
      deviceId: "device-1", testGroup, currentVersion: "2.21.0", target: "windows", arch: "x86_64", origin: "https://control.example",
    });

    expect(result === null).toBe(hidden);
    if (!hidden) expect(result).toMatchObject({ version: "2.22.0", notes: "Notes for 2.22.0", pub_date: "2026-09-11T00:00:00Z", signature: "signature-release-1" });
  });

  it("chooses the numerically highest visible SemVer and never returns equal, lower, or invalid versions", async () => {
    const { database, service } = fixture();
    release(database, "v-9", "2.9.0", "production");
    release(database, "v-10", "2.10.0", "production");
    release(database, "v-pre", "2.11.0-beta.2", "production");
    release(database, "v-final", "2.11.0", "production");
    release(database, "v-bad", "not-a-version", "production");

    await expect(service.resolveUpdate({ deviceId: "device-1", testGroup: false, currentVersion: "bad", target: "windows", arch: "x86_64", origin: "https://control.example" })).rejects.toMatchObject({ code: "INVALID_REQUEST" });
    await expect(service.resolveUpdate({ deviceId: "device-1", testGroup: false, currentVersion: "2.10.0-01", target: "windows", arch: "x86_64", origin: "https://control.example" })).rejects.toMatchObject({ code: "INVALID_REQUEST" });
    await expect(service.resolveUpdate({ deviceId: "device-1", testGroup: false, currentVersion: "2.11.0", target: "windows", arch: "x86_64", origin: "https://control.example" })).resolves.toBeNull();
    await expect(service.resolveUpdate({ deviceId: "device-1", testGroup: false, currentVersion: "2.12.0", target: "windows", arch: "x86_64", origin: "https://control.example" })).resolves.toBeNull();

    await expect(service.resolveUpdate({ deviceId: "device-1", testGroup: false, currentVersion: "2.9.0", target: "windows", arch: "x86_64", origin: "https://control.example" }))
      .resolves.toMatchObject({ version: "2.11.0", signature: "signature-v-final" });
  });

  it("issues a short-lived opaque ticket bound to the requesting device and release", async () => {
    const { database, service } = fixture();
    release(database, "release-1", "2.22.0", "production");

    const result = await service.resolveUpdate({
      deviceId: "device-1", testGroup: false, currentVersion: "2.21.0", target: "windows", arch: "x86_64", origin: "https://control.example",
    });
    const ticket = new URL(result!.url).pathname.split("/").at(-1)!;
    const stored = database.prepare("SELECT device_id, release_id, expires_at, token_hash FROM download_tickets").get() as Record<string, unknown>;

    expect(result!.url).toMatch(/^https:\/\/control\.example\/v1\/download\/[A-Za-z0-9_-]{43}$/);
    expect(stored).toMatchObject({ device_id: "device-1", release_id: "release-1", expires_at: 1_700_000_900 });
    expect(stored.token_hash).toMatch(/^[a-f0-9]{64}$/);
    expect(stored.token_hash).not.toBe(ticket);
  });

  it("rejects non-Windows route contracts before looking up releases", async () => {
    const { service } = fixture();
    await expect(service.resolveUpdate({ deviceId: "device-1", testGroup: false, currentVersion: "2.21.0", target: "darwin", arch: "arm64", origin: "https://control.example" }))
      .rejects.toMatchObject({ code: "INVALID_REQUEST" });
  });
});
