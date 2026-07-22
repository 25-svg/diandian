use cap_std::ambient_authority;
use cap_std::fs::{Dir, OpenOptions};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Component, Path};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WriteReceipt {
    pub relative_path: String,
    pub byte_len: u64,
    pub sha256: String,
}

pub async fn write_verified_markdown(
    vault: &Path,
    relative_path: &Path,
    expected_absent: bool,
    content: &str,
) -> Result<WriteReceipt, String> {
    let vault = vault.to_path_buf();
    let relative_path = relative_path.to_path_buf();
    let content = content.to_owned();
    tokio::task::spawn_blocking(move || {
        write_verified_markdown_blocking(&vault, &relative_path, expected_absent, &content)
    })
    .await
    .map_err(|error| error.to_string())?
}

fn write_verified_markdown_blocking(
    vault: &Path,
    relative_path: &Path,
    expected_absent: bool,
    content: &str,
) -> Result<WriteReceipt, String> {
    let components = normal_components(relative_path)?;
    let destination = components.last().ok_or("目标路径无效")?.clone();
    let vault = Dir::open_ambient_dir(vault, ambient_authority()).map_err(format_io_error)?;
    let parent = open_or_create_parent(vault, &components[..components.len() - 1])
        .map_err(format_io_error)?;
    let temporary = OsString::from(format!(".{}.tmp", uuid::Uuid::new_v4()));
    let mut published = false;

    let result = (|| {
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        let mut file = parent
            .open_with(Path::new(&temporary), &options)
            .map_err(format_io_error)?;
        file.write_all(content.as_bytes())
            .map_err(format_io_error)?;
        file.sync_all().map_err(format_io_error)?;
        drop(file);

        if expected_absent {
            parent
                .hard_link(Path::new(&temporary), &parent, Path::new(&destination))
                .map_err(|error| {
                    if error.kind() == io::ErrorKind::AlreadyExists {
                        "目标母稿版本已经存在".to_string()
                    } else {
                        format_io_error(error)
                    }
                })?;
            published = true;
            parent
                .remove_file(Path::new(&temporary))
                .map_err(format_io_error)?;
        } else {
            parent
                .rename(Path::new(&temporary), &parent, Path::new(&destination))
                .map_err(format_io_error)?;
            published = true;
        }

        let read_back = parent
            .read(Path::new(&destination))
            .map_err(format_io_error)?;
        if read_back != content.as_bytes() {
            return Err("母稿写入校验失败".into());
        }

        Ok(WriteReceipt {
            relative_path: normalized_relative_path(relative_path),
            byte_len: read_back.len() as u64,
            sha256: format!("{:x}", Sha256::digest(&read_back)),
        })
    })();

    if result.is_err() {
        let _ = parent.remove_file(Path::new(&temporary));
        if published {
            let _ = parent.remove_file(Path::new(&destination));
        }
    }
    result
}

fn normal_components(path: &Path) -> Result<Vec<OsString>, String> {
    if path.as_os_str().is_empty() || path.is_absolute() {
        return Err("目标路径无效".into());
    }
    path.components()
        .map(|component| match component {
            Component::Normal(value) => Ok(value.to_os_string()),
            _ => Err("目标路径超出知识库".into()),
        })
        .collect()
}

fn open_or_create_parent(mut parent: Dir, components: &[OsString]) -> io::Result<Dir> {
    for component in components {
        let component = Path::new(component);
        parent = match parent.open_dir(component) {
            Ok(child) => child,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                match parent.create_dir(component) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error),
                }
                parent.open_dir(component)?
            }
            Err(error) => return Err(error),
        };
    }
    Ok(parent)
}

fn format_io_error(error: io::Error) -> String {
    error.to_string()
}

fn normalized_relative_path(path: &Path) -> String {
    path.components()
        .map(|component| component.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}
