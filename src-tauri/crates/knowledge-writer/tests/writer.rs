use knowledge_writer::write_verified_markdown;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;
use std::sync::Arc;

fn temporary_files(parent: &Path) -> Vec<String> {
    fs::read_dir(parent)
        .unwrap()
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with('.') && name.ends_with(".tmp"))
        .collect()
}

#[tokio::test]
async fn writes_utf8_with_atomic_readback_and_sha256_receipt() {
    let vault = tempfile::tempdir().unwrap();
    let content = "# 企业母稿\n\n佳能小白兔参数。\n";

    let receipt = write_verified_markdown(
        vault.path(),
        Path::new("10-企业母稿/MS-001/V1.0.md"),
        true,
        content,
    )
    .await
    .unwrap();

    let destination = vault.path().join("10-企业母稿/MS-001/V1.0.md");
    assert_eq!(fs::read(&destination).unwrap(), content.as_bytes());
    assert_eq!(receipt.relative_path, "10-企业母稿/MS-001/V1.0.md");
    assert_eq!(receipt.byte_len, content.len() as u64);
    assert_eq!(
        receipt.sha256,
        format!("{:x}", Sha256::digest(content.as_bytes()))
    );
    assert!(temporary_files(destination.parent().unwrap()).is_empty());
}

#[tokio::test]
async fn rejects_traversal_and_absolute_paths() {
    let vault = tempfile::tempdir().unwrap();
    let outside = vault.path().parent().unwrap().join("outside.md");

    assert!(
        write_verified_markdown(vault.path(), Path::new("../outside.md"), true, "x")
            .await
            .is_err()
    );
    assert!(write_verified_markdown(vault.path(), &outside, true, "x")
        .await
        .is_err());
    assert!(!outside.exists());
}

#[cfg(unix)]
#[tokio::test]
async fn rejects_symlink_escape() {
    let vault = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::os::unix::fs::symlink(outside.path(), vault.path().join("escape")).unwrap();

    assert!(
        write_verified_markdown(vault.path(), Path::new("escape/out.md"), true, "x")
            .await
            .is_err()
    );
    assert!(!outside.path().join("out.md").exists());
}

#[cfg(windows)]
#[tokio::test]
async fn rejects_symlink_escape() {
    let vault = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let link = vault.path().join("escape");
    let output = std::process::Command::new("cmd")
        .args(["/C", "mklink", "/J"])
        .arg(&link)
        .arg(outside.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "junction creation failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    assert!(
        write_verified_markdown(vault.path(), Path::new("escape/out.md"), true, "x")
            .await
            .is_err()
    );
    assert!(!outside.path().join("out.md").exists());
}

#[tokio::test]
async fn expected_absent_refuses_to_overwrite() {
    let vault = tempfile::tempdir().unwrap();
    let destination = vault.path().join("10-企业母稿/existing.md");
    fs::create_dir_all(destination.parent().unwrap()).unwrap();
    fs::write(&destination, "original").unwrap();

    assert!(write_verified_markdown(
        vault.path(),
        Path::new("10-企业母稿/existing.md"),
        true,
        "replacement",
    )
    .await
    .is_err());
    assert_eq!(fs::read_to_string(&destination).unwrap(), "original");
    assert!(temporary_files(destination.parent().unwrap()).is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn concurrent_expected_absent_has_exactly_one_winner_without_temp_files() {
    let vault = tempfile::tempdir().unwrap();
    let relative = Path::new("10-企业母稿/concurrent.md");
    let barrier = Arc::new(tokio::sync::Barrier::new(16));
    let mut tasks = Vec::new();

    for index in 0..16 {
        let vault = vault.path().to_path_buf();
        let barrier = Arc::clone(&barrier);
        tasks.push(tokio::spawn(async move {
            let content = format!("writer-{index}");
            barrier.wait().await;
            let result = write_verified_markdown(
                &vault,
                Path::new("10-企业母稿/concurrent.md"),
                true,
                &content,
            )
            .await;
            (content, result)
        }));
    }

    let mut winners = Vec::new();
    for task in tasks {
        let (content, result) = task.await.unwrap();
        if result.is_ok() {
            winners.push(content);
        }
    }

    assert_eq!(winners.len(), 1);
    let destination = vault.path().join(relative);
    assert_eq!(fs::read_to_string(&destination).unwrap(), winners[0]);
    assert!(temporary_files(destination.parent().unwrap()).is_empty());
}

#[tokio::test]
async fn removes_temporary_file_when_atomic_rename_fails() {
    let vault = tempfile::tempdir().unwrap();
    let destination = vault.path().join("10-企业母稿/blocked.md");
    fs::create_dir_all(&destination).unwrap();

    assert!(write_verified_markdown(
        vault.path(),
        Path::new("10-企业母稿/blocked.md"),
        false,
        "content",
    )
    .await
    .is_err());
    assert!(temporary_files(destination.parent().unwrap()).is_empty());
}
