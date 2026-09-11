pub mod account;
pub mod ai;
pub mod anchor_detection;
pub mod anchor_knowledge;
pub mod config;
pub mod doudian_orders;
pub mod knowledge;
pub mod license;
pub mod live_dashboard;
pub mod live_dashboard_binding;
pub mod macros;
pub mod master_script;
pub mod message;
pub mod recorder;
pub mod review_pipeline;
pub mod review_sample;
pub mod startup;
pub mod task;
pub mod training;
pub mod transcript_review;
pub mod utils;
pub mod video;
pub mod video_editing;

use crate::database::account::AccountRow;

#[derive(serde::Serialize)]
pub struct AccountInfo {
    pub accounts: Vec<AccountRow>,
}
