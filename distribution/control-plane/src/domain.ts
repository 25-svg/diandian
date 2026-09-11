export type AdminRole = "owner" | "operator";

export type DeviceStatus = "active" | "revoked";

export type ReleaseStatus = "draft" | "testing" | "production" | "halted";

export interface LeasePayload {
  deviceId: string;
  issuedAt: number;
  expiresAt: number;
  serverTime: number;
}

export interface UpdateResponse {
  version: string;
  notes: string;
  pub_date: string;
  url: string;
  signature: string;
}
