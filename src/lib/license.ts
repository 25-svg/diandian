import { invoke } from "@tauri-apps/api/core";

export type LicenseStatus = "unactivated" | "valid" | "offline_grace" | "expired" | "revoked" | "clock_invalid" | "blocked";
export type LicenseStatusDto = { status: LicenseStatus; daysRemaining: number | null; message: string };
const messages: Record<LicenseStatus, string> = {
  unactivated: "请输入管理员提供的激活码，授权此电脑。",
  valid: "设备已授权。",
  offline_grace: "当前使用离线授权，请联网续期。",
  expired: "离线授权已到期，请联网重新验证。",
  revoked: "此设备授权已被撤销，请联系管理员。",
  clock_invalid: "系统时间异常，请校准时间后联网验证。",
  blocked: "无法安全验证授权，请联网重试或联系管理员。",
};
export function resolveLicenseView(value: unknown): "activation" | "app" | "app-with-warning" | "blocked" {
  const status = value && typeof value === "object" ? (value as Record<string, unknown>).status : undefined;
  if (status === "unactivated") return "activation";
  if (status === "valid") return "app";
  if (status === "offline_grace") {
    const days = (value as Record<string, unknown>).daysRemaining;
    if (Number.isInteger(days) && Number(days) >= 1 && Number(days) <= 7) return "app-with-warning";
  }
  return "blocked";
}
export function parseLicenseStatus(value: unknown): LicenseStatusDto {
  const blocked: LicenseStatusDto = { status: "blocked", daysRemaining: null, message: messages.blocked };
  if (!value || typeof value !== "object" || Array.isArray(value)) return blocked;
  const record = value as Record<string, unknown>;
  if (Object.keys(record).some(key => !["status", "daysRemaining", "message"].includes(key)) ||
      typeof record.status !== "string" || !Object.prototype.hasOwnProperty.call(messages, record.status) ||
      typeof record.message !== "string" || !("daysRemaining" in record)) return blocked;
  const status = record.status as LicenseStatus;
  if (status === "offline_grace" ? resolveLicenseView(record) !== "app-with-warning" : record.daysRemaining !== null) return blocked;
  // Ignore transport text. No backend error, path, or secret can become UI copy.
  return { status, daysRemaining: record.daysRemaining as number | null, message: messages[status] };
}
type Invoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>;
export function createLicenseClient(call: Invoke = invoke) {
  const pending = new Map<string, Promise<LicenseStatusDto>>();
  let tail: Promise<LicenseStatusDto> | null = null;
  function run(command: string, args?: Record<string, unknown>): Promise<LicenseStatusDto> {
    const duplicate = pending.get(command);
    if (duplicate) return duplicate;
    // Only command names identify pending work; activation secrets stay in args.
    const execute = () => call(command, args);
    const result = (tail ? tail.then(execute, execute) : execute()).then(parseLicenseStatus).catch((): LicenseStatusDto => ({
      status: "blocked", daysRemaining: null, message: "授权操作未完成，请检查网络后重试。",
    })).finally(() => {
      pending.delete(command);
      if (tail === result) tail = null;
    });
    pending.set(command, result);
    tail = result;
    return result;
  }
  return {
    status: () => run("get_license_status"),
    activate: (code: string, label: string) => run("activate_device", { code: code.trim(), label: label.trim() }),
    renew: () => run("renew_device_license"),
  };
}
