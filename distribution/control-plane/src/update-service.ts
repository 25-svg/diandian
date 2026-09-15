import { randomToken, sha256Hex } from "./crypto";
import type { UpdateResponse } from "./domain";
import type { ArtifactStore } from "./artifact-store";

const UPDATE_TICKET_SECONDS = 15 * 60;
const UPDATE_TOKEN_BYTES = 32;
const UPDATE_EVENT_TYPES = new Set(["check", "download_started", "download_failed", "install_succeeded", "install_failed"]);
const WINDOWS_TARGET = "windows";
const WINDOWS_ARCH = "x86_64";
const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:(?:0|[1-9]\d*|[0-9A-Za-z-]+)(?:\.(?:0|[1-9]\d*|[0-9A-Za-z-]+))*)))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;

type UpdateErrorCode = "INVALID_REQUEST" | "TICKET_INVALID" | "ARTIFACT_NOT_FOUND" | "RETRYABLE";

export class UpdateError extends Error {
  constructor(public readonly code: UpdateErrorCode, message: string) {
    super(message);
  }
}

interface ReleaseRecord {
  id: string;
  version: string;
  status: "draft" | "testing" | "production" | "halted";
  notes: string;
  pub_date: string;
  signature: string;
}

interface DownloadRecord extends ReleaseRecord {
  object_key: string;
}

interface ParsedVersion {
  core: bigint[];
  prerelease: string[] | null;
}

export function parseVersion(value: string): ParsedVersion | null {
  if (typeof value !== "string" || value.length > 256) return null;
  const match = SEMVER.exec(value);
  if (!match) return null;
  const prerelease = match[4]?.split(".") ?? null;
  if (prerelease?.some((identifier) => /^\d+$/.test(identifier) && identifier.length > 1 && identifier.startsWith("0"))) return null;
  return {
    core: [BigInt(match[1]!), BigInt(match[2]!), BigInt(match[3]!)],
    prerelease,
  };
}

function compareVersions(left: ParsedVersion, right: ParsedVersion): number {
  for (let index = 0; index < 3; index += 1) {
    if (left.core[index]! > right.core[index]!) return 1;
    if (left.core[index]! < right.core[index]!) return -1;
  }
  if (!left.prerelease && !right.prerelease) return 0;
  if (!left.prerelease) return 1;
  if (!right.prerelease) return -1;
  const count = Math.max(left.prerelease.length, right.prerelease.length);
  for (let index = 0; index < count; index += 1) {
    const a = left.prerelease[index];
    const b = right.prerelease[index];
    if (a === undefined) return -1;
    if (b === undefined) return 1;
    if (a === b) continue;
    const aNumeric = /^\d+$/.test(a);
    const bNumeric = /^\d+$/.test(b);
    if (aNumeric && bNumeric) return BigInt(a) > BigInt(b) ? 1 : -1;
    if (aNumeric) return -1;
    if (bNumeric) return 1;
    return a > b ? 1 : -1;
  }
  return 0;
}

function requireSafeRoute(target: string, arch: string): void {
  if (target !== WINDOWS_TARGET || arch !== WINDOWS_ARCH) {
    throw new UpdateError("INVALID_REQUEST", "Only windows/x86_64 updates are supported.");
  }
}

export function requireVersion(value: string): ParsedVersion {
  const parsed = parseVersion(value);
  if (!parsed) throw new UpdateError("INVALID_REQUEST", "Version must be a valid SemVer value.");
  return parsed;
}

function requireTicket(value: string): void {
  if (!/^[A-Za-z0-9_-]{43}$/.test(value)) throw new UpdateError("INVALID_REQUEST", "Download ticket is invalid.");
}

export class UpdateService {
  constructor(
    private readonly db: D1Database,
    private readonly clock: () => number = () => Math.floor(Date.now() / 1000),
    private readonly artifacts?: ArtifactStore,
  ) {}

  async resolveUpdate(input: {
    deviceId: string;
    testGroup: boolean;
    currentVersion: string;
    target: string;
    arch: string;
    origin: string;
  }): Promise<UpdateResponse | null> {
    requireSafeRoute(input.target, input.arch);
    const current = requireVersion(input.currentVersion);
    let origin: URL;
    try {
      origin = new URL(input.origin);
    } catch {
      throw new UpdateError("INVALID_REQUEST", "Request origin is invalid.");
    }
    if (origin.pathname !== "/" || origin.search || origin.hash) throw new UpdateError("INVALID_REQUEST", "Request origin is invalid.");

    const { results } = await this.db.prepare(
      `SELECT r.id, r.version, r.status, r.notes, r.pub_date, r.signature
       FROM releases r JOIN devices d ON d.id = ?
       WHERE d.status = 'active' AND r.platform = ? AND r.arch = ?
         AND (r.status = 'production' OR (r.status = 'testing' AND d.test_group = '1'))`,
    ).bind(input.deviceId, input.target, input.arch).all<ReleaseRecord>();
    let candidate: { release: ReleaseRecord; version: ParsedVersion } | null = null;
    for (const release of results) {
      const version = parseVersion(release.version);
      if (!version || compareVersions(version, current) <= 0) continue;
      if (!candidate || compareVersions(version, candidate.version) > 0) candidate = { release, version };
    }
    if (!candidate) return null;

    const ticket = randomToken(UPDATE_TOKEN_BYTES);
    const now = this.now();
    const stored = await this.db.prepare(
      `INSERT INTO download_tickets (id, token_hash, device_id, release_id, purpose, expires_at, created_at)
       SELECT ?, ?, d.id, r.id, 'update', ?, ?
       FROM devices d JOIN releases r ON r.id = ?
       WHERE d.id = ? AND d.status = 'active'
         AND (r.status = 'production' OR (r.status = 'testing' AND d.test_group = '1'))`,
    ).bind(crypto.randomUUID(), await sha256Hex(ticket), now + UPDATE_TICKET_SECONDS, now, candidate.release.id, input.deviceId).run();
    if (stored.meta.changes !== 1) return null;
    return {
      version: candidate.release.version,
      notes: candidate.release.notes,
      pub_date: candidate.release.pub_date,
      url: new URL(`/v1/download/${candidate.release.id}/${ticket}`, origin).toString(),
      signature: candidate.release.signature,
    };
  }

  async download(input: { ticket: string; releaseId: string; deviceId: string; testGroup: boolean }): Promise<Response> {
    requireTicket(input.ticket);
    if (!/^[A-Za-z0-9_-]{1,256}$/.test(input.releaseId)) throw new UpdateError("INVALID_REQUEST", "Release is invalid.");
    if (!this.artifacts) throw new Error("Artifact storage is unavailable");
    const now = this.now();
    const record = await this.db.prepare(
      `SELECT r.id, r.version, r.status, r.notes, r.pub_date, r.signature, r.object_key
       FROM download_tickets dt JOIN devices d ON d.id = dt.device_id
       JOIN releases r ON r.id = dt.release_id
       WHERE dt.token_hash = ? AND dt.device_id = ? AND dt.purpose = 'update'
         AND dt.expires_at > ? AND dt.revoked_at IS NULL AND d.status = 'active'
         AND (r.status = 'production' OR (r.status = 'testing' AND d.test_group = '1'))
         AND r.id = ?`,
    ).bind(await sha256Hex(input.ticket), input.deviceId, now, input.releaseId).first<DownloadRecord>();
    if (!record) {
      throw new UpdateError("TICKET_INVALID", "Download ticket is invalid or unavailable.");
    }
    const object = await this.artifacts.get(record.object_key);
    if (!object) throw new UpdateError("ARTIFACT_NOT_FOUND", "Update artifact is unavailable.");
    return new Response(object.body, {
      headers: {
        "cache-control": "private, no-store",
        "content-disposition": "attachment; filename=\"diandian-update.exe\"; filename*=UTF-8''diandian-update.exe",
        "content-length": String(object.size),
        "content-type": "application/octet-stream",
      },
    });
  }

  /** Initial-install links are intentionally bearerless, opaque, short-lived, and production-only. */
  async initialDownload(ticket: string): Promise<Response> {
    requireTicket(ticket);
    if (!this.artifacts) throw new Error("Artifact storage is unavailable");
    const record = await this.db.prepare(
      `SELECT r.id, r.version, r.status, r.notes, r.pub_date, r.signature, r.object_key
       FROM download_tickets dt JOIN releases r ON r.id = dt.release_id
       WHERE dt.token_hash = ? AND dt.device_id IS NULL AND dt.purpose = 'initial'
         AND dt.expires_at > ? AND dt.revoked_at IS NULL AND r.status = 'production'`,
    ).bind(await sha256Hex(ticket), this.now()).first<DownloadRecord>();
    if (!record) throw new UpdateError("TICKET_INVALID", "Download ticket is invalid or unavailable.");
    const object = await this.artifacts.get(record.object_key);
    if (!object) throw new UpdateError("ARTIFACT_NOT_FOUND", "Update artifact is unavailable.");
    const stillValid = await this.db.prepare(
      `SELECT 1 FROM download_tickets dt JOIN releases r ON r.id = dt.release_id
       WHERE dt.token_hash = ? AND dt.device_id IS NULL AND dt.purpose = 'initial'
         AND dt.expires_at > ? AND dt.revoked_at IS NULL AND r.id = ? AND r.status = 'production'`,
    ).bind(await sha256Hex(ticket), this.now(), record.id).first();
    if (!stillValid) throw new UpdateError("TICKET_INVALID", "Download ticket is invalid or unavailable.");
    return new Response(object.body, {
      headers: {
        "cache-control": "private, no-store",
        "content-disposition": "attachment; filename=\"diandian-update.exe\"; filename*=UTF-8''diandian-update.exe",
        "content-length": String(object.size),
        "content-type": "application/octet-stream",
      },
    });
  }

  async reportEvent(input: { deviceId: string; releaseId: string | null; currentVersion: string; eventType: string; clientEventId?: string }): Promise<void> {
    const eventVersion = requireVersion(input.currentVersion);
    if (!UPDATE_EVENT_TYPES.has(input.eventType) || input.deviceId.length === 0 || input.deviceId.length > 256) {
      throw new UpdateError("INVALID_REQUEST", "Update event is invalid.");
    }
    if (input.clientEventId !== undefined
      && (input.eventType !== "install_succeeded" || !/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(input.clientEventId))) {
      throw new UpdateError("INVALID_REQUEST", "Update event is invalid.");
    }
    if (input.clientEventId) {
      const existing = await this.db.prepare(
        "SELECT device_id, release_id, current_version, event_type FROM update_events WHERE id = ?",
      ).bind(input.clientEventId).first<{ device_id: string; release_id: string | null; current_version: string; event_type: string }>();
      if (existing) {
        if (existing.device_id === input.deviceId && existing.release_id === input.releaseId
          && existing.current_version === input.currentVersion && existing.event_type === input.eventType) return;
        throw new UpdateError("INVALID_REQUEST", "Update event id conflicts with an existing event.");
      }
    }
    const now = this.now();
    if (input.eventType === "check") {
      if (input.releaseId !== null) throw new UpdateError("INVALID_REQUEST", "Update event is invalid.");
      const inserted = await this.db.prepare(
        `INSERT INTO update_events (id, device_id, release_id, current_version, event_type, created_at)
         SELECT ?, d.id, NULL, ?, 'check', ? FROM devices d WHERE d.id = ? AND d.status = 'active'`,
      ).bind(crypto.randomUUID(), input.currentVersion, now, input.deviceId).run();
      if (inserted.meta.changes !== 1) throw new UpdateError("TICKET_INVALID", "Update event is unavailable.");
      return;
    }
    if (!input.releaseId || !/^[A-Za-z0-9_-]{1,256}$/.test(input.releaseId)) throw new UpdateError("INVALID_REQUEST", "Update event is invalid.");
    const eligibility = await this.eventEligibility(input.deviceId, input.releaseId);
    if (!eligibility || (input.eventType === "install_succeeded" && eligibility.version !== input.currentVersion)) {
      throw new UpdateError("TICKET_INVALID", "Update event is unavailable.");
    }
    if (input.eventType === "install_succeeded") {
      await this.recordInstallSuccess(input.deviceId, input.releaseId, input.currentVersion, eventVersion, now, input.clientEventId ?? crypto.randomUUID());
      return;
    }
    const inserted = await this.db.prepare(
      `INSERT INTO update_events (id, device_id, release_id, current_version, event_type, created_at)
       SELECT ?, d.id, r.id, ?, ?, ?
       FROM devices d JOIN releases r ON r.id = ?
       WHERE d.id = ? AND d.status = 'active' AND r.status IN ('testing', 'production', 'halted')
         AND (r.status != 'testing' OR d.test_group = '1')
         AND EXISTS (SELECT 1 FROM download_tickets dt WHERE dt.device_id = d.id AND dt.release_id = r.id AND dt.purpose = 'update')`,
    ).bind(crypto.randomUUID(), input.currentVersion, input.eventType, now, input.releaseId, input.deviceId).run();
    if (inserted.meta.changes !== 1) throw new UpdateError("TICKET_INVALID", "Update event is unavailable.");
  }

  private async eventEligibility(deviceId: string, releaseId: string): Promise<{ version: string; current_version: string | null } | null> {
    return this.db.prepare(
      `SELECT r.version, d.current_version FROM devices d JOIN releases r ON r.id = ?
       WHERE d.id = ? AND d.status = 'active' AND r.status IN ('testing', 'production', 'halted')
         AND (r.status != 'testing' OR d.test_group = '1')
         AND EXISTS (SELECT 1 FROM download_tickets dt WHERE dt.device_id = d.id AND dt.release_id = r.id AND dt.purpose = 'update')`,
    ).bind(releaseId, deviceId).first<{ version: string; current_version: string | null }>();
  }

  private async recordInstallSuccess(deviceId: string, releaseId: string, version: string, parsedVersion: ParsedVersion, now: number, eventId: string): Promise<void> {
    for (let attempt = 0; attempt < 4; attempt += 1) {
      const eligibility = await this.eventEligibility(deviceId, releaseId);
      if (!eligibility || eligibility.version !== version) throw new UpdateError("TICKET_INVALID", "Update event is unavailable.");
      const current = eligibility.current_version === null ? null : parseVersion(eligibility.current_version);
      if (current && compareVersions(parsedVersion, current) <= 0) {
        const recorded = await this.insertInstallEvent(eventId, deviceId, releaseId, version, now);
        if (recorded.meta.changes !== 1) {
          const existing = await this.installEventPayload(eventId);
          if (!existing) throw new UpdateError("TICKET_INVALID", "Update event is unavailable.");
          if (existing.device_id !== deviceId || existing.release_id !== releaseId
            || existing.current_version !== version || existing.event_type !== "install_succeeded") {
            throw new UpdateError("INVALID_REQUEST", "Update event id conflicts with an existing event.");
          }
        }
        return;
      }
      const [recorded, advanced] = await this.db.batch([
        this.installEventStatement(eventId, deviceId, releaseId, version, now, eligibility.current_version),
        this.installVersionStatement(eventId, deviceId, releaseId, version, now, eligibility.current_version),
      ]);
      if (recorded?.meta.changes === 1 && advanced?.meta.changes === 1) return;
      if (recorded?.meta.changes === 0) {
        const existing = await this.installEventPayload(eventId);
        if (existing && (existing.device_id !== deviceId || existing.release_id !== releaseId
          || existing.current_version !== version || existing.event_type !== "install_succeeded")) {
          throw new UpdateError("INVALID_REQUEST", "Update event id conflicts with an existing event.");
        }
      }
    }
    throw new UpdateError("RETRYABLE", "Update state changed; retry the request.");
  }

  private installEventStatement(eventId: string, deviceId: string, releaseId: string, version: string, now: number, expectedCurrent: string | null): D1PreparedStatement {
    return this.db.prepare(
      `INSERT OR IGNORE INTO update_events (id, device_id, release_id, current_version, event_type, created_at)
       SELECT ?, d.id, r.id, ?, 'install_succeeded', ?
       FROM devices d JOIN releases r ON r.id = ?
       WHERE d.id = ? AND d.status = 'active' AND d.current_version IS ? AND r.version = ?
         AND r.status IN ('testing', 'production', 'halted')
         AND (r.status != 'testing' OR d.test_group = '1')
         AND EXISTS (SELECT 1 FROM download_tickets dt WHERE dt.device_id = d.id AND dt.release_id = r.id AND dt.purpose = 'update')`,
    ).bind(eventId, version, now, releaseId, deviceId, expectedCurrent, version);
  }

  private installVersionStatement(eventId: string, deviceId: string, releaseId: string, version: string, now: number, expectedCurrent: string | null): D1PreparedStatement {
    return this.db.prepare(
      `UPDATE devices SET current_version = ?, last_seen_at = ?
       WHERE id = ? AND status = 'active' AND current_version IS ?
         AND EXISTS (
           SELECT 1 FROM releases r WHERE r.id = ? AND r.version = ? AND r.status IN ('testing', 'production', 'halted')
             AND (r.status != 'testing' OR devices.test_group = '1')
         )
         AND EXISTS (SELECT 1 FROM download_tickets dt WHERE dt.device_id = devices.id AND dt.release_id = ? AND dt.purpose = 'update')
         AND EXISTS (
           SELECT 1 FROM update_events ue WHERE ue.id = ? AND ue.device_id = devices.id
             AND ue.release_id = ? AND ue.current_version = ? AND ue.event_type = 'install_succeeded'
         )`,
    ).bind(version, now, deviceId, expectedCurrent, releaseId, version, releaseId, eventId, releaseId, version);
  }

  private async insertInstallEvent(eventId: string, deviceId: string, releaseId: string, version: string, now: number): Promise<D1Result> {
    return this.db.prepare(
      `INSERT OR IGNORE INTO update_events (id, device_id, release_id, current_version, event_type, created_at)
       SELECT ?, d.id, r.id, ?, 'install_succeeded', ?
       FROM devices d JOIN releases r ON r.id = ?
       WHERE d.id = ? AND d.status = 'active' AND r.version = ?
         AND r.status IN ('testing', 'production', 'halted')
         AND (r.status != 'testing' OR d.test_group = '1')
         AND EXISTS (SELECT 1 FROM download_tickets dt WHERE dt.device_id = d.id AND dt.release_id = r.id AND dt.purpose = 'update')`,
    ).bind(eventId, version, now, releaseId, deviceId, version).run();
  }

  private async installEventPayload(eventId: string): Promise<{ device_id: string; release_id: string | null; current_version: string; event_type: string } | null> {
    return this.db.prepare(
      "SELECT device_id, release_id, current_version, event_type FROM update_events WHERE id = ?",
    ).bind(eventId).first<{ device_id: string; release_id: string | null; current_version: string; event_type: string }>();
  }

  private now(): number {
    const now = this.clock();
    if (!Number.isSafeInteger(now)) throw new Error("Clock must return a safe integer timestamp");
    return now;
  }
}
