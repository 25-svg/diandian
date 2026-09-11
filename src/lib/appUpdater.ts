import { invoke } from "@tauri-apps/api/core";

export type PrivateUpdatePhase =
  | "idle" | "checking" | "downloading" | "waiting_for_idle"
  | "installing" | "failed" | "up_to_date";

export interface PrivateUpdateStatus {
  status: PrivateUpdatePhase;
  version: string | null;
  message: string;
}

export type InvokeUpdateStatus = (command: string, args?: Record<string, unknown>) => Promise<unknown>;

const invokeUpdateStatus: InvokeUpdateStatus = (command, args) =>
  invoke<unknown>(command, args);

const PHASE_MESSAGES: Record<PrivateUpdatePhase, string> = {
  idle: "更新服务待命。",
  checking: "正在安全检查更新。",
  downloading: "正在后台下载并校验更新。",
  waiting_for_idle: "更新已就绪，将在软件连续空闲两分钟后自动安装。",
  installing: "正在安装更新，软件即将重启。",
  failed: "本次更新未完成，稍后将自动重试。",
  up_to_date: "当前已是最新版本。",
};

function parsePrivateUpdateStatus(value: unknown): PrivateUpdateStatus {
  if (!value || typeof value !== "object" || Array.isArray(value)) throw new Error("Invalid private update status");
  const record = value as Record<string, unknown>;
  const keys = Object.keys(record).sort();
  if (keys.length !== 3 || keys[0] !== "message" || keys[1] !== "status" || keys[2] !== "version") throw new Error("Invalid private update status");
  if (typeof record.status !== "string" || !(record.status in PHASE_MESSAGES)) throw new Error("Invalid private update status");
  const status = record.status as PrivateUpdatePhase;
  if (record.message !== PHASE_MESSAGES[status]) throw new Error("Invalid private update status");
  if (record.version !== null && (typeof record.version !== "string" || record.version.length === 0 || record.version.length > 128 || !/^[0-9A-Za-z.+-]+$/.test(record.version))) throw new Error("Invalid private update status");
  return { status, version: record.version as string | null, message: PHASE_MESSAGES[status] };
}

export function readPrivateUpdateStatus(
  invokeStatus: InvokeUpdateStatus = invokeUpdateStatus,
): Promise<PrivateUpdateStatus> {
  return invokeStatus("get_private_update_status", undefined).then(parsePrivateUpdateStatus);
}

export function watchPrivateUpdateStatus(
  onStatus: (status: PrivateUpdateStatus) => void,
  intervalMs = 5_000,
  invokeStatus: InvokeUpdateStatus = invokeUpdateStatus,
): () => void {
  let stopped = false;
  const refresh = async () => {
    try {
      const status = await readPrivateUpdateStatus(invokeStatus);
      if (!stopped) onStatus(status);
    } catch {
      // Rust coordinator owns retries and safe diagnostics; UI polling is best effort.
    }
  };
  void refresh();
  const timer = globalThis.setInterval(refresh, intervalMs);
  return () => { stopped = true; globalThis.clearInterval(timer); };
}
