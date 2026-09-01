use super::{Database, DatabaseError};
use crate::database::live_dashboard::LiveDashboardSessionRow;

pub const LIVE_DASHBOARD_BINDINGS_MIGRATION_SQL: &str = r#"
CREATE TABLE live_dashboard_bindings (
  live_id TEXT NOT NULL PRIMARY KEY,
  session_id INTEGER NOT NULL REFERENCES live_dashboard_sessions(id) ON DELETE CASCADE,
  match_method TEXT NOT NULL CHECK(match_method IN ('auto_account', 'auto_shop', 'manual')),
  bound_at TEXT NOT NULL
);
CREATE INDEX idx_live_dashboard_bindings_session
ON live_dashboard_bindings(session_id);
"#;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct LiveDashboardBindingRow {
    pub live_id: String,
    pub session_id: i64,
    pub match_method: String,
    pub bound_at: String,
}

impl Database {
    pub async fn get_live_dashboard_binding(
        &self,
        live_id: &str,
    ) -> Result<Option<LiveDashboardBindingRow>, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        Ok(
            sqlx::query_as("SELECT * FROM live_dashboard_bindings WHERE live_id = $1")
                .bind(live_id)
                .fetch_optional(&pool)
                .await?,
        )
    }

    pub async fn get_live_dashboard_session(
        &self,
        session_id: i64,
    ) -> Result<Option<LiveDashboardSessionRow>, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        Ok(
            sqlx::query_as("SELECT * FROM live_dashboard_sessions WHERE id = $1")
                .bind(session_id)
                .fetch_optional(&pool)
                .await?,
        )
    }

    pub async fn get_bound_live_dashboard_session(
        &self,
        live_id: &str,
    ) -> Result<Option<LiveDashboardSessionRow>, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        Ok(sqlx::query_as(
            "SELECT sessions.* FROM live_dashboard_bindings bindings \
             JOIN live_dashboard_sessions sessions ON sessions.id = bindings.session_id \
             WHERE bindings.live_id = $1",
        )
        .bind(live_id)
        .fetch_optional(&pool)
        .await?)
    }

    pub async fn delete_live_dashboard_binding(&self, live_id: &str) -> Result<(), DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        sqlx::query("DELETE FROM live_dashboard_bindings WHERE live_id = $1")
            .bind(live_id)
            .execute(&pool)
            .await?;
        Ok(())
    }

    pub async fn get_live_ids_bound_to_session(
        &self,
        session_id: i64,
    ) -> Result<Vec<String>, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        Ok(sqlx::query_scalar::<_, String>(
            "SELECT live_id FROM live_dashboard_bindings WHERE session_id = $1 ORDER BY bound_at DESC",
        )
        .bind(session_id)
        .fetch_all(&pool)
        .await?)
    }

    pub async fn bind_live_dashboard_session(
        &self,
        live_id: &str,
        session_id: i64,
        match_method: &str,
    ) -> Result<LiveDashboardBindingRow, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        let bound_at = chrono::Utc::now().to_rfc3339();
        sqlx::query_as(
            "INSERT INTO live_dashboard_bindings (live_id, session_id, match_method, bound_at) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT(live_id) DO UPDATE SET \
               session_id = excluded.session_id, match_method = excluded.match_method, bound_at = excluded.bound_at \
             RETURNING *",
        )
        .bind(live_id)
        .bind(session_id)
        .bind(match_method)
        .bind(bound_at)
        .fetch_one(&pool)
        .await
        .map_err(DatabaseError::from)
    }

    pub async fn list_bound_live_dashboard_sessions_for_live_ids(
        &self,
        live_ids: &[String],
    ) -> Result<Vec<LiveDashboardBindingSummaryRow>, DatabaseError> {
        if live_ids.is_empty() {
            return Ok(Vec::new());
        }

        let pool = self.db.read().await.clone().unwrap();
        let mut builder = sqlx::QueryBuilder::new(
            "SELECT bindings.live_id, bindings.session_id, bindings.match_method, \
             sessions.account_key, sessions.shop_name, sessions.started_at, \
             sessions.payment_amount_fen, sessions.deal_item_count \
             FROM live_dashboard_bindings bindings \
             JOIN live_dashboard_sessions sessions ON sessions.id = bindings.session_id \
             WHERE bindings.live_id IN (",
        );
        {
            let mut separated = builder.separated(", ");
            for live_id in live_ids {
                separated.push_bind(live_id);
            }
        }
        builder.push(")");
        Ok(builder
            .build_query_as::<LiveDashboardBindingSummaryRow>()
            .fetch_all(&pool)
            .await?)
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct LiveDashboardBindingSummaryRow {
    pub live_id: String,
    pub session_id: i64,
    pub match_method: String,
    pub account_key: String,
    pub shop_name: String,
    pub started_at: String,
    pub payment_amount_fen: i64,
    pub deal_item_count: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::live_dashboard::{
        LIVE_DASHBOARD_KPI_ALIGNMENT_MIGRATION_SQL, LIVE_DASHBOARD_MIGRATION_SQL,
        LIVE_DASHBOARD_TIMELINE_MIGRATION_SQL,
    };
    use sqlx::{sqlite::SqlitePoolOptions, Executor};

    async fn test_database() -> Database {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        pool.execute(LIVE_DASHBOARD_MIGRATION_SQL).await.unwrap();
        pool.execute(LIVE_DASHBOARD_KPI_ALIGNMENT_MIGRATION_SQL)
            .await
            .unwrap();
        pool.execute(LIVE_DASHBOARD_TIMELINE_MIGRATION_SQL)
            .await
            .unwrap();
        pool.execute(LIVE_DASHBOARD_BINDINGS_MIGRATION_SQL)
            .await
            .unwrap();
        let database = Database::new();
        database.set(pool).await;
        database
    }

    async fn insert_session(database: &Database, account_key: &str) -> i64 {
        let pool = database.db.read().await.clone().unwrap();
        sqlx::query(
            "INSERT INTO live_dashboard_sessions \
             (account_key, shop_name, started_at, payment_amount_fen, source_file, imported_at, deal_item_count) \
             VALUES ($1, $2, '2026-07-28T08:15:49', 23655200, 'valid-live-dashboard.xlsx', '2026-07-28T16:00:00Z', 51)",
        )
        .bind(account_key)
        .bind("金典拍拍相机专卖店")
        .execute(&pool)
        .await
        .unwrap()
        .last_insert_rowid()
    }

    #[tokio::test]
    async fn lists_only_bound_rows_for_requested_live_ids() {
        let database = test_database().await;
        let expected_session_id = insert_session(&database, "20296833869").await;
        let ignored_session_id = insert_session(&database, "ignored").await;
        database
            .bind_live_dashboard_session("bound-live", expected_session_id, "manual")
            .await
            .unwrap();
        database
            .bind_live_dashboard_session("other-live", ignored_session_id, "manual")
            .await
            .unwrap();

        let rows = database
            .list_bound_live_dashboard_sessions_for_live_ids(&[
                "bound-live".to_string(),
                "unbound-live".to_string(),
            ])
            .await
            .unwrap();

        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].live_id, "bound-live");
        assert_eq!(rows[0].session_id, expected_session_id);
        assert_eq!(rows[0].account_key, "20296833869");
        assert_eq!(rows[0].shop_name, "金典拍拍相机专卖店");
        assert_eq!(rows[0].payment_amount_fen, 23_655_200);
        assert_eq!(rows[0].deal_item_count, Some(51));
    }
}
