use chzzk::request::Auth;
use serde::Deserialize;

use crate::ffmpeg::Ffmpeg;

#[derive(Deserialize)]
pub struct Channel {
    pub channel_id: String,
    pub channel_name: String,
}

const fn zero() -> i8 {
    0
}

#[derive(Clone, Copy, Deserialize)]
pub struct Timezone {
    #[serde(default = "zero")]
    pub hours: i8,
    #[serde(default = "zero")]
    pub minutes: i8,
    #[serde(default = "zero")]
    pub seconds: i8,
}

impl Default for Timezone {
    fn default() -> Self {
        Self {
            hours: zero(),
            minutes: zero(),
            seconds: zero(),
        }
    }
}

impl From<Timezone> for time::UtcOffset {
    fn from(
        Timezone {
            hours,
            minutes,
            seconds,
        }: Timezone,
    ) -> Self {
        time::UtcOffset::from_hms(hours, minutes, seconds).unwrap()
    }
}

impl From<Timezone> for chrono::FixedOffset {
    fn from(
        Timezone {
            hours,
            minutes,
            seconds,
        }: Timezone,
    ) -> Self {
        chrono::FixedOffset::east_opt(
            (hours as i32 * 3600) + (minutes as i32 * 60) + seconds as i32,
        )
        .unwrap()
    }
}

#[derive(Deserialize)]
pub struct Config {
    pub path: String,
    pub auth: Option<Auth>,
    pub channels: Vec<Channel>,
    #[serde(default = "Ffmpeg::default")]
    pub ffmpeg: Ffmpeg,
    #[serde(default = "Timezone::default")]
    pub timezone: Timezone,
}
