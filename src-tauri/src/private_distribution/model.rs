use serde::{Deserialize, Serialize};

pub const MAX_LEASE_BYTES: usize = 4096;
pub const MAX_SAFE_TIME: i64 = 9_007_199_254_740_991;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LicenseStatus {
    Unactivated,
    Valid,
    OfflineGrace,
    Expired,
    Revoked,
    ClockInvalid,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LicenseError {
    #[error("License configuration is unavailable")]
    Configuration,
    #[error("License data is invalid")]
    InvalidLease,
    #[error("License input is invalid")]
    InvalidInput,
    #[error("Secure credential storage is unavailable")]
    Vault,
    #[error("License service is unavailable")]
    Unavailable,
    #[error("License response is invalid")]
    InvalidResponse,
    #[error("Device authorization was revoked")]
    Revoked,
    #[error("Device credential is invalid")]
    InvalidToken,
    #[error("Activation code is invalid or expired")]
    InvalidCode,
    #[error("Activation code was already used")]
    CodeUsed,
    #[error("Installation is already activated")]
    AlreadyActivated,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredCredential {
    pub install_id: String,
    pub device_token: String,
    pub lease: String,
    pub last_server_time: i64,
}

impl std::fmt::Debug for StoredCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("StoredCredential([REDACTED])")
    }
}

pub(crate) fn valid_token(value: &str) -> bool {
    value.len() == 43
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}
pub(crate) fn valid_text(value: &str, max: usize) -> bool {
    !value.trim().is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}
pub(crate) fn valid_time(value: i64) -> bool {
    (0..=MAX_SAFE_TIME).contains(&value)
}
impl StoredCredential {
    pub(crate) fn validate(&self) -> Result<(), LicenseError> {
        if !valid_text(&self.install_id, 256)
            || !valid_token(&self.device_token)
            || !valid_text(&self.lease, MAX_LEASE_BYTES)
            || !valid_time(self.last_server_time)
        {
            return Err(LicenseError::InvalidInput);
        }
        Ok(())
    }
}
