import { fetch as tauriFetch } from "@tauri-apps/plugin-http";

export type AppRole = "operations_manager" | "anchor";
export type AppUser = {
  id: string;
  username: string;
  displayName: string;
  role: AppRole;
  anchorId: string | null;
  managedTeamIds: string[];
  mustChangePassword: boolean;
};

export const anchorRoutes = ["总览", "直播间", "录播", "录播分析", "切片", "任务", "情景训练", "主播教练", "相机知识问答", "主播知识库", "账号", "设置", "关于"] as const;
export const operationsRoutes = ["运营总览", "主播团队", "改进任务", "资产审核", "设置", "关于"] as const;

export function defaultRoute(role: AppRole): string { return role === "operations_manager" ? "运营总览" : "总览"; }
export function canAccessRoute(role: AppRole, route: string): boolean {
  return (role === "operations_manager" ? operationsRoutes : anchorRoutes).includes(route as never);
}

function parseUser(value: unknown): AppUser | null {
  if (!value || typeof value !== "object" || Array.isArray(value)) return null;
  const user = value as Record<string, unknown>;
  const role = user.role;
  if ((role !== "operations_manager" && role !== "anchor") || typeof user.id !== "string" || typeof user.username !== "string" || typeof user.displayName !== "string"
    || (user.anchorId !== null && typeof user.anchorId !== "string") || !Array.isArray(user.managedTeamIds) || user.managedTeamIds.some((id) => typeof id !== "string") || typeof user.mustChangePassword !== "boolean") return null;
  if ((role === "anchor" && !user.anchorId) || (role === "operations_manager" && user.anchorId !== null)) return null;
  return user as AppUser;
}

type Fetch = (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
export type AppAuthClient = ReturnType<typeof createAppAuthClient>;

export function createAppAuthClient(options: { baseUrl?: string; fetcher?: Fetch } = {}) {
  const buildEnv = (import.meta as ImportMeta & { env?: { VITE_APP_AUTH_API_BASE_URL?: string } }).env;
  const baseUrl = (options.baseUrl ?? buildEnv?.VITE_APP_AUTH_API_BASE_URL ?? "").replace(/\/$/, "");
  const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;
  const fetcher = options.fetcher ?? (isTauri ? tauriFetch : globalThis.fetch.bind(globalThis));
  let token = "";

  async function request(path: string, init: RequestInit = {}): Promise<unknown> {
    if (!baseUrl) throw new Error("AUTH_NOT_CONFIGURED");
    const headers = new Headers(init.headers);
    if (token) headers.set("authorization", `Bearer ${token}`);
    const response = await fetcher(`${baseUrl}/v1/app-auth/${path}`, { ...init, headers, cache: "no-store" });
    if (!response.ok) {
      if (response.status === 401 && path !== "login") token = "";
      throw new Error(response.status === 401 ? "INVALID_CREDENTIALS" : "AUTH_UNAVAILABLE");
    }
    if (response.status === 204) return null;
    return response.json();
  }

  return {
    async login(username: string, password: string): Promise<AppUser> {
      const value = await request("login", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ username: username.trim().toLowerCase(), password }) }) as Record<string, unknown>;
      const nextUser = parseUser(value?.user);
      if (!nextUser || typeof value?.token !== "string" || !/^[A-Za-z0-9_-]{43}$/.test(value.token)) throw new Error("INVALID_RESPONSE");
      token = value.token; return nextUser;
    },
    async me(): Promise<AppUser | null> {
      if (!token) return null;
      try { return parseUser((await request("me") as Record<string, unknown>)?.user); } catch { return null; }
    },
    async changePassword(currentPassword: string, newPassword: string): Promise<AppUser> {
      const value = await request("password", { method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ currentPassword, newPassword }) }) as Record<string, unknown>;
      const nextUser = parseUser(value?.user); if (!nextUser) throw new Error("INVALID_RESPONSE"); return nextUser;
    },
    async logout(): Promise<void> { try { if (token) await request("logout", { method: "POST" }); } finally { token = ""; } },
    hasSession(): boolean { return Boolean(token); },
  };
}
