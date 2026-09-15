import { readFileSync } from "node:fs";
import { DatabaseSync } from "node:sqlite";
import { afterEach, describe, expect, it } from "vitest";

const migration = ["0001_initial.sql", "0002_app_auth.sql", "0003_local_admin_auth.sql"].map((name) => readFileSync(new URL(`../migrations/${name}`, import.meta.url), "utf8")).join("\n");
const requiredTables = ["admins", "devices", "activation_codes", "releases", "download_tickets", "update_events", "audit_logs", "app_users", "app_user_sessions", "admin_sessions"];

describe("initial D1 migration", () => {
  const databases: DatabaseSync[] = [];
  afterEach(() => databases.splice(0).forEach((database) => database.close()));

  function createDatabase() {
    const database = new DatabaseSync(":memory:");
    database.exec(migration);
    databases.push(database);
    return database;
  }

  it("creates the required tables, hash-only credentials, checks, and indexes", () => {
    const database = createDatabase();
    const tables = database.prepare("SELECT name FROM sqlite_master WHERE type = 'table'").all().map(({ name }) => name);
    const indexes = database.prepare("SELECT name FROM sqlite_master WHERE type = 'index'").all().map(({ name }) => name);
    expect(tables).toEqual(expect.arrayContaining(requiredTables));
    expect(indexes).toEqual(expect.arrayContaining(["idx_devices_token_hash", "idx_activation_codes_expires_at", "idx_releases_status_platform_arch_version", "idx_download_tickets_token_hash", "idx_download_tickets_expires_at"]));

    for (const table of requiredTables) {
      const columns = database.prepare(`PRAGMA table_info(${table})`).all().map(({ name }) => name);
      expect(columns).not.toContain("token");
      expect(columns).not.toContain("code");
    }
    const activationColumns = database.prepare("PRAGMA table_info(activation_codes)").all().map(({ name }) => name);
    expect(activationColumns).toEqual(expect.arrayContaining(["code_hash", "status", "used_by_device_id", "used_at"]));
    expect(database.prepare("SELECT sql FROM sqlite_master WHERE name = 'devices'").get()).toHaveProperty("sql", expect.stringContaining("CHECK"));
    expect(database.prepare("PRAGMA table_info(admins)").all().map(({ name }) => name)).toContain("disabled_at");
    expect(indexes).toEqual(expect.arrayContaining(["idx_app_users_role_active", "idx_app_sessions_token", "idx_app_sessions_user", "idx_admin_sessions_token", "idx_admin_sessions_admin"]));
  });

  it("enforces one activation code for one device and ticket purpose binding", () => {
    const database = createDatabase();
    database.prepare("INSERT INTO devices (id, fingerprint_hash, token_hash, status, activated_at, created_at) VALUES (?, ?, ?, ?, ?, ?)").run("device-1", "fingerprint-1", "token-1", "active", 1, 1);
    database.prepare("INSERT INTO releases (id, version, status, notes, pub_date, object_key, size, sha256, platform, arch, signature, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)").run("release-1", "1.0.0", "production", "", "2026-09-11", "releases/1.0.0.zip", 10, "sha", "windows", "x64", "signature", 1, 1);
    database.prepare("INSERT INTO activation_codes (id, code_hash, status, used_by_device_id, used_at, expires_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)").run("code-1", "code-hash-1", "used", "device-1", 2, 3, 1);
    expect(() => database.prepare("INSERT INTO activation_codes (id, code_hash, status, used_by_device_id, used_at, expires_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)").run("code-2", "code-hash-2", "used", null, null, 3, 1)).toThrow();

    database.prepare("INSERT INTO download_tickets (id, token_hash, device_id, release_id, purpose, expires_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)").run("ticket-initial", "ticket-hash-1", null, "release-1", "initial", 3, 1);
    expect(() => database.prepare("INSERT INTO download_tickets (id, token_hash, device_id, release_id, purpose, expires_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)").run("ticket-update", "ticket-hash-2", null, "release-1", "update", 3, 1)).toThrow();
    expect(() => database.prepare("INSERT INTO download_tickets (id, token_hash, device_id, release_id, purpose, expires_at, created_at) VALUES (?, ?, ?, ?, ?, ?, ?)").run("ticket-foreign-key", "ticket-hash-3", "missing-device", "release-1", "update", 3, 1)).toThrow();
  });

  it("allows a version for different artifacts but rejects duplicate artifact targets", () => {
    const database = createDatabase();
    const insertRelease = database.prepare("INSERT INTO releases (id, version, status, notes, pub_date, object_key, size, sha256, platform, arch, signature, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)");

    insertRelease.run("release-windows", "1.0.0", "production", "", "2026-09-11", "releases/windows.zip", 10, "windows-sha", "windows", "x64", "signature", 1, 1);
    expect(() => insertRelease.run("release-macos", "1.0.0", "production", "", "2026-09-11", "releases/macos.zip", 10, "macos-sha", "darwin", "arm64", "signature", 1, 1)).not.toThrow();
    expect(() => insertRelease.run("release-windows-duplicate", "1.0.0", "production", "", "2026-09-11", "releases/windows-duplicate.zip", 10, "windows-sha-2", "windows", "x64", "signature", 1, 1)).toThrow();
  });
});
