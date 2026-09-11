import { createRemoteJWKSet, jwtVerify, type RemoteJWKSet } from "jose";

export interface VerifiedAccessIdentity { email: string }
export interface VerifiedServiceIdentity { serviceTokenId: string }
export interface AccessIdentityAdapter { verify(request: Request): Promise<VerifiedAccessIdentity | null> }
export interface ServiceIdentityAdapter { verify(request: Request): Promise<VerifiedServiceIdentity | null> }
export interface AccessJwtConfig { issuer: string; audience: string; jwksUrl: string }

const jwksByConfig = new Map<string, RemoteJWKSet>();
const MAX_ASSERTION = 16_384;
const MAX_IDENTITY = 320;

function trustedConfig(config: AccessJwtConfig): RemoteJWKSet | null {
  if (!config.issuer || !config.audience || !config.jwksUrl) return null;
  try {
    const issuer = new URL(config.issuer); const jwks = new URL(config.jwksUrl);
    if (issuer.protocol !== "https:" || jwks.protocol !== "https:" || issuer.origin !== jwks.origin || issuer.username || issuer.password || issuer.search || issuer.hash || jwks.username || jwks.password || jwks.search || jwks.hash || jwks.pathname !== "/cdn-cgi/access/certs") return null;
    const key = `${issuer.href}|${config.audience}|${jwks.href}`;
    let resolver = jwksByConfig.get(key);
    if (!resolver) { resolver = createRemoteJWKSet(jwks, { cacheMaxAge: 10 * 60_000, cooldownDuration: 30_000, timeoutDuration: 5_000 }); jwksByConfig.set(key, resolver); }
    return resolver;
  } catch { return null; }
}

async function verifiedClaims(request: Request, config: AccessJwtConfig): Promise<Record<string, unknown> | null> {
  const assertion = request.headers.get("cf-access-jwt-assertion"); const keys = trustedConfig(config);
  if (!keys || !assertion || assertion.length > MAX_ASSERTION) return null;
  try {
    const { payload } = await jwtVerify(assertion, keys, { issuer: config.issuer, audience: config.audience, algorithms: ["RS256"], requiredClaims: ["iss", "aud", "exp", "iat"], clockTolerance: 30 });
    const now = Math.floor(Date.now() / 1000); const exp = payload.exp; const iat = payload.iat; const nbf = payload.nbf;
    if (typeof exp !== "number" || typeof iat !== "number" || !Number.isSafeInteger(exp) || !Number.isSafeInteger(iat) || exp <= iat || iat > now + 30 || (nbf !== undefined && !Number.isSafeInteger(nbf))) return null;
    return payload as Record<string, unknown>;
  } catch { return null; }
}

function safeIdentity(value: unknown): string | null { return typeof value === "string" && value.length > 0 && value.length <= MAX_IDENTITY && !/[\r\n\0]/.test(value) ? value : null; }

/** Verifies signed Cloudflare Access user JWTs; ordinary identity headers are never trusted. */
export function createAccessIdentityAdapter(config: AccessJwtConfig): AccessIdentityAdapter {
  return { async verify(request) { const claims = await verifiedClaims(request, config); const email = safeIdentity(claims?.email); return email ? { email: email.toLowerCase() } : null; } };
}

/** Separate service-token trust boundary; Cloudflare Access puts its stable name in common_name. */
export function createServiceIdentityAdapter(config: AccessJwtConfig): ServiceIdentityAdapter {
  return { async verify(request) { const claims = await verifiedClaims(request, config); const serviceTokenId = safeIdentity(claims?.common_name); return serviceTokenId ? { serviceTokenId } : null; } };
}
