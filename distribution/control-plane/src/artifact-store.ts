import type { Env } from "./env";

export interface ArtifactHead {
  size: number;
  customMetadata?: Record<string, string>;
}

export interface ArtifactObject extends ArtifactHead {
  body: ReadableStream<Uint8Array>;
}

export interface ArtifactStore {
  head(key: string): Promise<ArtifactHead | null>;
  get(key: string): Promise<ArtifactObject | null>;
}

type Fetcher = (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
type GitHubAsset = { id: number; name: string; size: number; digest?: string | null };

const API_VERSION = "2022-11-28";
const MAX_METADATA_BYTES = 16 * 1024;

function githubAssetId(key: string): string | null {
  const match = /^github:([1-9]\d{0,19})$/.exec(key);
  return match?.[1] ?? null;
}

function validRepository(value: string): boolean {
  return /^[A-Za-z0-9_.-]{1,100}\/[A-Za-z0-9_.-]{1,100}$/.test(value);
}

function validSecret(value: string): boolean {
  return value.length >= 20 && value.length <= 4096 && !/[\x00-\x20\x7f-\x9f]/.test(value);
}

async function boundedJson(response: Response): Promise<unknown> {
  const length = response.headers.get("content-length");
  if (length && (!/^\d+$/.test(length) || Number(length) > MAX_METADATA_BYTES)) throw new Error("GitHub asset metadata is invalid");
  const reader = response.body?.getReader();
  if (!reader) throw new Error("GitHub asset metadata is invalid");
  const chunks: Uint8Array[] = [];
  let total = 0;
  try {
    while (true) {
      const part = await reader.read();
      if (part.done) break;
      total += part.value.byteLength;
      if (total > MAX_METADATA_BYTES) {
        await reader.cancel();
        throw new Error("GitHub asset metadata is invalid");
      }
      chunks.push(part.value);
    }
  } finally {
    reader.releaseLock();
  }
  const bytes = new Uint8Array(total);
  let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
  return JSON.parse(new TextDecoder("utf-8", { fatal: true }).decode(bytes));
}

function parseAsset(value: unknown, expectedId: string): GitHubAsset {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("GitHub asset metadata is invalid");
  const asset = value as Record<string, unknown>;
  if (!Number.isSafeInteger(asset.id) || String(asset.id) !== expectedId || typeof asset.name !== "string"
    || asset.name.length < 1 || asset.name.length > 255 || !Number.isSafeInteger(asset.size) || (asset.size as number) < 1
    || (asset.digest !== null && asset.digest !== undefined && typeof asset.digest !== "string")) {
    throw new Error("GitHub asset metadata is invalid");
  }
  return asset as unknown as GitHubAsset;
}

function sha256Metadata(asset: GitHubAsset): Record<string, string> | undefined {
  const match = /^sha256:([a-f0-9]{64})$/i.exec(asset.digest ?? "");
  return match ? { sha256: match[1]!.toLowerCase() } : undefined;
}

function safeAssetLocation(value: string | null): URL {
  if (!value) throw new Error("GitHub asset redirect is invalid");
  const url = new URL(value);
  const hostname = url.hostname.toLowerCase();
  const allowed = hostname === "release-assets.githubusercontent.com"
    || hostname.endsWith(".release-assets.githubusercontent.com")
    || hostname === "objects.githubusercontent.com"
    || hostname.endsWith(".objects.githubusercontent.com");
  if (url.protocol !== "https:" || url.username || url.password || !allowed) throw new Error("GitHub asset redirect is invalid");
  return url;
}

export class GitHubArtifactStore implements ArtifactStore {
  constructor(
    private readonly repository: string,
    private readonly token: string,
    private readonly fetcher: Fetcher = (input, init) => fetch(input, init),
  ) {
    if (!validRepository(repository) || !validSecret(token)) throw new Error("GitHub artifact storage is unavailable");
  }

  private endpoint(assetId: string): string {
    return `https://api.github.com/repos/${this.repository}/releases/assets/${assetId}`;
  }

  private headers(accept: string): HeadersInit {
    return {
      accept,
      authorization: `Bearer ${this.token}`,
      "user-agent": "diandian-update-worker",
      "x-github-api-version": API_VERSION,
    };
  }

  async head(key: string): Promise<ArtifactHead | null> {
    const assetId = githubAssetId(key);
    if (!assetId) return null;
    const response = await this.fetcher(this.endpoint(assetId), { headers: this.headers("application/vnd.github+json"), redirect: "manual" });
    if (response.status === 404) { await response.body?.cancel(); return null; }
    if (!response.ok) { await response.body?.cancel(); throw new Error("GitHub asset metadata is unavailable"); }
    const asset = parseAsset(await boundedJson(response), assetId);
    return { size: asset.size, customMetadata: sha256Metadata(asset) };
  }

  async get(key: string): Promise<ArtifactObject | null> {
    const assetId = githubAssetId(key);
    if (!assetId) return null;
    const response = await this.fetcher(this.endpoint(assetId), { headers: this.headers("application/octet-stream"), redirect: "manual" });
    if (response.status === 404) { await response.body?.cancel(); return null; }
    let binary = response;
    if (response.status === 302 || response.status === 301 || response.status === 307 || response.status === 308) {
      const location = safeAssetLocation(response.headers.get("location"));
      await response.body?.cancel();
      binary = await this.fetcher(location, { redirect: "manual", headers: { "user-agent": "diandian-update-worker" } });
    }
    if (!binary.ok || !binary.body) { await binary.body?.cancel(); throw new Error("GitHub release asset is unavailable"); }
    const sizeRaw = binary.headers.get("content-length");
    const size = sizeRaw && /^\d+$/.test(sizeRaw) ? Number(sizeRaw) : NaN;
    if (!Number.isSafeInteger(size) || size < 1) { await binary.body.cancel(); throw new Error("GitHub release asset size is invalid"); }
    return { body: binary.body, size };
  }
}

export function artifactStoreFromEnv(env: Pick<Env, "ARTIFACTS" | "GITHUB_RELEASES_REPOSITORY" | "GITHUB_RELEASES_TOKEN">): ArtifactStore {
  if (env.GITHUB_RELEASES_REPOSITORY && env.GITHUB_RELEASES_TOKEN) {
    return new GitHubArtifactStore(env.GITHUB_RELEASES_REPOSITORY, env.GITHUB_RELEASES_TOKEN);
  }
  if (env.ARTIFACTS) return env.ARTIFACTS;
  throw new Error("Artifact storage is unavailable");
}
