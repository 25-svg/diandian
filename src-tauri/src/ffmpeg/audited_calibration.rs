use serde::Serialize;
use std::fs::OpenOptions;
use std::io::{self, Write};
use std::path::Path;

pub(super) fn apply_calibration_if_audited<T, A, E>(
    original: T,
    calibrate: impl FnOnce(&mut T) -> A,
    persist_audit: impl FnOnce(&A) -> Result<(), E>,
) -> (T, Result<A, E>)
where
    T: Clone,
{
    let mut calibrated = original.clone();
    let audit = calibrate(&mut calibrated);
    match persist_audit(&audit) {
        Ok(()) => (calibrated, Ok(audit)),
        Err(error) => (original, Err(error)),
    }
}

pub(super) fn persist_json_durably(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let content = serde_json::to_vec_pretty(value)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let mut temporary_name = path.as_os_str().to_os_string();
    temporary_name.push(".tmp");
    let temporary_path = std::path::PathBuf::from(temporary_name);
    let result = (|| {
        let mut file = OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&temporary_path)?;
        file.write_all(&content)?;
        file.sync_all()?;
        std::fs::rename(&temporary_path, path)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(temporary_path);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Debug, PartialEq, Serialize)]
    struct TestAudit {
        change: &'static str,
    }

    #[test]
    fn audit_persistence_failure_returns_original_result() {
        let root = tempfile::tempdir().unwrap();
        let audit_path = root.path().join("missing").join("video.audit.json");

        let (result, audit) = apply_calibration_if_audited(
            "original".to_string(),
            |candidate| {
                *candidate = "calibrated".to_string();
                TestAudit { change: "entity" }
            },
            |audit| persist_json_durably(&audit_path, audit),
        );

        assert_eq!(result, "original");
        assert!(audit.is_err());
        assert!(!audit_path.exists());
    }

    #[test]
    fn audit_persistence_success_returns_calibrated_result_and_audit() {
        let root = tempfile::tempdir().unwrap();
        let audit_path = root.path().join("video.audit.json");

        let (result, audit) = apply_calibration_if_audited(
            "original".to_string(),
            |candidate| {
                *candidate = "calibrated".to_string();
                TestAudit { change: "entity" }
            },
            |audit| persist_json_durably(&audit_path, audit),
        );

        assert_eq!(result, "calibrated");
        assert_eq!(audit.unwrap(), TestAudit { change: "entity" });
        assert_eq!(
            std::fs::read_to_string(audit_path).unwrap(),
            "{\n  \"change\": \"entity\"\n}"
        );
    }
}
