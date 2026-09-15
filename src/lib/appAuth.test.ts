import assert from "node:assert/strict";
import { canAccessRoute, createAppAuthClient, defaultRoute } from "./appAuth.js";

assert.equal(defaultRoute("anchor"), "总览");
assert.equal(defaultRoute("operations_manager"), "运营总览");
assert.equal(canAccessRoute("anchor", "切片"), true);
assert.equal(canAccessRoute("anchor", "运营总览"), false);
assert.equal(canAccessRoute("operations_manager", "主播团队"), true);
assert.equal(canAccessRoute("operations_manager", "直播间"), false);

const calls: Array<{ url: string; authorization: string | null }> = [];
const client = createAppAuthClient({ baseUrl: "https://auth.example/", fetcher: async (input, init) => {
  const headers = new Headers(init?.headers); calls.push({ url: String(input), authorization: headers.get("authorization") });
  if (String(input).endsWith("/login")) return Response.json({ token: "A".repeat(43), user: { id: "u1", username: "anchor.a", displayName: "主播甲", role: "anchor", anchorId: "anchor-a", managedTeamIds: [], mustChangePassword: false } });
  if (String(input).endsWith("/me")) return Response.json({ user: { id: "u1", username: "anchor.a", displayName: "主播甲", role: "anchor", anchorId: "anchor-a", managedTeamIds: [], mustChangePassword: false } });
  return new Response(null, { status: 204 });
} });

const user = await client.login(" Anchor.A ", "not-recorded-here");
assert.equal(user.role, "anchor");
assert.equal((await client.me())?.anchorId, "anchor-a");
assert.equal(calls[0]?.authorization, null);
assert.equal(calls[1]?.authorization, `Bearer ${"A".repeat(43)}`);
await client.logout();
assert.equal(client.hasSession(), false);
console.log("app auth tests passed");
