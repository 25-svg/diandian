use std::path::Path;

fn clear_readonly(path: &Path) -> std::io::Result<()> {
    let metadata = std::fs::metadata(path)?;
    let mut perms = metadata.permissions();
    if perms.readonly() {
        perms.set_readonly(false);
        std::fs::set_permissions(path, perms)?;
    }
    Ok(())
}

fn is_dir_not_empty(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::DirectoryNotEmpty || error.raw_os_error() == Some(145)
}

fn is_missing(error: &std::io::Error) -> bool {
    error.kind() == std::io::ErrorKind::NotFound
}

/// Bottom-up directory removal that clears read-only bits and tolerates NAS/SMB quirks.
pub fn remove_dir_all_robust(path: &Path) -> std::io::Result<()> {
    if !path.exists() {
        return Ok(());
    }

    if path.is_file() {
        let _ = clear_readonly(path);
        return std::fs::remove_file(path);
    }

    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        remove_dir_all_robust(&entry.path())?;
    }

    let _ = clear_readonly(path);
    match std::fs::remove_dir(path) {
        Ok(()) => Ok(()),
        Err(error) if is_dir_not_empty(&error) => {
            if path.exists() {
                std::thread::sleep(std::time::Duration::from_millis(100));
                remove_dir_all_robust(path)
            } else {
                Ok(())
            }
        }
        Err(error) => Err(error),
    }
}

#[cfg(windows)]
fn remove_dir_windows_cmd(path: &Path) -> std::io::Result<()> {
    use std::os::windows::process::CommandExt;
    use std::process::Command;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let path_str = path.to_string_lossy();
    let status = Command::new("cmd")
        .creation_flags(CREATE_NO_WINDOW)
        .args(["/C", "rmdir", "/S", "/Q", &path_str])
        .status()?;

    if status.success() || !path.exists() {
        Ok(())
    } else {
        Err(std::io::Error::other(format!(
            "rmdir failed with status {status}"
        )))
    }
}

#[cfg(not(windows))]
fn remove_dir_windows_cmd(_path: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other("windows-only delete fallback"))
}

pub fn remove_dir_all_with_fallbacks(path: &Path) -> std::io::Result<()> {
    match remove_dir_all_robust(path) {
        Ok(()) => Ok(()),
        Err(error) if is_missing(&error) => Ok(()),
        Err(first_error) => {
            if path.exists() {
                if let Ok(()) = remove_dir_windows_cmd(path) {
                    return Ok(());
                }
                if !path.exists() {
                    return Ok(());
                }
            } else {
                return Ok(());
            }
            Err(first_error)
        }
    }
}

pub fn archive_cache_delete_hint(path: &Path) -> &'static str {
    let path_text = path.to_string_lossy();
    if path_text.contains("#recycle") {
        "该路径位于 NAS 回收站 (#recycle)。请在 File Station 清空回收站，或将缓存目录改到正常共享文件夹（不要选 #recycle）。"
    } else if path_text.starts_with(r"\\") || (path_text.contains(":\\") && path_text.len() > 2) {
        "若为 NAS/网络盘，请确认没有播放器占用该录播，并在 File Station 检查是否有隐藏文件（如 @eaDir）。"
    } else {
        "请关闭正在播放该录播的窗口后重试。"
    }
}

#[cfg(test)]
mod tests {
    use super::{archive_cache_delete_hint, remove_dir_all_with_fallbacks};
    use std::path::PathBuf;

    #[test]
    fn recycle_path_hint_mentions_nas_recycle() {
        let path = PathBuf::from(r"Z:\#recycle\douyin\room\live");
        let hint = archive_cache_delete_hint(&path);
        assert!(hint.contains("#recycle"));
    }

    #[test]
    fn removes_nested_directory_tree() {
        let root =
            std::env::temp_dir().join(format!("shadowreplay-fs-util-{}", uuid::Uuid::new_v4()));
        let nested = root.join("a").join("b");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("segment.ts"), b"data").unwrap();

        remove_dir_all_with_fallbacks(&root).unwrap();
        assert!(!root.exists());
    }

    #[test]
    fn missing_directory_is_already_deleted() {
        let path = std::env::temp_dir().join(format!(
            "shadowreplay-fs-util-missing-{}",
            uuid::Uuid::new_v4()
        ));

        assert!(remove_dir_all_with_fallbacks(&path).is_ok());
    }
}
