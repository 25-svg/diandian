import { describe, expect, it } from "vitest";

import { createAccessIdentityAdapter } from "../src/admin-auth";

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
});
