use super::{
    model::{LicenseError, StoredCredential},
    security::{protect_for_current_windows_user, unprotect_for_current_windows_user},
};
use std::{
    io::{Read, Write},
    path::{Path, PathBuf},
};

const MAGIC: &[u8] = b"DIANDIAN-DEVICE-V1\0";
const MAX_VAULT_BYTES: u64 = 32_768;
pub struct CredentialVault {
    path: PathBuf,
}

impl CredentialVault {
    pub fn at(path: PathBuf) -> Self {
        Self { path }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn load(&self) -> Result<Option<StoredCredential>, LicenseError> {
        let file = match std::fs::File::open(&self.path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err(LicenseError::Vault),
        };
        let mut bytes = Vec::new();
        file.take(MAX_VAULT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| LicenseError::Vault)?;
        if bytes.len() as u64 > MAX_VAULT_BYTES {
            return Err(LicenseError::Vault);
        }
        let encrypted = bytes.strip_prefix(MAGIC).ok_or(LicenseError::Vault)?;
        let plain =
            unprotect_for_current_windows_user(encrypted).map_err(|_| LicenseError::Vault)?;
        if plain.len() as u64 > MAX_VAULT_BYTES {
            return Err(LicenseError::Vault);
        }
        let credential: StoredCredential =
            serde_json::from_slice(&plain).map_err(|_| LicenseError::Vault)?;
        credential.validate().map_err(|_| LicenseError::Vault)?;
        Ok(Some(credential))
    }
    pub fn store(&self, credential: &StoredCredential) -> Result<(), LicenseError> {
        credential.validate()?;
        let plain = serde_json::to_vec(credential).map_err(|_| LicenseError::Vault)?;
        if plain.len() as u64 > MAX_VAULT_BYTES {
            return Err(LicenseError::Vault);
        }
        // User-scoped DPAPI runs before any filesystem write; no plaintext temp.
        let encrypted =
            protect_for_current_windows_user(&plain).map_err(|_| LicenseError::Vault)?;
        if (encrypted.len() + MAGIC.len()) as u64 > MAX_VAULT_BYTES {
            return Err(LicenseError::Vault);
        }
        let parent = self
            .path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .ok_or(LicenseError::Vault)?;
        std::fs::create_dir_all(parent).map_err(|_| LicenseError::Vault)?;
        let mut temporary =
            tempfile::NamedTempFile::new_in(parent).map_err(|_| LicenseError::Vault)?;
        temporary
            .write_all(MAGIC)
            .and_then(|_| temporary.write_all(&encrypted))
            .map_err(|_| LicenseError::Vault)?;
        temporary
            .as_file()
            .sync_all()
            .map_err(|_| LicenseError::Vault)?;
        // `NamedTempFile::persist` uses rename semantics. Windows refuses to
        // rename over an existing file, so remove the old encrypted blob first.
        // If the replacement fails, the app fails closed (no usable credential)
        // and the caller can activate again; plaintext is never written.
        #[cfg(windows)]
        match std::fs::remove_file(&self.path) {
            Ok(()) => (),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
            Err(_) => return Err(LicenseError::Vault),
        }
        temporary
            .persist(&self.path)
            .map_err(|_| LicenseError::Vault)?;
        Ok(())
    }
    pub fn clear(&self) -> Result<(), LicenseError> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(LicenseError::Vault),
        }
    }
}
