import { describe, expect, it, vi } from "vitest";
import { exportJWK, generateKeyPair, SignJWT } from "jose";

import { createAccessIdentityAdapter, createServiceIdentityAdapter } from "../src/admin-auth";

const issuer = "https://team.cloudflareaccess.com";
const jwksUrl = `${issuer}/cdn-cgi/access/certs`;

async function jwt(claims: Record<string, unknown>, kid = "key-1") {
  const { privateKey, publicKey } = await generateKeyPair("RS256");
  const jwk = await exportJWK(publicKey); Object.assign(jwk, { kid, alg: "RS256", use: "sig" });
  const token = await new SignJWT(claims).setProtectedHeader({ alg: "RS256", kid }).sign(privateKey);
  return { token, jwk };
}

describe("Access identity boundary", () => {
  it("never treats the unverified email header as an identity", async () => {
    const adapter = createAccessIdentityAdapter({
      issuer: "https://team.cloudflareaccess.com",
      audience: "admin-audience",
      jwksUrl: "https://team.cloudflareaccess.com/cdn-cgi/access/certs",
    });

    await expect(adapter.verify(new Request("https://admin.example/api/admin/overview", {
      headers: { "cf-access-authenticated-user-email": "owner@example.com" },
    }))).resolves.toBeNull();
  });

  it("keeps admin and publisher audiences separate", async () => {
    const admin = createAccessIdentityAdapter({ issuer: "https://team.cloudflareaccess.com", audience: "admin-audience", jwksUrl: "https://team.cloudflareaccess.com/cdn-cgi/access/certs" });
    const publisher = createAccessIdentityAdapter({ issuer: "https://team.cloudflareaccess.com", audience: "publisher-audience", jwksUrl: "https://team.cloudflareaccess.com/cdn-cgi/access/certs" });

    // Invalid compact JWS must fail closed for both trust domains; no header can bridge them.
    const request = new Request("https://admin.example/api/admin/overview", { headers: { "cf-access-jwt-assertion": "not.a.jwt", "cf-access-authenticated-user-email": "owner@example.com" } });
    await expect(admin.verify(request)).resolves.toBeNull();
    await expect(publisher.verify(request)).resolves.toBeNull();
  });

  it("accepts signed user and service Access JWTs only for their own required identity claim", async () => {
    const now = Math.floor(Date.now() / 1000);
    const user = await jwt({ iss: issuer, aud: "admin", exp: now + 300, iat: now, email: "owner@example.com" });
    const service = await jwt({ iss: issuer, aud: "publisher", exp: now + 300, iat: now, common_name: "release-ci" }, "key-2");
    const fetch = vi.fn(async () => new Response(JSON.stringify({ keys: [user.jwk, service.jwk] }), { headers: { "content-type": "application/json" } }));
    vi.stubGlobal("fetch", fetch);
    const admin = createAccessIdentityAdapter({ issuer, audience: "admin", jwksUrl });
    const publisher = createServiceIdentityAdapter({ issuer, audience: "publisher", jwksUrl });
    await expect(admin.verify(new Request("https://admin", { headers: { "cf-access-jwt-assertion": user.token } }))).resolves.toEqual({ email: "owner@example.com" });
    await expect(publisher.verify(new Request("https://admin", { headers: { "cf-access-jwt-assertion": service.token } }))).resolves.toEqual({ serviceTokenId: "release-ci" });
    await expect(publisher.verify(new Request("https://admin", { headers: { "cf-access-jwt-assertion": user.token } }))).resolves.toBeNull();
    vi.unstubAllGlobals();
  });

  it.each([
    [{ exp: 1_900_000_000, iat: 1_800_000_000, email: "a@example.com" }, "missing issuer/audience"],
    [{ iss: issuer, aud: "admin", exp: 1_900_000_000, email: "a@example.com" }, "missing iat"],
    [{ iss: issuer, aud: "admin", exp: 1_900_000_000, iat: 9_999_999_999, email: "a@example.com" }, "future iat"],
  ])("fails closed on %s", async (claims, label) => {
    const audience = `claim-${label}`;
    const signed = await jwt({ ...claims, ...(("aud" in claims && claims.aud) ? { aud: audience } : {}) });
    vi.stubGlobal("fetch", async () => new Response(JSON.stringify({ keys: [signed.jwk] }), { headers: { "content-type": "application/json" } }));
    const adapter = createAccessIdentityAdapter({ issuer, audience, jwksUrl });
    await expect(adapter.verify(new Request("https://admin", { headers: { "cf-access-jwt-assertion": signed.token } }))).resolves.toBeNull();
    vi.unstubAllGlobals();
  });
});
