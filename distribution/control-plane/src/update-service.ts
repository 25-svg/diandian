import { randomToken, sha256Hex } from "./crypto";
import type { UpdateResponse } from "./domain";

const UPDATE_TICKET_SECONDS = 15 * 60;
const UPDATE_TOKEN_BYTES = 32;
const UPDATE_EVENT_TYPES = new Set(["check", "download_started", "download_failed", "install_succeeded", "install_failed"]);
const WINDOWS_TARGET = "windows";
const WINDOWS_ARCH = "x86_64";
const SEMVER = /^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)(?:-((?:(?:0|[1-9]\d*|[0-9A-Za-z-]+)(?:\.(?:0|[1-9]\d*|[0-9A-Za-z-]+))*)))?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;

type UpdateErrorCode = "INVALID_REQUEST" | "TICKET_INVALID" | "ARTIFACT_NOT_FOUND";

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

function parseVersion(value: string): ParsedVersion | null {
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

function visibleToDevice(status: ReleaseRecord["status"], testGroup: boolean): boolean {
  return status === "production" || (status === "testing" && testGroup);
}

function requireSafeRoute(target: string, arch: string): void {
  if (target !== WINDOWS_TARGET || arch !== WINDOWS_ARCH) {
    throw new UpdateError("INVALID_REQUEST", "Only windows/x86_64 updates are supported.");
  }
}

function requireVersion(value: string): ParsedVersion {
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
    private readonly artifacts?: R2Bucket,
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
      "SELECT id, version, status, notes, pub_date, signature FROM releases WHERE platform = ? AND arch = ? AND status IN ('testing', 'production')",
    ).bind(input.target, input.arch).all<ReleaseRecord>();
    let candidate: { release: ReleaseRecord; version: ParsedVersion } | null = null;
    for (const release of results) {
      if (!visibleToDevice(release.status, input.testGroup)) continue;
      const version = parseVersion(release.version);
      if (!version || compareVersions(version, current) <= 0) continue;
      if (!candidate || compareVersions(version, candidate.version) > 0) candidate = { release, version };
    }
    if (!candidate) return null;

    const ticket = randomToken(UPDATE_TOKEN_BYTES);
    const now = this.now();
    await this.db.prepare(
      "INSERT INTO download_tickets (id, token_hash, device_id, release_id, purpose, expires_at, created_at) VALUES (?, ?, ?, ?, 'update', ?, ?)",
    ).bind(crypto.randomUUID(), await sha256Hex(ticket), input.deviceId, candidate.release.id, now + UPDATE_TICKET_SECONDS, now).run();
    return {
      version: candidate.release.version,
      notes: candidate.release.notes,
      pub_date: candidate.release.pub_date,
      url: new URL(`/v1/download/${ticket}`, origin).toString(),
      signature: candidate.release.signature,
    };
  }

  async download(input: { ticket: string; deviceId: string; testGroup: boolean }): Promise<Response> {
    requireTicket(input.ticket);
    if (!this.artifacts) throw new Error("Artifact storage is unavailable");
    const now = this.now();
    const record = await this.db.prepare(
      `SELECT r.id, r.version, r.status, r.notes, r.pub_date, r.signature, r.object_key
       FROM download_tickets dt JOIN releases r ON r.id = dt.release_id
       WHERE dt.token_hash = ? AND dt.device_id = ? AND dt.purpose = 'update'
         AND dt.expires_at > ? AND dt.revoked_at IS NULL`,
    ).bind(await sha256Hex(input.ticket), input.deviceId, now).first<DownloadRecord>();
    if (!record || !visibleToDevice(record.status, input.testGroup)) {
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

  async reportEvent(input: { deviceId: string; releaseId: string | null; currentVersion: string; eventType: string }): Promise<void> {
    if (!UPDATE_EVENT_TYPES.has(input.eventType) || input.deviceId.length === 0 || input.deviceId.length > 256 || !requireVersion(input.currentVersion)) {
      throw new UpdateError("INVALID_REQUEST", "Update event is invalid.");
    }
    if (input.releaseId !== null) {
      if (!/^[A-Za-z0-9_-]{1,256}$/.test(input.releaseId)) throw new UpdateError("INVALID_REQUEST", "Update event is invalid.");
      const release = await this.db.prepare("SELECT version FROM releases WHERE id = ?").bind(input.releaseId).first<{ version: string }>();
      if (!release || (input.eventType === "install_succeeded" && release.version !== input.currentVersion)) {
        throw new UpdateError("INVALID_REQUEST", "Update event is invalid.");
      }
    }
    const now = this.now();
    const statements = [
      this.db.prepare("INSERT INTO update_events (id, device_id, release_id, current_version, event_type, created_at) VALUES (?, ?, ?, ?, ?, ?)")
        .bind(crypto.randomUUID(), input.deviceId, input.releaseId, input.currentVersion, input.eventType, now),
    ];
    if (input.eventType === "install_succeeded") {
      statements.push(this.db.prepare("UPDATE devices SET current_version = ?, last_seen_at = ? WHERE id = ?").bind(input.currentVersion, now, input.deviceId));
    }
    if (statements.length === 1) await statements[0]!.run();
    else await this.db.batch(statements);
  }

  private now(): number {
    const now = this.clock();
    if (!Number.isSafeInteger(now)) throw new Error("Clock must return a safe integer timestamp");
    return now;
  }
}
