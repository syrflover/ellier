use chzzk::request::Auth;
use serde::Deserialize;

use crate::ffmpeg::Ffmpeg;

#[derive(Deserialize)]
pub struct Channel {
    pub channel_id: String,
    pub channel_name: String,
}

#[derive(Deserialize)]
pub struct Config {
    pub path: String,
    pub auth: Option<Auth>,
    pub channels: Vec<Channel>,
    #[serde(default = "Ffmpeg::default")]
    pub ffmpeg: Ffmpeg,
}
