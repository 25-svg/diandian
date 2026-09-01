use super::{Database, DatabaseError};
use sqlx::Row;

pub const COMPASS_ANALYSIS_MIGRATION_SQL: &str = r#"
CREATE TABLE compass_capture_analyses (
    capture_id TEXT PRIMARY KEY,
    target_date TEXT NOT NULL DEFAULT '',
    target_shop_name TEXT NOT NULL DEFAULT '',
    capture_status TEXT NOT NULL DEFAULT '',
    quality_json TEXT NOT NULL DEFAULT '{}',
    analysis_json TEXT NOT NULL DEFAULT '{}',
    raw_path TEXT NOT NULL DEFAULT '',
    analyzed_at TEXT NOT NULL
);
CREATE TABLE compass_metric_points (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    capture_id TEXT NOT NULL,
    metric_label TEXT NOT NULL,
    time_label TEXT NOT NULL,
    sort_value REAL NOT NULL DEFAULT 0,
    value REAL NOT NULL DEFAULT 0,
    unit TEXT NOT NULL DEFAULT '',
    source_endpoint TEXT NOT NULL DEFAULT '',
    FOREIGN KEY(capture_id) REFERENCES compass_capture_analyses(capture_id) ON DELETE CASCADE
);
CREATE INDEX idx_compass_metric_points_capture_metric
ON compass_metric_points(capture_id, metric_label, sort_value);
CREATE TABLE compass_product_insights (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    capture_id TEXT NOT NULL,
    product_id TEXT NOT NULL DEFAULT '',
    product_name TEXT NOT NULL DEFAULT '',
    explain_count INTEGER NOT NULL DEFAULT 0,
    click_count INTEGER NOT NULL DEFAULT 0,
    payment_amount REAL NOT NULL DEFAULT 0,
    sold_count INTEGER NOT NULL DEFAULT 0,
    FOREIGN KEY(capture_id) REFERENCES compass_capture_analyses(capture_id) ON DELETE CASCADE
);
CREATE INDEX idx_compass_product_insights_capture
ON compass_product_insights(capture_id, payment_amount DESC, explain_count DESC);
"#;

impl Database {
    pub async fn replace_compass_analysis(
        &self,
        analysis: &crate::compass_analysis::CompassCaptureAnalysis,
    ) -> Result<(), DatabaseError> {
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let mut transaction = pool.begin().await?;
        let quality_json = serde_json::to_string(&analysis.quality).unwrap_or_else(|_| "{}".into());
        let analysis_json = serde_json::to_string(analysis).unwrap_or_else(|_| "{}".into());
        sqlx::query(
            r#"INSERT INTO compass_capture_analyses
               (capture_id,target_date,target_shop_name,capture_status,quality_json,analysis_json,raw_path,analyzed_at)
               VALUES ($1,$2,$3,$4,$5,$6,$7,$8)
               ON CONFLICT(capture_id) DO UPDATE SET
               target_date=excluded.target_date,target_shop_name=excluded.target_shop_name,
               capture_status=excluded.capture_status,quality_json=excluded.quality_json,
               analysis_json=excluded.analysis_json,raw_path=excluded.raw_path,
               analyzed_at=excluded.analyzed_at"#,
        )
        .bind(&analysis.capture_id)
        .bind(&analysis.target_date)
        .bind(&analysis.target_shop_name)
        .bind(&analysis.capture_status)
        .bind(quality_json)
        .bind(analysis_json)
        .bind(&analysis.raw_path)
        .bind(&analysis.generated_at)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("DELETE FROM compass_metric_points WHERE capture_id=$1")
            .bind(&analysis.capture_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM compass_product_insights WHERE capture_id=$1")
            .bind(&analysis.capture_id)
            .execute(&mut *transaction)
            .await?;
        for metric in &analysis.metrics {
            for point in &metric.points {
                sqlx::query(
                    "INSERT INTO compass_metric_points (capture_id,metric_label,time_label,sort_value,value,unit,source_endpoint) VALUES ($1,$2,$3,$4,$5,$6,$7)",
                )
                .bind(&analysis.capture_id)
                .bind(&metric.label)
                .bind(&point.time_label)
                .bind(point.sort_value)
                .bind(point.value)
                .bind(&metric.unit)
                .bind(&metric.source_endpoint)
                .execute(&mut *transaction)
                .await?;
            }
        }
        for product in &analysis.products {
            sqlx::query(
                "INSERT INTO compass_product_insights (capture_id,product_id,product_name,explain_count,click_count,payment_amount,sold_count) VALUES ($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(&analysis.capture_id)
            .bind(&product.product_id)
            .bind(&product.product_name)
            .bind(product.explain_count)
            .bind(product.click_count)
            .bind(product.payment_amount)
            .bind(product.sold_count)
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    pub async fn get_compass_analysis_json(
        &self,
        capture_id: &str,
    ) -> Result<Option<String>, DatabaseError> {
        let pool = self
            .db
            .read()
            .await
            .clone()
            .ok_or(DatabaseError::NotFound)?;
        let row =
            sqlx::query("SELECT analysis_json FROM compass_capture_analyses WHERE capture_id=$1")
                .bind(capture_id)
                .fetch_optional(&pool)
                .await?;
        Ok(row.map(|value| value.get::<String, _>("analysis_json")))
    }
}
