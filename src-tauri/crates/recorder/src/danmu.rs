use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use tokio::io::AsyncWriteExt;
use tokio::{
    fs::{File, OpenOptions},
    io::{AsyncBufReadExt, BufReader},
    sync::RwLock,
};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct DanmuEntry {
    pub ts: i64,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_name: Option<String>,
}

pub struct DanmuStorage {
    cache: RwLock<Vec<DanmuEntry>>,
    file: RwLock<File>,
}

impl DanmuStorage {
    pub async fn new(file_path: &PathBuf) -> Option<DanmuStorage> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(file_path)
            .await;
        if file.is_err() {
            log::error!("Open danmu file failed: {}", file.err().unwrap());
            return None;
        }
        let file = file.unwrap();
        let reader = BufReader::new(file);
        let mut lines = reader.lines();
        let mut preload_cache: Vec<DanmuEntry> = Vec::new();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(entry) = parse_danmu_line(&line) {
                preload_cache.push(entry);
            }
        }
        let file = OpenOptions::new()
            .append(true)
            .create(true)
            .open(file_path)
            .await
            .expect("create danmu.txt failed");
        Some(DanmuStorage {
            cache: RwLock::new(preload_cache),
            file: RwLock::new(file),
        })
    }

    pub async fn add_line(
        &self,
        ts: i64,
        content: &str,
        user_id: Option<&str>,
        user_name: Option<&str>,
    ) {
        let entry = DanmuEntry {
            ts,
            content: content.to_string(),
            user_id: normalize_optional_field(user_id),
            user_name: normalize_optional_field(user_name),
        };
        self.cache.write().await.push(entry.clone());
        let line = serde_json::to_string(&entry)
            .unwrap_or_else(|_| format!("{ts}:{content}"));
        let _ = self
            .file
            .write()
            .await
            .write(format!("{line}\n").as_bytes())
            .await;
    }

    // get entries with ts relative to live start time
    pub async fn get_entries(&self, live_start_ts: i64) -> Vec<DanmuEntry> {
        let mut danmus: Vec<DanmuEntry> = self
            .cache
            .read()
            .await
            .iter()
            .map(|entry| DanmuEntry {
                ts: entry.ts - live_start_ts,
                content: entry.content.clone(),
                user_id: entry.user_id.clone(),
                user_name: entry.user_name.clone(),
            })
            .collect();
        // filter out danmus with ts < 0
        danmus.retain(|entry| entry.ts >= 0);
        danmus
    }
}

fn normalize_optional_field(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "0")
        .map(str::to_string)
}

pub fn parse_danmu_line(line: &str) -> Option<DanmuEntry> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    if line.starts_with('{') {
        return serde_json::from_str::<DanmuEntry>(line).ok().and_then(|mut entry| {
            entry.content = entry.content.trim().to_string();
            if entry.content.is_empty() {
                return None;
            }
            entry.user_id = normalize_optional_field(entry.user_id.as_deref());
            entry.user_name = normalize_optional_field(entry.user_name.as_deref());
            Some(entry)
        });
    }
    let (ts_str, content) = line.split_once(':')?;
    let ts = ts_str.trim().parse::<i64>().ok()?;
    let content = content.trim();
    if content.is_empty() {
        return None;
    }
    Some(DanmuEntry {
        ts,
        content: content.to_string(),
        user_id: None,
        user_name: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_legacy_line() {
        let entry = parse_danmu_line("1710000000000:有人吗:求链接").unwrap();
        assert_eq!(entry.ts, 1710000000000);
        assert_eq!(entry.content, "有人吗:求链接");
        assert_eq!(entry.user_id, None);
        assert_eq!(entry.user_name, None);
    }

    #[test]
    fn parse_json_line_with_speaker() {
        let entry = parse_danmu_line(
            r#"{"ts":1710000000001,"content":"多少钱","user_id":"12345","user_name":"小明"}"#,
        )
        .unwrap();
        assert_eq!(entry.ts, 1710000000001);
        assert_eq!(entry.content, "多少钱");
        assert_eq!(entry.user_id.as_deref(), Some("12345"));
        assert_eq!(entry.user_name.as_deref(), Some("小明"));
    }

    #[test]
    fn skip_empty_content() {
        assert!(parse_danmu_line("123:").is_none());
        assert!(parse_danmu_line(r#"{"ts":1,"content":"  "}"#).is_none());
    }
}
