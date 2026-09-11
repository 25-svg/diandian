import { LEASE_SECONDS, randomToken, sha256Hex, signLease } from "./crypto";
import { LicenseRepository } from "./repository";

const MAX_CODE_LENGTH = 256;
const MAX_INSTALL_ID_LENGTH = 256;
const MAX_LABEL_LENGTH = 256;
const DEVICE_TOKEN_BYTES = 32;

export type LicenseErrorCode =
  | "INVALID_REQUEST"
  | "ACTIVATION_CODE_INVALID"
  | "ACTIVATION_CODE_USED"
  | "INSTALLATION_ALREADY_ACTIVATED"
  | "DEVICE_TOKEN_INVALID"
  | "DEVICE_REVOKED";

export class LicenseError extends Error {
  constructor(public readonly code: LicenseErrorCode, message: string) {
    super(message);
  }
}

export interface LicenseServiceContract {
  activate(input: { code: string; installId: string; label: string }): Promise<{ deviceToken: string; lease: string }>;
  renew(deviceToken: string): Promise<{ lease: string }>;
  status(deviceToken: string): Promise<{ status: "active" | "revoked"; serverTime: number }>;
}

export class LicenseService implements LicenseServiceContract {
  constructor(
    private readonly repository: LicenseRepository,
    private readonly privateJwk: JsonWebKey,
    private readonly clock: () => number = () => Math.floor(Date.now() / 1000),
  ) {}

  async activate(input: { code: string; installId: string; label: string }): Promise<{ deviceToken: string; lease: string }> {
    const validated = this.validateActivation(input);
    await this.validateSigningKey();
    const now = this.now();
    const deviceToken = randomToken(DEVICE_TOKEN_BYTES);
    const deviceId = crypto.randomUUID();
    const [codeHash, installHash, tokenHash] = await Promise.all([
      sha256Hex(validated.code),
      sha256Hex(validated.installId),
      sha256Hex(deviceToken),
    ]);
    const claimed = await this.repository.claimActivation({
      codeHash,
      installHash,
      tokenHash,
      deviceId,
      label: validated.label,
      now,
    });
    if (!claimed) await this.throwActivationFailure(codeHash, installHash, now);

    return { deviceToken, lease: await this.lease(deviceId, now) };
  }

  async renew(deviceToken: string): Promise<{ lease: string }> {
    const device = await this.requireActiveDevice(deviceToken);
    const now = this.now();
    return { lease: await this.lease(device.id, now) };
  }

  async status(deviceToken: string): Promise<{ status: "active" | "revoked"; serverTime: number }> {
    const tokenHash = await this.tokenHash(deviceToken);
    const device = await this.repository.findDeviceByTokenHash(tokenHash);
    if (!device) throw new LicenseError("DEVICE_TOKEN_INVALID", "A valid Bearer device token is required.");
    return { status: device.status, serverTime: this.now() };
  }

  private validateActivation(input: { code: string; installId: string; label: string }) {
    if (!input || typeof input.code !== "string" || typeof input.installId !== "string" || typeof input.label !== "string") {
      throw new LicenseError("INVALID_REQUEST", "Activation input is invalid.");
    }
    const label = input.label.trim();
    if (
      input.code.trim().length === 0 || input.code.length > MAX_CODE_LENGTH ||
      input.installId.trim().length === 0 || input.installId.length > MAX_INSTALL_ID_LENGTH ||
      label.length === 0 || label.length > MAX_LABEL_LENGTH
    ) {
      throw new LicenseError("INVALID_REQUEST", "Activation input is invalid.");
    }
    return { code: input.code, installId: input.installId, label };
  }

  private async requireActiveDevice(deviceToken: string) {
    const tokenHash = await this.tokenHash(deviceToken);
    const device = await this.repository.findDeviceByTokenHash(tokenHash);
    if (!device) throw new LicenseError("DEVICE_TOKEN_INVALID", "A valid Bearer device token is required.");
    if (device.status === "revoked") throw new LicenseError("DEVICE_REVOKED", "This device has been revoked.");
    return device;
  }

  private async tokenHash(deviceToken: string): Promise<string> {
    if (typeof deviceToken !== "string" || !/^[A-Za-z0-9_-]{43}$/.test(deviceToken)) {
      throw new LicenseError("DEVICE_TOKEN_INVALID", "A valid Bearer device token is required.");
    }
    return sha256Hex(deviceToken);
  }

  private async throwActivationFailure(codeHash: string, installHash: string, now: number): Promise<never> {
    if (await this.repository.findDeviceByInstallHash(installHash)) {
      throw new LicenseError("INSTALLATION_ALREADY_ACTIVATED", "This installation is already activated.");
    }
    const activation = await this.repository.findActivationCode(codeHash);
    if (activation?.status === "used") throw new LicenseError("ACTIVATION_CODE_USED", "This activation code has already been used.");
    if (!activation || activation.status === "revoked" || activation.expires_at <= now) {
      throw new LicenseError("ACTIVATION_CODE_INVALID", "This activation code is invalid or expired.");
    }
    throw new LicenseError("ACTIVATION_CODE_INVALID", "This activation code is invalid or expired.");
  }

  private now(): number {
    const value = this.clock();
    if (!Number.isSafeInteger(value)) throw new Error("Clock must return a safe integer timestamp");
    return value;
  }

  private async validateSigningKey(): Promise<void> {
    await crypto.subtle.importKey("jwk", this.privateJwk, { name: "ECDSA", namedCurve: "P-256" }, false, ["sign"]);
  }

  private lease(deviceId: string, issuedAt: number): Promise<string> {
    return signLease({ deviceId, issuedAt, expiresAt: issuedAt + LEASE_SECONDS, serverTime: issuedAt }, this.privateJwk);
  }
}
