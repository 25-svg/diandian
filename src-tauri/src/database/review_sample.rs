use super::{Database, DatabaseError};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
pub struct ReviewSampleRow {
    pub id: i64,
    pub sample_no: String,
    pub product: String,
    pub category: String,
    pub deal_status: String,
    pub evidence_strength: String,
    pub transcript_path: String,
    pub data_screenshot_path: String,
    pub review_status: String,
    pub is_b_baseline: i64,
    pub ops_score: Option<i64>,
    pub host_score: Option<i64>,
    pub control_score: Option<i64>,
    pub main_issue: String,
    pub notes: String,
    pub clip_type: String,
    pub source_video_path: String,
    pub review_file_path: String,
    pub transcription_quality: String,
    pub agent_version: String,
    pub calibration_score: Option<i64>,
    pub fact_accuracy_score: Option<i64>,
    pub key_action_score: Option<i64>,
    pub oral_usability_score: Option<i64>,
    pub training_value_score: Option<i64>,
    pub review_content: String,
    pub video_id: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ReviewSampleInput {
    pub id: Option<i64>,
    pub sample_no: String,
    pub product: String,
    pub category: String,
    pub deal_status: String,
    pub evidence_strength: String,
    pub transcript_path: String,
    pub data_screenshot_path: String,
    pub review_status: String,
    pub is_b_baseline: bool,
    pub ops_score: Option<i64>,
    pub host_score: Option<i64>,
    pub control_score: Option<i64>,
    pub main_issue: String,
    pub notes: String,
    pub clip_type: String,
    pub source_video_path: String,
    pub review_file_path: String,
    pub transcription_quality: String,
    pub agent_version: String,
    pub calibration_score: Option<i64>,
    pub fact_accuracy_score: Option<i64>,
    pub key_action_score: Option<i64>,
    pub oral_usability_score: Option<i64>,
    pub training_value_score: Option<i64>,
    pub review_content: String,
    pub video_id: Option<i64>,
}

impl Database {
    pub async fn seed_builtin_review_samples(&self) -> Result<Vec<ReviewSampleRow>, DatabaseError> {
        let existing = self.get_review_samples().await?;
        let existing_numbers: std::collections::HashSet<String> =
            existing.iter().map(|row| row.sample_no.clone()).collect();

        let seeds = vec![
            ReviewSampleInput {
                id: None,
                sample_no: "M1-001".to_string(),
                product: "佳能小白兔（完整型号待确认）".to_string(),
                category: "二手镜头 / 70-200".to_string(),
                deal_status: "已确认成交".to_string(),
                evidence_strength: "中".to_string(),
                transcript_path: r"D:\Desktop\佳能小白兔成交片段\直播话术.txt".to_string(),
                data_screenshot_path:
                    r"D:\Desktop\佳能小白兔成交片段\直播大屏数据.png；全场人群画像.png；直播间售出商品1-3.png"
                        .to_string(),
                review_status: "已复盘".to_string(),
                is_b_baseline: true,
                ops_score: None,
                host_score: None,
                control_score: None,
                main_issue: "片段级订单SKU和时间未匹配；验货结论与价格锚点偏弱；多商品链接交叉"
                    .to_string(),
                notes: "首个话术结构B基线；只迁移结构，不复制商品事实".to_string(),
                clip_type: "成交片段".to_string(),
                source_video_path:
                    r"D:\Desktop\佳能小白兔成交片段\screenshot_2026-07-15_08-53-35.mp4"
                        .to_string(),
                review_file_path: "builtin://review_samples/M1-001.md".to_string(),
                transcription_quality: "已有人工逐字稿；未评估音频转写质量".to_string(),
                agent_version: "V1.1".to_string(),
                calibration_score: None,
                fact_accuracy_score: None,
                key_action_score: None,
                oral_usability_score: None,
                training_value_score: None,
                review_content: include_str!("../../resources/review_samples/M1-001.md")
                    .to_string(),
                video_id: None,
            },
            ReviewSampleInput {
                id: None,
                sample_no: "M1-002".to_string(),
                product: "佳能5D3机身".to_string(),
                category: "二手相机机身 / 单反".to_string(),
                deal_status: "片段内未确认成交".to_string(),
                evidence_strength: "中".to_string(),
                transcript_path:
                    r"C:\Users\10230\Documents\Codex\2026-07-15\new-chat\outputs\M1-002-佳能5D3直播口播逐字稿.txt"
                        .to_string(),
                data_screenshot_path: "无单独数据截图；源视频包含直播画面".to_string(),
                review_status: "已复盘".to_string(),
                is_b_baseline: false,
                ops_score: None,
                host_score: None,
                control_score: None,
                main_issue: "主商品约55秒才明确；互动打断主线；没有形成连续交易链路"
                    .to_string(),
                notes: "讲得散负向校准样本，不适合作为B基线".to_string(),
                clip_type: "讲得散片段".to_string(),
                source_video_path:
                    r"D:\Desktop\71d502f13386b670f04708f97dbd9057.mp4".to_string(),
                review_file_path: "builtin://review_samples/M1-002.md".to_string(),
                transcription_quality: "部分听不清；已结合画面人工校对".to_string(),
                agent_version: "V1.1".to_string(),
                calibration_score: None,
                fact_accuracy_score: None,
                key_action_score: None,
                oral_usability_score: None,
                training_value_score: None,
                review_content: include_str!("../../resources/review_samples/M1-002.md")
                    .to_string(),
                video_id: None,
            },
        ];

        for seed in seeds {
            if !existing_numbers.contains(&seed.sample_no) {
                self.save_review_sample(&seed).await?;
            }
        }
        self.get_review_samples().await
    }

    pub async fn get_review_samples(&self) -> Result<Vec<ReviewSampleRow>, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        Ok(sqlx::query_as::<_, ReviewSampleRow>(
            "SELECT * FROM review_samples ORDER BY sample_no DESC, id DESC",
        )
        .fetch_all(&lock)
        .await?)
    }

    pub async fn save_review_sample(
        &self,
        sample: &ReviewSampleInput,
    ) -> Result<ReviewSampleRow, DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        let baseline = if sample.is_b_baseline { 1 } else { 0 };

        if baseline == 1 && !sample.category.trim().is_empty() {
            sqlx::query(
                "UPDATE review_samples SET is_b_baseline = 0, updated_at = datetime('now') WHERE category = $1",
            )
            .bind(sample.category.trim())
            .execute(&lock)
            .await?;
        }

        let id = if let Some(id) = sample.id {
            sqlx::query(
                r"UPDATE review_samples SET
                    sample_no=$1, product=$2, category=$3, deal_status=$4,
                    evidence_strength=$5, transcript_path=$6, data_screenshot_path=$7,
                    review_status=$8, is_b_baseline=$9, ops_score=$10, host_score=$11,
                    control_score=$12, main_issue=$13, notes=$14, clip_type=$15,
                    source_video_path=$16, review_file_path=$17, transcription_quality=$18,
                    agent_version=$19, calibration_score=$20, fact_accuracy_score=$21,
                    key_action_score=$22, oral_usability_score=$23, training_value_score=$24,
                    review_content=$25, video_id=$26, updated_at=datetime('now') WHERE id=$27",
            )
            .bind(sample.sample_no.trim())
            .bind(sample.product.trim())
            .bind(sample.category.trim())
            .bind(sample.deal_status.trim())
            .bind(sample.evidence_strength.trim())
            .bind(sample.transcript_path.trim())
            .bind(sample.data_screenshot_path.trim())
            .bind(sample.review_status.trim())
            .bind(baseline)
            .bind(sample.ops_score)
            .bind(sample.host_score)
            .bind(sample.control_score)
            .bind(sample.main_issue.trim())
            .bind(sample.notes.trim())
            .bind(sample.clip_type.trim())
            .bind(sample.source_video_path.trim())
            .bind(sample.review_file_path.trim())
            .bind(sample.transcription_quality.trim())
            .bind(sample.agent_version.trim())
            .bind(sample.calibration_score)
            .bind(sample.fact_accuracy_score)
            .bind(sample.key_action_score)
            .bind(sample.oral_usability_score)
            .bind(sample.training_value_score)
            .bind(&sample.review_content)
            .bind(sample.video_id)
            .bind(id)
            .execute(&lock)
            .await?;
            id
        } else {
            let result = sqlx::query(
                r"INSERT INTO review_samples (
                    sample_no, product, category, deal_status, evidence_strength,
                    transcript_path, data_screenshot_path, review_status, is_b_baseline,
                    ops_score, host_score, control_score, main_issue, notes, clip_type,
                    source_video_path, review_file_path, transcription_quality, agent_version,
                    calibration_score, fact_accuracy_score, key_action_score,
                    oral_usability_score, training_value_score, review_content, video_id,
                    created_at, updated_at
                ) VALUES (
                    $1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,
                    $21,$22,$23,$24,$25,$26,
                    datetime('now'),datetime('now')
                )",
            )
            .bind(sample.sample_no.trim())
            .bind(sample.product.trim())
            .bind(sample.category.trim())
            .bind(sample.deal_status.trim())
            .bind(sample.evidence_strength.trim())
            .bind(sample.transcript_path.trim())
            .bind(sample.data_screenshot_path.trim())
            .bind(sample.review_status.trim())
            .bind(baseline)
            .bind(sample.ops_score)
            .bind(sample.host_score)
            .bind(sample.control_score)
            .bind(sample.main_issue.trim())
            .bind(sample.notes.trim())
            .bind(sample.clip_type.trim())
            .bind(sample.source_video_path.trim())
            .bind(sample.review_file_path.trim())
            .bind(sample.transcription_quality.trim())
            .bind(sample.agent_version.trim())
            .bind(sample.calibration_score)
            .bind(sample.fact_accuracy_score)
            .bind(sample.key_action_score)
            .bind(sample.oral_usability_score)
            .bind(sample.training_value_score)
            .bind(&sample.review_content)
            .bind(sample.video_id)
            .execute(&lock)
            .await?;
            result.last_insert_rowid()
        };

        Ok(
            sqlx::query_as::<_, ReviewSampleRow>("SELECT * FROM review_samples WHERE id = $1")
                .bind(id)
                .fetch_one(&lock)
                .await?,
        )
    }

    pub async fn delete_review_sample(&self, id: i64) -> Result<(), DatabaseError> {
        let lock = self.db.read().await.clone().unwrap();
        sqlx::query("DELETE FROM review_samples WHERE id = $1")
            .bind(id)
            .execute(&lock)
            .await?;
        Ok(())
    }
}
