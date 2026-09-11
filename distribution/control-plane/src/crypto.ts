import type { LeasePayload } from "./domain";

const encoder = new TextEncoder();

export const LEASE_SECONDS = 7 * 24 * 60 * 60;

const MAX_TOKEN_BYTES = 65_536;
const P256_SIGNATURE_BYTES = 64;

function base64url(bytes: Uint8Array): string {
  return btoa(String.fromCharCode(...bytes))
    .replace(/\+/g, "-")
    .replace(/\//g, "_")
    .replace(/=+$/g, "");
}

function hex(bytes: Uint8Array): string {
  return [...bytes].map((byte) => byte.toString(16).padStart(2, "0")).join("");
}

export function randomToken(bytes: number): string {
  if (!Number.isInteger(bytes) || bytes < 1 || bytes > MAX_TOKEN_BYTES) {
    throw new RangeError(`Token byte count must be an integer from 1 through ${MAX_TOKEN_BYTES}`);
  }
  return base64url(crypto.getRandomValues(new Uint8Array(bytes)));
}

export async function sha256Hex(value: string): Promise<string> {
  return hex(new Uint8Array(await crypto.subtle.digest("SHA-256", encoder.encode(value))));
}

function assertSafeLeaseTime(value: number, field: string): void {
  if (!Number.isSafeInteger(value)) {
    throw new RangeError(`${field} must be a finite safe integer`);
  }
}

/**
 * Signs canonical JSON as an ECDSA P-256 SHA-256 lease. The signature segment is
 * IEEE-P1363 raw `r || s`, encoded as exactly 64 bytes before base64url encoding.
 */
export async function signLease(payload: LeasePayload, privateJwk: JsonWebKey): Promise<string> {
  assertSafeLeaseTime(payload.issuedAt, "issuedAt");
  assertSafeLeaseTime(payload.expiresAt, "expiresAt");
  assertSafeLeaseTime(payload.serverTime, "serverTime");
  if (payload.expiresAt - payload.issuedAt !== LEASE_SECONDS) {
    throw new RangeError(`expiresAt must be exactly ${LEASE_SECONDS} seconds after issuedAt`);
  }

  const body = JSON.stringify({
    deviceId: payload.deviceId,
    issuedAt: payload.issuedAt,
    expiresAt: payload.expiresAt,
    serverTime: payload.serverTime,
  });
  const key = await crypto.subtle.importKey(
    "jwk",
    privateJwk,
    { name: "ECDSA", namedCurve: "P-256" },
    false,
    ["sign"],
  );
  const signature = await crypto.subtle.sign(
    { name: "ECDSA", hash: "SHA-256" },
    key,
    encoder.encode(body),
  );
  const signatureBytes = new Uint8Array(signature);
  if (signatureBytes.byteLength !== P256_SIGNATURE_BYTES) {
    throw new Error("ECDSA P-256 signature must use a 64-byte IEEE-P1363 encoding");
  }

  return `${base64url(encoder.encode(body))}.${base64url(signatureBytes)}`;
}
