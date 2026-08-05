use crate::state::State;
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MasterSectionIndexEntry {
    pub id: i64,
    pub title: String,
    pub purpose: String,
    pub kind: String,
    pub keywords: Vec<String>,
}

/// Database-backed section index.  Prompts can use this bounded list instead of inventing chapters.
pub async fn get_section_index(
    state: &State,
    script_key: &str,
) -> Result<Vec<MasterSectionIndexEntry>, String> {
    let master = state
        .db
        .get_latest_published_master(script_key)
        .await
        .map_err(String::from)?;
    let sections = state
        .db
        .list_master_sections(master.id)
        .await
        .map_err(String::from)?;
    Ok(sections
        .into_iter()
        .map(|section| {
            let purpose = serde_json::from_str::<serde_json::Value>(&section.metadata_json)
                .ok()
                .and_then(|value| {
                    value
                        .get("purpose")
                        .and_then(|item| item.as_str())
                        .map(str::to_string)
                })
                .unwrap_or_else(|| section.title.clone());
            let mut keywords = section
                .title
                .split(|c: char| {
                    c.is_whitespace() || matches!(c, '/' | '、' | '，' | ',' | '：' | ':')
                })
                .filter(|item| !item.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>();
            if let Some(card) = section.product_card_id.as_ref() {
                keywords.push(card.clone());
            }
            MasterSectionIndexEntry {
                id: section.id,
                title: section.title,
                purpose,
                kind: section.section_kind,
                keywords,
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    #[test]
    fn chapter_keyword_split_keeps_chinese_titles() {
        let words = "稀缺性催单与库存话术"
            .split_whitespace()
            .collect::<Vec<_>>();
        assert_eq!(words, vec!["稀缺性催单与库存话术"]);
    }
}
