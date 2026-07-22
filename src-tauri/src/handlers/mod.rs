pub mod account;
pub mod ai;
pub mod config;
pub mod macros;
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
