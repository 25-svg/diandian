use serde::Serialize;
use std::{collections::HashSet, path::Path};

const CONTEXT_PADDING_MS: u64 = 30_000;
const MAX_CONTEXT_ITEMS: usize = 80;
const MAX_MESSAGE_CHARS: usize = 120;
const MAX_CONTEXT_CHARS: usize = 3_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DanmuEntry {
    pub timestamp_ms: u64,
    pub text: String,
}

#[derive(Debug, Clone)]
pub struct DanmuTimeline {
    base_timestamp_ms: u64,
    entries: Vec<DanmuEntry>,
}

#[derive(Debug, Clone)]
pub struct ChunkContext {
    pub payload: String,
    pub evidence_count: usize,
}

#[derive(Serialize)]
struct DialogContext<'a> {
    context_type: &'static str,
    context_data: Vec<ContextItem<'a>>,
}

#[derive(Serialize)]
struct ContextItem<'a> {
    text: &'a str,
}

impl DanmuTimeline {
    #[cfg(test)]
    pub(crate) fn from_test_entries(base_timestamp_ms: u64, entries: Vec<DanmuEntry>) -> Self {
        Self {
            base_timestamp_ms,
            entries,
        }
    }

    pub fn load_for_media(media_path: &Path) -> Result<Option<Self>, String> {
        let parent = match media_path.parent() {
            Some(parent) => parent,
            None => return Ok(None),
        };
        let danmu_path = parent.join("danmu.txt");
        if !danmu_path.is_file() {
            return Ok(None);
        }
        let content = std::fs::read_to_string(&danmu_path)
            .map_err(|error| format!("读取动态ASR弹幕上下文失败: {error}"))?;
        let entries = parse_danmu(&content);
        if entries.is_empty() {
            return Ok(None);
        }
        let base_timestamp_ms = parent
            .file_name()
            .and_then(|value| value.to_str())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(entries[0].timestamp_ms);
        Ok(Some(Self {
            base_timestamp_ms,
            entries,
        }))
    }

    pub fn for_chunk(&self, start_offset_ms: u64, duration_ms: u64) -> Option<ChunkContext> {
        let start = self
            .base_timestamp_ms
            .saturating_add(start_offset_ms)
            .saturating_sub(CONTEXT_PADDING_MS);
        let end = self
            .base_timestamp_ms
            .saturating_add(start_offset_ms)
            .saturating_add(duration_ms)
            .saturating_add(CONTEXT_PADDING_MS);
        let mut seen = HashSet::new();
        let mut candidates = self
            .entries
            .iter()
            .filter(|entry| entry.timestamp_ms >= start && entry.timestamp_ms <= end)
            .filter_map(|entry| {
                let text = sanitize_message(&entry.text);
                if text.is_empty() || !seen.insert(text.clone()) {
                    return None;
                }
                Some((context_priority(&text), entry.timestamp_ms, text))
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return None;
        }
        candidates.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| right.1.cmp(&left.1)));
        candidates.truncate(MAX_CONTEXT_ITEMS);
        let mut used_chars: usize = 0;
        candidates.retain(|(_, _, text)| {
            let text_chars = text.chars().count();
            if used_chars.saturating_add(text_chars) > MAX_CONTEXT_CHARS {
                return false;
            }
            used_chars += text_chars;
            true
        });
        // Fire's dialog context expects newer evidence before older evidence.
        candidates.sort_by(|left, right| right.1.cmp(&left.1));
        let texts = candidates
            .iter()
            .map(|(_, _, text)| text.as_str())
            .collect::<Vec<_>>();
        let payload = serde_json::to_string(&DialogContext {
            context_type: "dialog_ctx",
            context_data: texts.iter().map(|text| ContextItem { text }).collect(),
        })
        .ok()?;
        Some(ChunkContext {
            payload,
            evidence_count: texts.len(),
        })
    }

    pub fn nearby(
        &self,
        start_offset_ms: u64,
        end_offset_ms: u64,
        window_ms: u64,
    ) -> Vec<&DanmuEntry> {
        let start = self
            .base_timestamp_ms
            .saturating_add(start_offset_ms)
            .saturating_sub(window_ms);
        let end = self
            .base_timestamp_ms
            .saturating_add(end_offset_ms)
            .saturating_add(window_ms);
        self.entries
            .iter()
            .filter(|entry| entry.timestamp_ms >= start && entry.timestamp_ms <= end)
            .collect()
    }
}

fn parse_danmu(content: &str) -> Vec<DanmuEntry> {
    let mut entries = content
        .lines()
        .filter_map(|line| {
            let (timestamp, text) = line.split_once(':')?;
            let timestamp_ms = timestamp.trim().parse::<u64>().ok()?;
            let text = text.trim();
            (!text.is_empty()).then(|| DanmuEntry {
                timestamp_ms,
                text: text.to_string(),
            })
        })
        .collect::<Vec<_>>();
    entries.sort_by_key(|entry| entry.timestamp_ms);
    entries
}

fn sanitize_message(value: &str) -> String {
    value
        .chars()
        .filter(|character| !character.is_control())
        .take(MAX_MESSAGE_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

fn context_priority(value: &str) -> u8 {
    let has_ascii_entity = value
        .chars()
        .any(|character| character.is_ascii_alphanumeric());
    let has_commerce_entity = [
        "成色", "现货", "价格", "链接", "镜头", "机身", "卡口", "几新", "口",
    ]
    .iter()
    .any(|keyword| value.contains(keyword));
    u8::from(has_ascii_entity) * 2 + u8::from(has_commerce_entity)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_aligns_chunk_context() {
        let entries =
            parse_danmu("1000000:A7M4有准新嘛\n1010000:99新有现货嘛\n1020000:看下成色\ninvalid\n");
        let timeline = DanmuTimeline {
            base_timestamp_ms: 1_000_000,
            entries,
        };
        let context = timeline.for_chunk(0, 60_000).expect("context");
        let json: serde_json::Value = serde_json::from_str(&context.payload).expect("valid json");
        assert_eq!(json["context_type"], "dialog_ctx");
        assert_eq!(context.evidence_count, 3);
        assert_eq!(json["context_data"][0]["text"], "看下成色");
        assert_eq!(json["context_data"][2]["text"], "A7M4有准新嘛");
    }

    #[test]
    fn ignores_messages_outside_the_chunk_window() {
        let timeline = DanmuTimeline {
            base_timestamp_ms: 1_000_000,
            entries: vec![DanmuEntry {
                timestamp_ms: 2_000_000,
                text: "不相关的下一段弹幕".to_string(),
            }],
        };
        assert!(timeline.for_chunk(0, 60_000).is_none());
    }
}
