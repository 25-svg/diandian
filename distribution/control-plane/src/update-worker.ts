import type { Env } from "./env";
import { sha256Hex } from "./crypto";
import { LicenseError, LicenseService, type LicenseErrorCode } from "./license-service";
import { LicenseRepository } from "./repository";
import { UpdateError, UpdateService } from "./update-service";

type LicenseEnv = Env & { LEASE_PRIVATE_JWK: string };
const MAX_ACTIVATION_BODY_BYTES = 4 * 1024;
const MAX_UPDATE_EVENT_BODY_BYTES = 4 * 1024;

const errorStatus: Record<LicenseErrorCode, number> = {
  INVALID_REQUEST: 400,
  ACTIVATION_CODE_INVALID: 400,
  ACTIVATION_CODE_USED: 409,
  INSTALLATION_ALREADY_ACTIVATED: 409,
  DEVICE_TOKEN_INVALID: 401,
  DEVICE_REVOKED: 403,
  PAYLOAD_TOO_LARGE: 413,
};

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json; charset=utf-8", "cache-control": "no-store" },
  });
}

function error(code: LicenseErrorCode | "INVALID_UPDATE_REQUEST" | "DOWNLOAD_UNAVAILABLE" | "ARTIFACT_NOT_FOUND" | "NOT_FOUND" | "METHOD_NOT_ALLOWED" | "INTERNAL_ERROR", message: string, status: number): Response {
  return json({ error: { code, message } }, status);
}

function bearerToken(request: Request): string {
  const match = /^Bearer ([A-Za-z0-9_-]{43})$/.exec(request.headers.get("authorization") ?? "");
  if (!match) throw new LicenseError("DEVICE_TOKEN_INVALID", "A valid Bearer device token is required.");
  return match[1]!;
}

function privateJwk(secret: string): JsonWebKey {
  if (typeof secret !== "string" || secret.length === 0 || secret.length > 16_384) throw new Error("Invalid lease signing configuration");
  const value: unknown = JSON.parse(secret);
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("Invalid lease signing configuration");
  return value as JsonWebKey;
}

async function activationInput(request: Request): Promise<{ code: string; installId: string; label: string }> {
  const contentLength = request.headers.get("content-length");
  if (contentLength && (!/^\d+$/.test(contentLength) || Number(contentLength) > MAX_ACTIVATION_BODY_BYTES)) {
    throw new LicenseError("PAYLOAD_TOO_LARGE", "Activation request body is too large.");
  }
  if (!request.body) throw new LicenseError("INVALID_REQUEST", "Activation input is invalid.");
  const reader = request.body.getReader();
  const chunks: Uint8Array[] = [];
  let length = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      length += value.byteLength;
      if (length > MAX_ACTIVATION_BODY_BYTES) {
        try {
          await reader.cancel();
        } catch {
          // Cleanup failure cannot replace the client-visible payload limit error.
        }
        throw new LicenseError("PAYLOAD_TOO_LARGE", "Activation request body is too large.");
      }
      chunks.push(value);
    }
  } finally {
    reader.releaseLock();
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  let value: unknown;
  try {
    value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    throw new LicenseError("INVALID_REQUEST", "Activation input is invalid.");
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new LicenseError("INVALID_REQUEST", "Activation input is invalid.");
  const input = value as Record<string, unknown>;
  return { code: input.code as string, installId: input.installId as string, label: input.label as string };
}

function signingService(env: LicenseEnv): LicenseService {
  return new LicenseService(new LicenseRepository(env.DB), privateJwk(env.LEASE_PRIVATE_JWK));
}

function statusService(env: LicenseEnv): LicenseService {
  return new LicenseService(new LicenseRepository(env.DB), undefined);
}

async function activeDevice(request: Request, env: LicenseEnv) {
  const tokenHash = await sha256Hex(bearerToken(request));
  const device = await new LicenseRepository(env.DB).findDeviceByTokenHash(tokenHash);
  if (!device) throw new LicenseError("DEVICE_TOKEN_INVALID", "A valid Bearer device token is required.");
  if (device.status !== "active") throw new LicenseError("DEVICE_REVOKED", "This device has been revoked.");
  return { id: device.id, testGroup: device.test_group === "1" };
}

function decodePathPart(value: string): string {
  try {
    const decoded = decodeURIComponent(value);
    if (decoded.length === 0 || /[\\/\u0000\r\n]/.test(decoded)) throw new Error("invalid");
    return decoded;
  } catch {
    throw new UpdateError("INVALID_REQUEST", "Update request path is invalid.");
  }
}

async function updateEventInput(request: Request): Promise<{ deviceId?: unknown; releaseId: unknown; currentVersion: unknown; eventType: unknown }> {
  const contentLength = request.headers.get("content-length");
  if (contentLength && (!/^\d+$/.test(contentLength) || Number(contentLength) > MAX_UPDATE_EVENT_BODY_BYTES)) {
    throw new LicenseError("PAYLOAD_TOO_LARGE", "Update event body is too large.");
  }
  if (!request.body) throw new LicenseError("INVALID_REQUEST", "Update event is invalid.");
  const reader = request.body.getReader();
  const chunks: Uint8Array[] = [];
  let length = 0;
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) break;
      length += value.byteLength;
      if (length > MAX_UPDATE_EVENT_BODY_BYTES) {
        try { await reader.cancel(); } catch { /* Preserve the stable limit response. */ }
        throw new LicenseError("PAYLOAD_TOO_LARGE", "Update event body is too large.");
      }
      chunks.push(value);
    }
  } finally {
    reader.releaseLock();
  }
  const bytes = new Uint8Array(length);
  let offset = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, offset);
    offset += chunk.byteLength;
  }
  let value: unknown;
  try {
    value = JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
  } catch {
    throw new LicenseError("INVALID_REQUEST", "Update event is invalid.");
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new LicenseError("INVALID_REQUEST", "Update event is invalid.");
  const input = value as Record<string, unknown>;
  const keys = Object.keys(input);
  if (keys.some((key) => !["deviceId", "releaseId", "currentVersion", "eventType"].includes(key))) {
    throw new LicenseError("INVALID_REQUEST", "Update event is invalid.");
  }
  return { deviceId: input.deviceId, releaseId: input.releaseId, currentVersion: input.currentVersion, eventType: input.eventType };
}

function updateService(env: LicenseEnv): UpdateService {
  return new UpdateService(env.DB, undefined, env.ARTIFACTS);
}

async function handle(request: Request, env: LicenseEnv): Promise<Response> {
  const path = new URL(request.url).pathname;
  if (path === "/v1/activate") {
    if (request.method !== "POST") return error("METHOD_NOT_ALLOWED", "Method not allowed.", 405);
    const result = await signingService(env).activate(await activationInput(request));
    return json(result, 201);
  }
  if (path === "/v1/lease/renew") {
    if (request.method !== "POST") return error("METHOD_NOT_ALLOWED", "Method not allowed.", 405);
    return json(await signingService(env).renew(bearerToken(request)));
  }
  if (path === "/v1/license/status") {
    if (request.method !== "GET") return error("METHOD_NOT_ALLOWED", "Method not allowed.", 405);
    return json(await statusService(env).status(bearerToken(request)));
  }
  const rawParts = path.split("/");
  if (rawParts.length === 6 && rawParts[1] === "v1" && rawParts[2] === "update") {
    if (request.method !== "GET") return error("METHOD_NOT_ALLOWED", "Method not allowed.", 405);
    const device = await activeDevice(request, env);
    const result = await updateService(env).resolveUpdate({
      deviceId: device.id,
      testGroup: device.testGroup,
      target: decodePathPart(rawParts[3]!),
      arch: decodePathPart(rawParts[4]!),
      currentVersion: decodePathPart(rawParts[5]!),
      origin: new URL(request.url).origin,
    });
    return result ? json(result) : new Response(null, { status: 204, headers: { "cache-control": "no-store" } });
  }
  if (rawParts.length === 4 && rawParts[1] === "v1" && rawParts[2] === "download") {
    if (request.method !== "GET") return error("METHOD_NOT_ALLOWED", "Method not allowed.", 405);
    const device = await activeDevice(request, env);
    return updateService(env).download({ ticket: decodePathPart(rawParts[3]!), deviceId: device.id, testGroup: device.testGroup });
  }
  if (path === "/v1/update-events") {
    if (request.method !== "POST") return error("METHOD_NOT_ALLOWED", "Method not allowed.", 405);
    const device = await activeDevice(request, env);
    const input = await updateEventInput(request);
    if (input.deviceId !== undefined && input.deviceId !== device.id) throw new UpdateError("INVALID_REQUEST", "Update event is invalid.");
    if ((input.releaseId !== null && typeof input.releaseId !== "string") || typeof input.currentVersion !== "string" || typeof input.eventType !== "string") {
      throw new UpdateError("INVALID_REQUEST", "Update event is invalid.");
    }
    await updateService(env).reportEvent({ deviceId: device.id, releaseId: input.releaseId, currentVersion: input.currentVersion, eventType: input.eventType });
    return new Response(null, { status: 204, headers: { "cache-control": "no-store" } });
  }
  return error("NOT_FOUND", "Route not found.", 404);
}

export default {
  async fetch(request: Request, env: LicenseEnv, _context: ExecutionContext): Promise<Response> {
    try {
      return await handle(request, env);
    } catch (cause) {
      if (cause instanceof LicenseError) return error(cause.code, cause.message, errorStatus[cause.code]);
      if (cause instanceof UpdateError) {
        const status = cause.code === "INVALID_REQUEST" ? 400 : cause.code === "ARTIFACT_NOT_FOUND" ? 404 : 403;
        const code = cause.code === "INVALID_REQUEST" ? "INVALID_UPDATE_REQUEST" : cause.code === "ARTIFACT_NOT_FOUND" ? "ARTIFACT_NOT_FOUND" : "DOWNLOAD_UNAVAILABLE";
        return error(code, cause.message, status);
      }
      return error("INTERNAL_ERROR", "Unable to process this request.", 500);
    }
  },
};
