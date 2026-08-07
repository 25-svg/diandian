use super::{Database, DatabaseError};
use crate::live_data_import::{
    LiveChannelImport, LiveDashboardImport, LiveProductImport, LiveShortVideoImport,
};

pub const LIVE_DASHBOARD_MIGRATION_SQL: &str = r#"
CREATE TABLE live_dashboard_sessions (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  account_key TEXT NOT NULL,
  shop_name TEXT NOT NULL DEFAULT '',
  started_at TEXT NOT NULL,
  payment_amount_fen INTEGER NOT NULL DEFAULT 0,
  per_thousand_payment_amount_fen INTEGER,
  viewer_count INTEGER,
  average_online INTEGER,
  average_watch_seconds INTEGER,
  viewer_conversion_rate REAL,
  source_file TEXT NOT NULL,
  imported_at TEXT NOT NULL,
  UNIQUE(account_key, started_at)
);
CREATE TABLE live_dashboard_channels (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id INTEGER NOT NULL REFERENCES live_dashboard_sessions(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  viewer_count INTEGER,
  payment_amount_fen INTEGER,
  order_count INTEGER
);
CREATE TABLE live_dashboard_short_videos (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id INTEGER NOT NULL REFERENCES live_dashboard_sessions(id) ON DELETE CASCADE,
  title TEXT NOT NULL,
  published_at TEXT NOT NULL DEFAULT '',
  exposure_count INTEGER,
  referral_count INTEGER,
  click_rate REAL
);
CREATE TABLE live_dashboard_products (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id INTEGER NOT NULL REFERENCES live_dashboard_sessions(id) ON DELETE CASCADE,
  product_id TEXT NOT NULL,
  name TEXT NOT NULL DEFAULT '',
  payment_amount_fen INTEGER,
  sold_count INTEGER,
  buyer_count INTEGER,
  exposure_count INTEGER,
  click_count INTEGER,
  UNIQUE(session_id, product_id)
);
CREATE TABLE live_dashboard_imports (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id INTEGER REFERENCES live_dashboard_sessions(id) ON DELETE SET NULL,
  source_file TEXT NOT NULL,
  status TEXT NOT NULL,
  message TEXT NOT NULL DEFAULT '',
  imported_at TEXT NOT NULL
);
CREATE INDEX idx_live_dashboard_sessions_started_at ON live_dashboard_sessions(started_at DESC);
CREATE INDEX idx_live_dashboard_imports_source_file ON live_dashboard_imports(source_file);
"#;

pub const LIVE_DASHBOARD_KPI_ALIGNMENT_MIGRATION_SQL: &str = r#"
ALTER TABLE live_dashboard_sessions ADD COLUMN deal_buyer_count INTEGER;
ALTER TABLE live_dashboard_sessions ADD COLUMN deal_item_count INTEGER;
ALTER TABLE live_dashboard_sessions ADD COLUMN product_click_conversion_rate REAL;
ALTER TABLE live_dashboard_sessions ADD COLUMN exposure_viewer_rate REAL;
ALTER TABLE live_dashboard_sessions ADD COLUMN qianchuan_spend_fen INTEGER;
"#;

pub const LIVE_DASHBOARD_TIMELINE_MIGRATION_SQL: &str = r#"
ALTER TABLE live_dashboard_sessions ADD COLUMN ended_at TEXT NOT NULL DEFAULT '';
"#;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct LiveDashboardSessionRow {
    pub id: i64,
    pub account_key: String,
    pub shop_name: String,
    pub started_at: String,
    pub ended_at: String,
    pub payment_amount_fen: i64,
    pub per_thousand_payment_amount_fen: Option<i64>,
    pub viewer_count: Option<i64>,
    pub average_online: Option<i64>,
    pub average_watch_seconds: Option<i64>,
    pub viewer_conversion_rate: Option<f64>,
    pub deal_buyer_count: Option<i64>,
    pub deal_item_count: Option<i64>,
    pub product_click_conversion_rate: Option<f64>,
    pub exposure_viewer_rate: Option<f64>,
    pub qianchuan_spend_fen: Option<i64>,
    pub source_file: String,
    pub imported_at: String,
}

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct LiveDashboardProductRow {
    pub id: i64,
    pub session_id: i64,
    pub product_id: String,
    pub name: String,
    pub payment_amount_fen: Option<i64>,
    pub sold_count: Option<i64>,
    pub buyer_count: Option<i64>,
    pub exposure_count: Option<i64>,
    pub click_count: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct LiveDashboardChannelRow {
    pub id: i64,
    pub session_id: i64,
    pub name: String,
    pub viewer_count: Option<i64>,
    pub payment_amount_fen: Option<i64>,
    pub order_count: Option<i64>,
}

#[derive(Debug, Clone, serde::Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct LiveDashboardShortVideoRow {
    pub id: i64,
    pub session_id: i64,
    pub title: String,
    pub published_at: String,
    pub exposure_count: Option<i64>,
    pub referral_count: Option<i64>,
    pub click_rate: Option<f64>,
}

impl Database {
    pub async fn upsert_live_dashboard(
        &self,
        imported: LiveDashboardImport,
        source_file: &str,
    ) -> Result<LiveDashboardSessionRow, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        let mut transaction = pool.begin().await?;
        let now = chrono::Utc::now().to_rfc3339();
        let session = sqlx::query_as::<_, LiveDashboardSessionRow>(
            "INSERT INTO live_dashboard_sessions (account_key, shop_name, started_at, ended_at, payment_amount_fen, per_thousand_payment_amount_fen, viewer_count, average_online, average_watch_seconds, viewer_conversion_rate, deal_buyer_count, deal_item_count, product_click_conversion_rate, exposure_viewer_rate, qianchuan_spend_fen, source_file, imported_at) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17) \
             ON CONFLICT(account_key, started_at) DO UPDATE SET \
               shop_name=excluded.shop_name, ended_at=excluded.ended_at, payment_amount_fen=excluded.payment_amount_fen, \
               per_thousand_payment_amount_fen=excluded.per_thousand_payment_amount_fen, viewer_count=excluded.viewer_count, \
               average_online=excluded.average_online, average_watch_seconds=excluded.average_watch_seconds, viewer_conversion_rate=excluded.viewer_conversion_rate, \
               deal_buyer_count=excluded.deal_buyer_count, deal_item_count=excluded.deal_item_count, \
               product_click_conversion_rate=excluded.product_click_conversion_rate, exposure_viewer_rate=excluded.exposure_viewer_rate, \
               qianchuan_spend_fen=excluded.qianchuan_spend_fen, source_file=excluded.source_file, imported_at=excluded.imported_at \
             RETURNING *",
        )
        .bind(&imported.session.account_key)
        .bind(&imported.session.shop_name)
        .bind(&imported.session.started_at)
        .bind(&imported.session.ended_at)
        .bind(imported.session.payment_amount_fen)
        .bind(imported.session.per_thousand_payment_amount_fen)
        .bind(imported.session.viewer_count)
        .bind(imported.session.average_online)
        .bind(imported.session.average_watch_seconds)
        .bind(imported.session.viewer_conversion_rate)
        .bind(imported.session.deal_buyer_count)
        .bind(imported.session.deal_item_count)
        .bind(imported.session.product_click_conversion_rate)
        .bind(imported.session.exposure_viewer_rate)
        .bind(imported.session.qianchuan_spend_fen)
        .bind(source_file)
        .bind(&now)
        .fetch_one(&mut *transaction)
        .await?;

        for table in [
            "live_dashboard_channels",
            "live_dashboard_short_videos",
            "live_dashboard_products",
        ] {
            sqlx::query(&format!("DELETE FROM {table} WHERE session_id = $1"))
                .bind(session.id)
                .execute(&mut *transaction)
                .await?;
        }
        for channel in &imported.channels {
            insert_channel(&mut transaction, session.id, channel).await?;
        }
        for video in &imported.short_videos {
            insert_short_video(&mut transaction, session.id, video).await?;
        }
        for product in &imported.products {
            insert_product(&mut transaction, session.id, product).await?;
        }
        sqlx::query("INSERT INTO live_dashboard_imports (session_id, source_file, status, message, imported_at) VALUES ($1,$2,'success','',$3)")
            .bind(session.id).bind(source_file).bind(now).execute(&mut *transaction).await?;
        transaction.commit().await?;
        Ok(session)
    }

    pub async fn list_live_dashboard_sessions(
        &self,
    ) -> Result<Vec<LiveDashboardSessionRow>, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        Ok(
            sqlx::query_as("SELECT * FROM live_dashboard_sessions ORDER BY started_at DESC")
                .fetch_all(&pool)
                .await?,
        )
    }

    pub async fn list_live_dashboard_products(
        &self,
        session_id: i64,
    ) -> Result<Vec<LiveDashboardProductRow>, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        Ok(sqlx::query_as("SELECT * FROM live_dashboard_products WHERE session_id=$1 ORDER BY payment_amount_fen DESC, id ASC")
            .bind(session_id).fetch_all(&pool).await?)
    }

    pub async fn list_live_dashboard_channels(
        &self,
        session_id: i64,
    ) -> Result<Vec<LiveDashboardChannelRow>, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        Ok(sqlx::query_as("SELECT * FROM live_dashboard_channels WHERE session_id=$1 ORDER BY payment_amount_fen DESC, id ASC")
            .bind(session_id).fetch_all(&pool).await?)
    }

    pub async fn list_live_dashboard_short_videos(
        &self,
        session_id: i64,
    ) -> Result<Vec<LiveDashboardShortVideoRow>, DatabaseError> {
        let pool = self.db.read().await.clone().unwrap();
        Ok(sqlx::query_as("SELECT * FROM live_dashboard_short_videos WHERE session_id=$1 ORDER BY referral_count DESC, id ASC")
            .bind(session_id).fetch_all(&pool).await?)
    }
}

async fn insert_channel(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    session_id: i64,
    channel: &LiveChannelImport,
) -> Result<(), DatabaseError> {
    sqlx::query("INSERT INTO live_dashboard_channels (session_id,name,viewer_count,payment_amount_fen,order_count) VALUES ($1,$2,$3,$4,$5)")
        .bind(session_id).bind(&channel.name).bind(channel.viewer_count).bind(channel.payment_amount_fen).bind(channel.order_count)
        .execute(&mut **transaction).await?;
    Ok(())
}

async fn insert_short_video(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    session_id: i64,
    video: &LiveShortVideoImport,
) -> Result<(), DatabaseError> {
    sqlx::query("INSERT INTO live_dashboard_short_videos (session_id,title,published_at,exposure_count,referral_count,click_rate) VALUES ($1,$2,$3,$4,$5,$6)")
        .bind(session_id).bind(&video.title).bind(&video.published_at).bind(video.exposure_count).bind(video.referral_count).bind(video.click_rate)
        .execute(&mut **transaction).await?;
    Ok(())
}

async fn insert_product(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    session_id: i64,
    product: &LiveProductImport,
) -> Result<(), DatabaseError> {
    sqlx::query("INSERT INTO live_dashboard_products (session_id,product_id,name,payment_amount_fen,sold_count,buyer_count,exposure_count,click_count) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(session_id).bind(&product.product_id).bind(&product.name).bind(product.payment_amount_fen).bind(product.sold_count).bind(product.buyer_count).bind(product.exposure_count).bind(product.click_count)
        .execute(&mut **transaction).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::Database;
    use crate::live_data_import::{
        LiveChannelImport, LiveDashboardImport, LiveProductImport, LiveSessionImport,
        LiveShortVideoImport,
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
        let database = Database::new();
        database.set(pool).await;
        database
    }

    fn imported_session(product_count: usize) -> LiveDashboardImport {
        LiveDashboardImport {
            session: LiveSessionImport {
                account_key: "20296833869".into(),
                shop_name: "金典拍拍相机专卖店".into(),
                started_at: "2026-07-28T08:15:49".into(),
                ended_at: "2026-07-28T15:44:39".into(),
                payment_amount_fen: 23_655_200,
                per_thousand_payment_amount_fen: Some(2_643_925),
                viewer_count: Some(6782),
                average_online: Some(22),
                average_watch_seconds: Some(65),
                viewer_conversion_rate: Some(0.0068),
                deal_buyer_count: Some(46),
                deal_item_count: Some(product_count as i64),
                product_click_conversion_rate: Some(0.0308),
                exposure_viewer_rate: Some(0.1012),
                qianchuan_spend_fen: Some(220_151),
            },
            channels: vec![LiveChannelImport {
                name: "整体".into(),
                viewer_count: Some(6763),
                payment_amount_fen: Some(23_655_200),
                order_count: Some(51),
                qianchuan_spend_fen: Some(220_151),
            }],
            short_videos: vec![LiveShortVideoImport {
                title: "短视频".into(),
                published_at: "2026-07-24 08:24".into(),
                exposure_count: Some(2248),
                referral_count: Some(140),
                click_rate: Some(0.0623),
            }],
            products: (0..product_count)
                .map(|index| LiveProductImport {
                    product_id: format!("product-{index}"),
                    name: format!("商品 {index}"),
                    payment_amount_fen: Some(100),
                    sold_count: Some(1),
                    buyer_count: Some(1),
                    exposure_count: Some(10),
                    click_count: Some(2),
                })
                .collect(),
        }
    }

    #[tokio::test]
    async fn repeated_import_replaces_details_without_creating_a_second_session() {
        let database = test_database().await;
        database
            .upsert_live_dashboard(imported_session(2), "first.xlsx")
            .await
            .unwrap();
        let updated = database
            .upsert_live_dashboard(imported_session(3), "second.xlsx")
            .await
            .unwrap();

        assert_eq!(
            database.list_live_dashboard_sessions().await.unwrap().len(),
            1
        );
        assert_eq!(
            database
                .list_live_dashboard_products(updated.id)
                .await
                .unwrap()
                .len(),
            3
        );
        assert_eq!(updated.source_file, "second.xlsx");
        assert_eq!(updated.deal_buyer_count, Some(46));
        assert_eq!(updated.deal_item_count, Some(3));
        assert_eq!(updated.product_click_conversion_rate, Some(0.0308));
        assert_eq!(updated.exposure_viewer_rate, Some(0.1012));
        assert_eq!(updated.qianchuan_spend_fen, Some(220_151));
    }
}
