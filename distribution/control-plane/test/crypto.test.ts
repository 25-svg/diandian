import { describe, expect, it } from "vitest";

import { LEASE_SECONDS, randomToken, sha256Hex, signLease } from "../src/crypto";

function decodeBase64url(value: string): ArrayBuffer {
  const source = Buffer.from(value, "base64url");
  const output = new ArrayBuffer(source.byteLength);
  new Uint8Array(output).set(source);
  return output;
}

async function createKeyPair(): Promise<{ privateJwk: JsonWebKey; publicJwk: JsonWebKey }> {
  const pair = await crypto.subtle.generateKey(
    { name: "ECDSA", namedCurve: "P-256" },
    true,
    ["sign", "verify"],
  );
  return {
    privateJwk: await crypto.subtle.exportKey("jwk", pair.privateKey),
    publicJwk: await crypto.subtle.exportKey("jwk", pair.publicKey),
  };
}

async function verifyFixtureLease(lease: string, publicJwk: JsonWebKey): Promise<boolean> {
  const [encodedBody, encodedSignature] = lease.split(".");
  if (!encodedBody || !encodedSignature) return false;

  const key = await crypto.subtle.importKey(
    "jwk",
    publicJwk,
    { name: "ECDSA", namedCurve: "P-256" },
    false,
    ["verify"],
  );
  return crypto.subtle.verify(
    { name: "ECDSA", hash: "SHA-256" },
    key,
    decodeBase64url(encodedSignature),
    decodeBase64url(encodedBody),
  );
}

describe("offline license lease cryptography", () => {
  it("does not expose the device token and signs exactly seven days", async () => {
    const { privateJwk, publicJwk } = await createKeyPair();
    const token = randomToken(32);
    const issuedAt = 100;
    const expiresAt = issuedAt + LEASE_SECONDS;
    const lease = await signLease({ deviceId: "dev-1", issuedAt, expiresAt, serverTime: issuedAt }, privateJwk);
    const [encodedBody, encodedSignature] = lease.split(".");

    expect(token).not.toContain("=");
    expect(token).toMatch(/^[A-Za-z0-9_-]+$/);
    expect(await sha256Hex(token)).toMatch(/^[a-f0-9]{64}$/);
    expect(expiresAt - issuedAt).toBe(7 * 24 * 60 * 60);
    expect(encodedBody).toMatch(/^[A-Za-z0-9_-]+$/);
    expect(encodedSignature).toMatch(/^[A-Za-z0-9_-]+$/);
    expect(new TextDecoder().decode(decodeBase64url(encodedBody!))).toBe(
      '{"deviceId":"dev-1","issuedAt":100,"expiresAt":604900,"serverTime":100}',
    );
    expect(await verifyFixtureLease(lease, publicJwk)).toBe(true);
  });
});
