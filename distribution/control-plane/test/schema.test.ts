import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("initial D1 migration", () => {
  it("creates the private control-plane tables without plaintext credentials", () => {
    const migration = readFileSync(
      new URL("../migrations/0001_initial.sql", import.meta.url),
      "utf8"
    );
    const requiredTables = [
      "admins",
      "devices",
      "activation_codes",
      "releases",
      "download_tickets",
      "update_events",
      "audit_logs"
    ];

    for (const table of requiredTables) {
      expect(migration).toContain(`CREATE TABLE ${table}`);
    }
    expect(migration).not.toMatch(/\b(token|code)\s+TEXT\b/);
    expect(migration).toContain("token_hash TEXT");
    expect(migration).toContain("code_hash TEXT");
  });
});
