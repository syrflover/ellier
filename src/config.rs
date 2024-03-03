use std::{env, fmt::Debug, fs, str::FromStr};

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

impl Config {
    pub fn new() -> Self {
        Self::from_env().or(Self::from_file()).unwrap()
    }

    pub fn from_file() -> Option<Self> {
        serde_json::from_slice::<Config>(&fs::read("./config.json").ok()?).ok()
    }

    pub fn from_env() -> Option<Self> {
        let auth = match (env_opt("NID_SES"), env_opt("NID_AUT"), env_opt("NID_JKL")) {
            (Some(nid_ses), Some(nid_aut), Some(nid_jkl)) => Some(Auth {
                nid_ses,
                nid_aut,
                nid_jkl,
            }),
            _ => None,
        };

        let channel = Channel {
            channel_id: env_opt("CHANNEL_ID")?,
            channel_name: env_opt("CHANNEL_NAME")?,
        };

        Some(Self {
            path: env_opt("ELLIER_PATH")?,
            auth,
            channels: vec![channel],
            ffmpeg: Ffmpeg {
                post_process: env_opt("ENABLE_POST_PROCESSING").unwrap_or(false),
                ffmpeg_binary: String::new(),
                video_codec: env_opt("VIDEO_CODEC").unwrap_or_default(),
                audio_codec: env_opt("AUDIO_CODEC").unwrap_or_default(),
            },
            timezone: Timezone {
                hours: env_opt("TZ_HOURS").unwrap_or(0),
                minutes: env_opt("TZ_MINUTES").unwrap_or(0),
                seconds: env_opt("TZ_SECONDS").unwrap_or(0),
            },
        })
    }
}

fn env<T>(key: &str) -> T
where
    T: FromStr,
    <T as FromStr>::Err: Debug,
{
    let var = match env::var(key) {
        Ok(r) => r,
        Err(_) => panic!("not set {key}"),
    };

    var.parse().expect("Please set dotenv to valid value")
}

fn env_opt<T>(key: &str) -> Option<T>
where
    T: FromStr,
    <T as FromStr>::Err: Debug,
{
    env::var(key)
        .ok()
        .map(|var| var.parse().expect("Please set dotenv to valid value"))
}
