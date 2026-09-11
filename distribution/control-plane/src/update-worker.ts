import type { Env } from "./env";
import { LicenseError, LicenseService, type LicenseErrorCode } from "./license-service";
import { LicenseRepository } from "./repository";

type LicenseEnv = Env & { LEASE_PRIVATE_JWK: string };

const errorStatus: Record<LicenseErrorCode, number> = {
  INVALID_REQUEST: 400,
  ACTIVATION_CODE_INVALID: 400,
  ACTIVATION_CODE_USED: 409,
  INSTALLATION_ALREADY_ACTIVATED: 409,
  DEVICE_TOKEN_INVALID: 401,
  DEVICE_REVOKED: 403,
};

function json(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { "content-type": "application/json; charset=utf-8", "cache-control": "no-store" },
  });
}

function error(code: LicenseErrorCode | "NOT_FOUND" | "METHOD_NOT_ALLOWED" | "INTERNAL_ERROR", message: string, status: number): Response {
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
  let value: unknown;
  try {
    value = await request.json();
  } catch {
    throw new LicenseError("INVALID_REQUEST", "Activation input is invalid.");
  }
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new LicenseError("INVALID_REQUEST", "Activation input is invalid.");
  const input = value as Record<string, unknown>;
  return { code: input.code as string, installId: input.installId as string, label: input.label as string };
}

function service(env: LicenseEnv): LicenseService {
  return new LicenseService(new LicenseRepository(env.DB), privateJwk(env.LEASE_PRIVATE_JWK));
}

async function handle(request: Request, env: LicenseEnv): Promise<Response> {
  const path = new URL(request.url).pathname;
  if (path === "/v1/activate") {
    if (request.method !== "POST") return error("METHOD_NOT_ALLOWED", "Method not allowed.", 405);
    const result = await service(env).activate(await activationInput(request));
    return json(result, 201);
  }
  if (path === "/v1/lease/renew") {
    if (request.method !== "POST") return error("METHOD_NOT_ALLOWED", "Method not allowed.", 405);
    return json(await service(env).renew(bearerToken(request)));
  }
  if (path === "/v1/license/status") {
    if (request.method !== "GET") return error("METHOD_NOT_ALLOWED", "Method not allowed.", 405);
    return json(await service(env).status(bearerToken(request)));
  }
  return error("NOT_FOUND", "Route not found.", 404);
}

export default {
  async fetch(request: Request, env: LicenseEnv, _context: ExecutionContext): Promise<Response> {
    try {
      return await handle(request, env);
    } catch (cause) {
      if (cause instanceof LicenseError) return error(cause.code, cause.message, errorStatus[cause.code]);
      return error("INTERNAL_ERROR", "Unable to process this request.", 500);
    }
  },
};
