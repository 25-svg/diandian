use super::model::{valid_text, valid_time, LicenseError, LicenseStatus, MAX_LEASE_BYTES};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ring::signature::{UnparsedPublicKey, ECDSA_P256_SHA256_FIXED};
use serde::{Deserialize, Serialize};

pub const LEASE_SECONDS: i64 = 604_800;

/// Field order is the signed JSON contract in control-plane/src/crypto.ts.
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LeaseClaims {
    pub device_id: String,
    pub issued_at: i64,
    pub expires_at: i64,
    pub server_time: i64,
}

/// Compiled key is base64url without padding of an uncompressed SEC1 P-256
/// public point (65 bytes, leading 0x04). It is separate from the updater key.
pub fn compiled_public_key() -> Result<Vec<u8>, LicenseError> {
    let encoded = option_env!("DIANDIAN_LEASE_PUBLIC_KEY").ok_or(LicenseError::Configuration)?;
    if encoded.len() != 87 {
        return Err(LicenseError::Configuration);
    }
    let key = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| LicenseError::Configuration)?;
    if key.len() != 65 || key[0] != 4 {
        return Err(LicenseError::Configuration);
    }
    Ok(key)
}

pub fn verify_lease(lease: &str, public_key: &[u8]) -> Result<LeaseClaims, LicenseError> {
    if lease.len() > MAX_LEASE_BYTES || public_key.len() != 65 || public_key.first() != Some(&4) {
        return Err(LicenseError::InvalidLease);
    }
    let (body, signature) = lease.split_once('.').ok_or(LicenseError::InvalidLease)?;
    let body = URL_SAFE_NO_PAD
        .decode(body)
        .map_err(|_| LicenseError::InvalidLease)?;
    let signature = URL_SAFE_NO_PAD
        .decode(signature)
        .map_err(|_| LicenseError::InvalidLease)?;
    if signature.len() != 64 {
        return Err(LicenseError::InvalidLease);
    }
    // Both mathematically valid low-S and high-S signatures are accepted, exactly
    // like WebCrypto; the signature itself is never used as an object identity.
    UnparsedPublicKey::new(&ECDSA_P256_SHA256_FIXED, public_key)
        .verify(&body, &signature)
        .map_err(|_| LicenseError::InvalidLease)?;
    let claims: LeaseClaims =
        serde_json::from_slice(&body).map_err(|_| LicenseError::InvalidLease)?;
    if !valid_text(&claims.device_id, 256)
        || !valid_time(claims.issued_at)
        || !valid_time(claims.expires_at)
        || claims.server_time != claims.issued_at
        || claims.issued_at.checked_add(LEASE_SECONDS) != Some(claims.expires_at)
    {
        return Err(LicenseError::InvalidLease);
    }
    // Reject duplicate/unknown claims (serde above), alternate number spelling,
    // ordering, whitespace and escaping. No wall clock enters the signed bytes.
    if serde_json::to_vec(&claims).map_err(|_| LicenseError::InvalidLease)? != body {
        return Err(LicenseError::InvalidLease);
    }
    Ok(claims)
}

pub fn evaluate_lease_at(
    lease: &str,
    public_key: &[u8],
    now: i64,
    last_server_time: i64,
) -> Result<LicenseStatus, LicenseError> {
    if !valid_time(now) || !valid_time(last_server_time) {
        return Err(LicenseError::InvalidInput);
    }
    let claims = verify_lease(lease, public_key)?;
    if now < last_server_time.max(claims.server_time) {
        return Ok(LicenseStatus::ClockInvalid);
    }
    // expiresAt IS offlineUntil. There is no extra grace period beyond seven days.
    if now >= claims.expires_at {
        return Ok(LicenseStatus::Expired);
    }
    if now == last_server_time.max(claims.server_time) {
        return Ok(LicenseStatus::Valid);
    }
    Ok(LicenseStatus::OfflineGrace)
}

/// `revoked` may only be set from a successfully authenticated server response;
/// callers must persist that decision so network failure cannot restore access.
pub fn evaluate_status_at(
    lease: Option<&str>,
    public_key: &[u8],
    now: i64,
    last_server_time: i64,
    revoked: bool,
) -> Result<LicenseStatus, LicenseError> {
    if revoked {
        return Ok(LicenseStatus::Revoked);
    }
    match lease {
        Some(lease) => evaluate_lease_at(lease, public_key, now, last_server_time),
        None => Ok(LicenseStatus::Unactivated),
    }
}
