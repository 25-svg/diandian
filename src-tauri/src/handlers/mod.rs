pub mod account;
pub mod ai;
pub mod anchor_detection;
pub mod config;
pub mod knowledge;
pub mod live_dashboard;
pub mod macros;
pub mod master_script;
pub mod message;
pub mod recorder;
pub mod review_sample;
pub mod task;
pub mod transcript_review;
pub mod utils;
pub mod video;
pub mod video_editing;

use crate::database::account::AccountRow;

#[derive(serde::Serialize)]
pub struct AccountInfo {
    pub accounts: Vec<AccountRow>,
}
