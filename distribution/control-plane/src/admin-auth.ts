import { createRemoteJWKSet, jwtVerify, type RemoteJWKSet } from "jose";

export interface VerifiedAccessIdentity { email: string }

export interface AccessIdentityAdapter {
  verify(request: Request): Promise<VerifiedAccessIdentity | null>;
}

export interface AccessJwtConfig {
  issuer: string;
  audience: string;
  jwksUrl: string;
}

const jwksByConfig = new Map<string, RemoteJWKSet>();

function remoteKeys(config: AccessJwtConfig): RemoteJWKSet | null {
  if (!config.issuer || !config.audience || !config.jwksUrl) return null;
  try {
    const issuer = new URL(config.issuer);
    const jwksUrl = new URL(config.jwksUrl);
    if (issuer.protocol !== "https:" || jwksUrl.protocol !== "https:") return null;
    const key = `${issuer.origin}|${config.audience}|${jwksUrl.href}`;
    let keys = jwksByConfig.get(key);
    if (!keys) {
      keys = createRemoteJWKSet(jwksUrl, { cacheMaxAge: 10 * 60_000, cooldownDuration: 0, timeoutDuration: 5_000 });
      jwksByConfig.set(key, keys);
    }
    return keys;
  } catch {
    return null;
  }
}

/**
 * The only production bridge from an Access request to application identity.
 * It verifies the signed assertion before returning an email; the convenience
 * Access email header is deliberately never read.
 */
export function createAccessIdentityAdapter(config: AccessJwtConfig): AccessIdentityAdapter {
  const keys = remoteKeys(config);
  return {
    async verify(request): Promise<VerifiedAccessIdentity | null> {
      const assertion = request.headers.get("cf-access-jwt-assertion");
      if (!keys || !assertion || assertion.length > 16_384) return null;
      try {
        const { payload } = await jwtVerify(assertion, keys, {
          issuer: config.issuer,
          audience: config.audience,
          algorithms: ["RS256", "ES256"],
          clockTolerance: 30,
        });
        const email = payload.email;
        if (typeof email !== "string" || email.length === 0 || email.length > 320 || /[\r\n\0]/.test(email)) return null;
        return { email: email.toLowerCase() };
      } catch {
        // Authentication errors, unknown kids, and JWKS fetch errors all fail closed.
        return null;
      }
    },
  };
}
