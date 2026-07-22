use regex::Regex;
use serde::Serialize;

use super::{asr_context::DanmuTimeline, GenerateResult};

const EVIDENCE_WINDOW_MS: u64 = 20_000;
const CALIBRATION_VERSION: &str = "evidence-v1";

#[derive(Debug, Default, Serialize)]
pub struct CalibrationAudit {
    pub version: &'static str,
    pub changes: Vec<CalibrationChange>,
    pub quality_flags: Vec<QualityFlag>,
}

#[derive(Debug, Serialize)]
pub struct CalibrationChange {
    pub subtitle_position: usize,
    pub start_ms: u64,
    pub original: String,
    pub corrected: String,
    pub evidence: String,
    pub evidence_timestamp_ms: u64,
    pub confidence: f32,
    pub rule: &'static str,
}

#[derive(Debug, Serialize)]
pub struct QualityFlag {
    pub subtitle_position: usize,
    pub start_ms: u64,
    pub text: String,
    pub reasons: Vec<&'static str>,
    pub nearby_evidence: Vec<String>,
}

pub fn calibrate_with_danmu(
    result: &mut GenerateResult,
    timeline: &DanmuTimeline,
) -> CalibrationAudit {
    let mut audit = CalibrationAudit {
        version: CALIBRATION_VERSION,
        ..Default::default()
    };
    let candidate_regex = Regex::new(r"(?i)[a-z]+(?:-?[a-z0-9]+)+|[0-9]{2,}(?:-[0-9]{2,})?新?")
        .expect("valid evidence entity regex");
    let numeric_run = Regex::new(r"\d{4,}").expect("valid numeric regex");
    let mixed_entity = Regex::new(r"(?i)\d+[a-z]|[a-z]+\d+").expect("valid mixed regex");
    let spaced_entity =
        Regex::new(r"(?i)(?:[a-z0-9]\s+){2,}[a-z0-9]").expect("valid spaced entity regex");

    for item in &mut result.subtitle_content {
        let start_ms = time_to_ms(&item.start_time);
        let end_ms = time_to_ms(&item.end_time);
        let nearby = timeline.nearby(start_ms, end_ms, EVIDENCE_WINDOW_MS);
        let original = item.text.clone();
        let mut corrected = original.clone();

        for evidence in &nearby {
            for matched in candidate_regex.find_iter(&evidence.text) {
                let candidate = matched.as_str();
                if candidate.chars().count() < 2 || !is_safe_evidence_candidate(candidate) {
                    continue;
                }
                let pattern = spoken_entity_pattern(candidate);
                let Ok(pattern) = Regex::new(&pattern) else {
                    continue;
                };
                let surface_range = pattern
                    .find_iter(&corrected)
                    .find(|surface| has_safe_entity_boundaries(&corrected, surface.range()))
                    .map(|surface| surface.range());
                if let Some(surface_range) = surface_range {
                    if &corrected[surface_range.clone()] == candidate {
                        continue;
                    }
                    let before = corrected.clone();
                    corrected.replace_range(surface_range, candidate);
                    audit.changes.push(CalibrationChange {
                        subtitle_position: item.pos,
                        start_ms,
                        original: before,
                        corrected: corrected.clone(),
                        evidence: evidence.text.clone(),
                        evidence_timestamp_ms: evidence.timestamp_ms,
                        confidence: 0.99,
                        rule: "nearby_danmu_entity_equivalence",
                    });
                }
            }
        }
        item.text = corrected;

        let mut reasons = Vec::new();
        if numeric_run.is_match(&item.text) {
            reasons.push("long_numeric_entity");
        }
        if mixed_entity.is_match(&item.text) {
            reasons.push("mixed_alphanumeric_entity");
        }
        if spaced_entity.is_match(&item.text) {
            reasons.push("spaced_alphanumeric_entity");
        }
        if !reasons.is_empty() {
            audit.quality_flags.push(QualityFlag {
                subtitle_position: item.pos,
                start_ms,
                text: item.text.clone(),
                reasons,
                nearby_evidence: nearby
                    .iter()
                    .take(8)
                    .map(|entry| entry.text.clone())
                    .collect(),
            });
        }
    }
    audit
}

fn is_safe_evidence_candidate(candidate: &str) -> bool {
    let numeric = candidate.strip_suffix('新').unwrap_or(candidate);
    if numeric.chars().all(|character| character.is_ascii_digit()) {
        return candidate.ends_with('新') || numeric.chars().count() >= 3;
    }
    true
}

fn has_safe_entity_boundaries(text: &str, range: std::ops::Range<usize>) -> bool {
    let before = text[..range.start].chars().next_back();
    let after = text[range.end..].chars().next();
    !before.is_some_and(|character| character.is_ascii_alphanumeric())
        && !after.is_some_and(|character| character.is_ascii_alphanumeric())
}

fn spoken_entity_pattern(candidate: &str) -> String {
    let mut pattern = String::from("(?i)");
    let mut first = true;
    for character in candidate.chars() {
        if character == '-' {
            pattern.push_str(r"(?:\s*[-杠]\s*|\s*)");
            continue;
        }
        if !first {
            pattern.push_str(r"[\s,，。]*");
        }
        first = false;
        match character {
            '0' => pattern.push_str("[0零〇]"),
            '1' => pattern.push_str("[1一幺]"),
            '2' => pattern.push_str("[2二两]"),
            '3' => pattern.push_str("[3三]"),
            '4' => pattern.push_str("[4四]"),
            '5' => pattern.push_str("[5五]"),
            '6' => pattern.push_str("[6六]"),
            '7' => pattern.push_str("[7七]"),
            '8' => pattern.push_str("[8八]"),
            '9' => pattern.push_str("[9九]"),
            value if value.is_ascii_alphabetic() => {
                pattern.push_str(&regex::escape(&value.to_string()))
            }
            value => pattern.push_str(&regex::escape(&value.to_string())),
        }
    }
    pattern
}

fn time_to_ms(time: &srtparse::Time) -> u64 {
    (((time.hours * 60 + time.minutes) * 60 + time.seconds) * 1000) + time.milliseconds
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subtitle_generator::{asr_context::DanmuEntry, SubtitleGeneratorType};

    #[test]
    fn normalizes_model_only_when_nearby_danmu_proves_it() {
        let timeline = DanmuTimeline::from_test_entries(
            1_000_000,
            vec![DanmuEntry {
                timestamp_ms: 1_010_000,
                text: "A7M4有准新嘛".to_string(),
            }],
        );
        let mut result = GenerateResult {
            generator_type: SubtitleGeneratorType::Volcengine,
            subtitle_id: String::new(),
            subtitle_content: vec![srtparse::Item {
                pos: 1,
                start_time: ms_to_time(5_000),
                end_time: ms_to_time(9_000),
                text: "a 七 m 四没有准新".to_string(),
            }],
        };
        let audit = calibrate_with_danmu(&mut result, &timeline);
        assert_eq!(result.subtitle_content[0].text, "A7M4没有准新");
        assert_eq!(audit.changes.len(), 1);
        assert_eq!(audit.changes[0].evidence, "A7M4有准新嘛");
    }

    #[test]
    fn leaves_unsupported_semantic_guess_unchanged() {
        let timeline = DanmuTimeline::from_test_entries(
            1_000_000,
            vec![DanmuEntry {
                timestamp_ms: 1_010_000,
                text: "99新有现货嘛".to_string(),
            }],
        );
        let mut result = GenerateResult {
            generator_type: SubtitleGeneratorType::Volcengine,
            subtitle_id: String::new(),
            subtitle_content: vec![srtparse::Item {
                pos: 1,
                start_time: ms_to_time(5_000),
                end_time: ms_to_time(9_000),
                text: "99新有线线".to_string(),
            }],
        };
        let audit = calibrate_with_danmu(&mut result, &timeline);
        assert_eq!(result.subtitle_content[0].text, "99新有线线");
        assert!(audit.changes.is_empty());
    }

    #[test]
    fn does_not_replace_an_entity_prefix() {
        let timeline = DanmuTimeline::from_test_entries(
            1_000_000,
            vec![DanmuEntry {
                timestamp_ms: 1_010_000,
                text: "zve1".to_string(),
            }],
        );
        let mut result = GenerateResult {
            generator_type: SubtitleGeneratorType::Volcengine,
            subtitle_id: String::new(),
            subtitle_content: vec![srtparse::Item {
                pos: 1,
                start_time: ms_to_time(5_000),
                end_time: ms_to_time(9_000),
                text: "ZVE 10，ZVE 1".to_string(),
            }],
        };
        let audit = calibrate_with_danmu(&mut result, &timeline);
        assert_eq!(result.subtitle_content[0].text, "ZVE 10，zve1");
        assert_eq!(audit.changes.len(), 1);
    }

    #[test]
    fn ignores_an_isolated_two_digit_message() {
        let timeline = DanmuTimeline::from_test_entries(
            1_000_000,
            vec![DanmuEntry {
                timestamp_ms: 1_010_000,
                text: "99".to_string(),
            }],
        );
        let mut result = GenerateResult {
            generator_type: SubtitleGeneratorType::Volcengine,
            subtitle_id: String::new(),
            subtitle_content: vec![srtparse::Item {
                pos: 1,
                start_time: ms_to_time(5_000),
                end_time: ms_to_time(9_000),
                text: "这个就应该是九九的".to_string(),
            }],
        };
        let audit = calibrate_with_danmu(&mut result, &timeline);
        assert_eq!(result.subtitle_content[0].text, "这个就应该是九九的");
        assert!(audit.changes.is_empty());
    }

    fn ms_to_time(total_ms: u64) -> srtparse::Time {
        srtparse::Time {
            hours: total_ms / 3_600_000,
            minutes: (total_ms % 3_600_000) / 60_000,
            seconds: (total_ms % 60_000) / 1000,
            milliseconds: total_ms % 1000,
        }
    }
}
