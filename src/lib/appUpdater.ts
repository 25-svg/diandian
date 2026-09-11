import { invoke } from "@tauri-apps/api/core";

export type PrivateUpdatePhase =
  | "idle" | "checking" | "downloading" | "waiting_for_idle"
  | "installing" | "failed" | "up_to_date";

export interface PrivateUpdateStatus {
  status: PrivateUpdatePhase;
  version: string | null;
  message: string;
}

export type InvokeUpdateStatus = (command: string, args?: Record<string, unknown>) => Promise<PrivateUpdateStatus>;

const invokeUpdateStatus: InvokeUpdateStatus = (command, args) =>
  invoke<PrivateUpdateStatus>(command, args);

export function readPrivateUpdateStatus(
  invokeStatus: InvokeUpdateStatus = invokeUpdateStatus,
): Promise<PrivateUpdateStatus> {
  return invokeStatus("get_private_update_status", undefined);
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
