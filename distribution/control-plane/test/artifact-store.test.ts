import { describe, expect, it, vi } from "vitest";

import { GitHubArtifactStore, artifactStoreFromEnv } from "../src/artifact-store";

const token = "github_pat_TEST_ONLY_abcdefghijklmnopqrstuvwxyz";

describe("GitHubArtifactStore", () => {
  it("reads private asset metadata with a scoped bearer token", async () => {
    const fetcher = vi.fn(async (_input: RequestInfo | URL, init?: RequestInit) => new Response(JSON.stringify({
      id: 123,
      name: "diandian.exe",
      size: 456,
      digest: `sha256:${"a".repeat(64)}`,
    }), { status: 200, headers: { "content-type": "application/json" } }));
    const store = new GitHubArtifactStore("25-svg/diandian-releases", token, fetcher);

    await expect(store.head("github:123")).resolves.toEqual({ size: 456, customMetadata: { sha256: "a".repeat(64) } });
    const init = fetcher.mock.calls[0]![1] as RequestInit;
    expect(new Headers(init.headers).get("authorization")).toBe(`Bearer ${token}`);
    expect(new Headers(init.headers).get("accept")).toBe("application/vnd.github+json");
  });

  it("streams a redirected private asset without forwarding the GitHub token", async () => {
    const calls: Array<{ url: string; authorization: string | null }> = [];
    const fetcher = vi.fn(async (input: RequestInfo | URL, init?: RequestInit) => {
      calls.push({ url: String(input), authorization: new Headers(init?.headers).get("authorization") });
      if (calls.length === 1) {
        return new Response(null, { status: 302, headers: { location: "https://release-assets.githubusercontent.com/example/signed" } });
      }
      return new Response(new Uint8Array([1, 2, 3]), { status: 200, headers: { "content-length": "3" } });
    });
    const store = new GitHubArtifactStore("25-svg/diandian-releases", token, fetcher);

    const object = await store.get("github:987");
    expect(object?.size).toBe(3);
    expect(new Uint8Array(await new Response(object?.body).arrayBuffer())).toEqual(new Uint8Array([1, 2, 3]));
    expect(calls).toEqual([
      { url: "https://api.github.com/repos/25-svg/diandian-releases/releases/assets/987", authorization: `Bearer ${token}` },
      { url: "https://release-assets.githubusercontent.com/example/signed", authorization: null },
    ]);
  });

  it("rejects redirects that could leak download access outside GitHub asset storage", async () => {
    const store = new GitHubArtifactStore("25-svg/diandian-releases", token, async () => new Response(null, {
      status: 302,
      headers: { location: "https://evil.example/collect" },
    }));
    await expect(store.get("github:123")).rejects.toThrow("redirect is invalid");
  });

  it("fails closed for malformed keys and missing storage configuration", async () => {
    const fetcher = vi.fn();
    const store = new GitHubArtifactStore("25-svg/diandian-releases", token, fetcher);
    await expect(store.get("../secret")).resolves.toBeNull();
    expect(fetcher).not.toHaveBeenCalled();
    expect(() => artifactStoreFromEnv({})).toThrow("Artifact storage is unavailable");
  });
});
