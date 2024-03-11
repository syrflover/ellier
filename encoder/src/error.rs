use std::io;

use chzzk::live::{get_live_detail, get_live_status};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("serialize_json: {0}")]
    SerializeJson(serde_json::Error),
    #[error("deserialize_json: {0}")]
    DeserializeJson(serde_json::Error),

    #[error("get_live_status: {0}")]
    GetLiveStatus(#[from] get_live_status::Error),
    #[error("get_live_detail: {0}")]
    GetLiveDetail(#[from] get_live_detail::Error),
}
