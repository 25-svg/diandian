use crate::database::knowledge::KnowledgeDocumentRecord;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CompanyDealBenchmark {
    pub card_id: String,
    pub title: String,
    pub version: String,
    pub product_key: String,
    pub product_name: String,
    pub aliases: Vec<String>,
    pub body: String,
}

pub fn parse_company_benchmark(
    record: &KnowledgeDocumentRecord,
) -> Result<CompanyDealBenchmark, String> {
    if record.card_type != "company_deal_benchmark" {
        return Err("知识卡不是公司成交基准".to_string());
    }
    let metadata = serde_json::from_str::<serde_json::Value>(&record.metadata_json)
        .map_err(|error| format!("公司成交基准元数据无法解析：{error}"))?;
    let product_key = metadata
        .get("product_key")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    let product_name = metadata
        .get("product_name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string();
    if product_key.is_empty() {
        return Err("公司成交基准缺少 product_key".to_string());
    }
    if product_name.is_empty() {
        return Err("公司成交基准缺少 product_name".to_string());
    }
    let aliases = match metadata.get("aliases") {
        None => Vec::new(),
        Some(value) => value
            .as_array()
            .ok_or_else(|| "公司成交基准 aliases 必须是数组".to_string())?
            .iter()
            .filter_map(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .collect::<Vec<_>>(),
    };

    Ok(CompanyDealBenchmark {
        card_id: record.card_id.clone(),
        title: record.title.clone(),
        version: record.version.clone(),
        product_key,
        product_name,
        aliases,
        body: record.body.clone(),
    })
}

pub fn select_company_benchmarks(
    records: &[KnowledgeDocumentRecord],
    corpus: &str,
) -> Vec<CompanyDealBenchmark> {
    let corpus = corpus.to_lowercase();
    let mut selected = records
        .iter()
        .filter_map(|record| parse_company_benchmark(record).ok())
        .filter(|benchmark| {
            std::iter::once(benchmark.product_name.as_str())
                .chain(benchmark.aliases.iter().map(String::as_str))
                .map(str::to_lowercase)
                .any(|term| !term.is_empty() && corpus.contains(&term))
        })
        .collect::<Vec<_>>();
    selected.sort_by(|left, right| {
        (&left.product_key, &left.version, &left.card_id).cmp(&(
            &right.product_key,
            &right.version,
            &right.card_id,
        ))
    });
    selected
}

#[cfg(test)]
mod tests {
    use super::{parse_company_benchmark, select_company_benchmarks};
    use crate::database::knowledge::KnowledgeDocumentRecord;

    fn record(card_id: &str, metadata_json: &str) -> KnowledgeDocumentRecord {
        KnowledgeDocumentRecord {
            card_id: card_id.to_string(),
            title: card_id.to_string(),
            card_type: "company_deal_benchmark".to_string(),
            version: "1.0.0".to_string(),
            relative_path: format!("12-公司成交基准/{card_id}.md"),
            metadata_json: metadata_json.to_string(),
            body: "成交结构正文".to_string(),
        }
    }

    #[test]
    fn selects_only_benchmarks_matching_product_name_or_alias() {
        let canon = record(
            "CDB-CANON-XIAOBAITU-001",
            r#"{"product_key":"canon-xiaobaitu","product_name":"佳能小白兔","aliases":["小白兔","EF 70-200"]}"#,
        );
        let sony = record(
            "CDB-SONY-001",
            r#"{"product_key":"sony-a7","product_name":"索尼 A7","aliases":["A7"]}"#,
        );

        let selected = select_company_benchmarks(&[canon, sony], "主播正在介绍小白兔的成色和配件");
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].product_key, "canon-xiaobaitu");
    }

    #[test]
    fn rejects_invalid_product_metadata() {
        let missing_key = record("CDB-INVALID-KEY", r#"{"product_name":"佳能小白兔"}"#);
        assert_eq!(
            parse_company_benchmark(&missing_key).unwrap_err(),
            "公司成交基准缺少 product_key"
        );

        let invalid_aliases = record(
            "CDB-INVALID-ALIASES",
            r#"{"product_key":"canon-xiaobaitu","product_name":"佳能小白兔","aliases":"小白兔"}"#,
        );
        assert_eq!(
            parse_company_benchmark(&invalid_aliases).unwrap_err(),
            "公司成交基准 aliases 必须是数组"
        );
    }
}
