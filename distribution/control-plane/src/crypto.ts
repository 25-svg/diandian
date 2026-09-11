import type { LeasePayload } from "./domain";

const encoder = new TextEncoder();

export const LEASE_SECONDS = 7 * 24 * 60 * 60;

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
  return base64url(crypto.getRandomValues(new Uint8Array(bytes)));
}

export async function sha256Hex(value: string): Promise<string> {
  return hex(new Uint8Array(await crypto.subtle.digest("SHA-256", encoder.encode(value))));
}

export async function signLease(payload: LeasePayload, privateJwk: JsonWebKey): Promise<string> {
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

  return `${base64url(encoder.encode(body))}.${base64url(new Uint8Array(signature))}`;
}
