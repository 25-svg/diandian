export interface DeviceRecord {
  id: string;
  status: "active" | "revoked";
  test_group: string | null;
}

interface ActivationCodeRecord {
  status: "unused" | "used" | "revoked";
  expires_at: number;
}

export interface ActivationClaimInput {
  codeHash: string;
  installHash: string;
  tokenHash: string;
  deviceId: string;
  label: string;
  now: number;
}

/** Persists only hashes. `batch` makes the guarded insert and code claim atomic in D1. */
export class LicenseRepository {
  constructor(private readonly db: D1Database) {}

  async claimActivation(input: ActivationClaimInput): Promise<boolean> {
    const statements = [
      this.db.prepare(
        `INSERT INTO devices (id, fingerprint_hash, token_hash, status, streamer_note, activated_at, created_at)
         SELECT ?, ?, ?, 'active', ?, ?, ?
         WHERE EXISTS (
           SELECT 1 FROM activation_codes
           WHERE code_hash = ? AND status = 'unused' AND expires_at > ?
         )
         AND NOT EXISTS (SELECT 1 FROM devices WHERE fingerprint_hash = ?)`,
      ).bind(
        input.deviceId,
        input.installHash,
        input.tokenHash,
        input.label,
        input.now,
        input.now,
        input.codeHash,
        input.now,
        input.installHash,
      ),
      this.db.prepare(
        `UPDATE activation_codes
         SET status = 'used', used_by_device_id = ?, used_at = ?
         WHERE code_hash = ? AND status = 'unused' AND expires_at > ?
           AND EXISTS (SELECT 1 FROM devices WHERE id = ? AND fingerprint_hash = ?)`,
      ).bind(input.deviceId, input.now, input.codeHash, input.now, input.deviceId, input.installHash),
    ];
    const results = await this.db.batch(statements);
    return results[1]?.meta.changes === 1;
  }

  async findDeviceByTokenHash(tokenHash: string): Promise<DeviceRecord | null> {
    return this.db.prepare("SELECT id, status, test_group FROM devices WHERE token_hash = ?").bind(tokenHash).first<DeviceRecord>();
  }

  async findDeviceByInstallHash(installHash: string): Promise<DeviceRecord | null> {
    return this.db.prepare("SELECT id, status FROM devices WHERE fingerprint_hash = ?").bind(installHash).first<DeviceRecord>();
  }

  async findActivationCode(codeHash: string): Promise<ActivationCodeRecord | null> {
    return this.db.prepare("SELECT status, expires_at FROM activation_codes WHERE code_hash = ?").bind(codeHash).first<ActivationCodeRecord>();
  }

  async revokeDevice(deviceId: string, revokedAt = Math.floor(Date.now() / 1000)): Promise<void> {
    await this.db.prepare("UPDATE devices SET status = 'revoked', revoked_at = ? WHERE id = ?").bind(revokedAt, deviceId).run();
  }
}
