use crate::database::review_sample::{ReviewSampleInput, ReviewSampleRow};
use crate::state::State;
use crate::state_type;

#[cfg(feature = "gui")]
use tauri::State as TauriState;

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn get_review_samples(state: state_type!()) -> Result<Vec<ReviewSampleRow>, String> {
    Ok(state.db.get_review_samples().await?)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn seed_builtin_review_samples(
    state: state_type!(),
) -> Result<Vec<ReviewSampleRow>, String> {
    Ok(state.db.seed_builtin_review_samples().await?)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn save_review_sample(
    state: state_type!(),
    sample: ReviewSampleInput,
) -> Result<ReviewSampleRow, String> {
    if sample.sample_no.trim().is_empty() {
        return Err("样本编号不能为空".to_string());
    }
    if sample.clip_type.trim().is_empty() {
        return Err("片段类型不能为空".to_string());
    }
    Ok(state.db.save_review_sample(&sample).await?)
}

#[cfg_attr(feature = "gui", tauri::command)]
pub async fn delete_review_sample(state: state_type!(), id: i64) -> Result<(), String> {
    Ok(state.db.delete_review_sample(id).await?)
}
