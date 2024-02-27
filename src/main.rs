use std::{
    io,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

use chrono::{DateTime, FixedOffset, Utc};
use chzzk::{
    live::{get_live_detail::GetLiveDetail, get_live_status::GetLiveStatus},
    model::{LiveDetail, LivePlaybackMedia, LiveStatus, LiveStatusType},
    request::Auth,
};
use ellier::{
    config::{Channel, Config},
    ffmpeg::{AudioCodec, Ffmpeg, OutputFormat, VideoCodec},
};
use serde::Serialize;
use tap::Tap;
use time::macros::offset;
use tokio::{
    fs,
    signal::{
        self,
        unix::{Signal, SignalKind},
    },
    time::sleep,
};
use tracing::Level;

pub struct EncodeStream<'a> {
    stream_url: &'a str,
    save_directory: &'a Path,
    now: DateTime<FixedOffset>,

    video_codec: VideoCodec,
    audio_codec: AudioCodec,
    output_format: OutputFormat,
}

impl<'a> EncodeStream<'a> {
    pub fn execute(self) -> io::Result<Child> {
        let Self {
            stream_url,
            save_directory,
            now,
            video_codec,
            audio_codec,
            output_format,
        } = self;

        let save_file_path =
            save_directory.join(format!("{}.mkv", now.format("%Y-%m-%d_%H-%M-%S")));

        Command::new("streamlink")
            .args([
                stream_url,
                "best",
                "--loglevel",
                "info",
                "--ffmpeg-video-transcode",
                video_codec.as_str(),
                "--ffmpeg-audio-transcode",
                audio_codec.as_str(),
                "--ffmpeg-fout",
                output_format.as_str(),
            ])
            .arg("-o")
            .arg(save_file_path.as_os_str())
            .stdin(Stdio::inherit())
            .stderr(Stdio::inherit())
            .stdout(Stdio::null())
            .tap(|ffmpeg| tracing::info!("Ffmpeg{:#?}", ffmpeg.get_args()))
            .spawn()
    }
}

pub struct GetStream<'a> {
    auth: Option<&'a Auth>,
    channel_id: &'a str,
}

impl<'a> GetStream<'a> {
    pub async fn execute(self) -> ellier::Result<Option<(LiveDetail, LivePlaybackMedia)>> {
        let Self { auth, channel_id } = self;

        let live_status = GetLiveStatus { channel_id }.send(auth).await?;

        if let LiveStatusType::Open = live_status.status {
            let live_detail = GetLiveDetail { channel_id }.send(auth).await?;

            if live_detail.inherit.adult && live_detail.inherit.live_playback.is_none() {
                tracing::warn!("YOU'RE NOT AN ADULT");
                return Ok(None);
            }

            let Some(live_playback) = live_detail.inherit.live_playback.as_ref() else {
                return Ok(None);
            };

            let stream = live_playback
                .media
                .iter()
                .find(|media| media.media_id == "HLS")
                .cloned();

            Ok(stream.map(|stream| (live_detail, stream)))
        } else {
            Ok(None)
        }
    }
}

pub struct WatchStream<'a> {
    auth: Option<&'a Auth>,
    save_directory: &'a Path,
    channel_id: &'a str,
    now: DateTime<FixedOffset>,
    ffmpeg: &'a Ffmpeg,
}

impl<'a> WatchStream<'a> {
    pub async fn execute(self) -> ellier::Result<Option<(LiveDetail, Child)>> {
        let Self {
            auth,
            save_directory,
            channel_id,
            now,
            ffmpeg:
                Ffmpeg {
                    video_codec,
                    audio_codec,
                    output_format,
                },
        } = self;

        let Some((live_detail, stream)) = GetStream { auth, channel_id }.execute().await? else {
            return Ok(None);
        };

        fs::create_dir_all(&save_directory).await?;

        let encoder = EncodeStream {
            stream_url: &stream.path,
            save_directory,
            now,
            video_codec: *video_codec,
            audio_codec: *audio_codec,
            output_format: *output_format,
        }
        .execute()?;

        Ok(Some((live_detail, encoder)))
    }
}

async fn save_metadata<T: Serialize>(
    save_dir: impl AsRef<Path>,
    live: &T,
    now: DateTime<FixedOffset>,
) -> ellier::Result<()> {
    let save_path = save_dir
        .as_ref()
        .join(format!("{}.json", now.format("%Y-%m-%d_%H-%M-%S")));

    let json = serde_json::to_vec(&live).map_err(ellier::Error::SerializeJson)?;

    fs::write(save_path, json).await?;

    Ok(())
}

async fn run() {
    let Config {
        path,
        auth,
        channels,
        ffmpeg,
    } = serde_json::from_slice::<Config>(&fs::read("./config.json").await.unwrap()).unwrap();

    let index = std::env::args()
        .find(|arg| arg.starts_with("--index="))
        .and_then(|x| x["--index=".len()..].trim().parse::<usize>().ok());

    let name = std::env::args()
        .find(|arg| arg.starts_with("--name="))
        .map(|x| x["--name=".len()..].trim().to_owned());

    let Channel {
        channel_id,
        channel_name,
    } = if let Some(index) = index {
        channels.into_iter().nth(index).expect("hasn't channel")
    } else if let Some(name) = name {
        channels
            .into_iter()
            .find(|x| x.channel_name == name)
            .expect("hasn't channel")
    } else {
        panic!("please set `--index=<number>` or `--name=<string>`");
    };

    tracing::info!("channel_id   = {:?}", channel_id);
    tracing::info!("channel_name = {:?}", channel_name);

    let mut sigterm = signal::unix::signal(SignalKind::terminate()).unwrap();

    let save_directory = PathBuf::from(&path).join(channel_name);
    let mut encoder_process = None::<Child>;
    let mut prev_live = None::<LiveStatus>;

    loop {
        let now = Utc::now().with_timezone(&FixedOffset::east_opt(9 * 3600).unwrap());

        match encoder_process.as_mut() {
            Some(encoder) => match encoder.try_wait() {
                Ok(Some(exit_code)) => {
                    tracing::info!("ended encode stream: {}", exit_code);
                    encoder_process = None;
                    prev_live = None;
                    continue; // 예상치 않은 종료가 발생할 수 있으므로 5초 기다리지 않음
                }
                Err(err) => {
                    tracing::error!("{err}");
                    encoder_process = None;
                    prev_live = None;
                    continue; // 예상치 않은 종료가 발생할 수 있으므로 5초 기다리지 않음
                }
                Ok(None) => {
                    fn is_modified(prev: Option<&LiveStatus>, curr: Option<&LiveStatus>) -> bool {
                        let Some((prev, curr)) = prev.zip(curr) else {
                            return false;
                        };

                        let modified_category = [
                            prev.category_type != curr.category_type,
                            prev.live_category != curr.live_category,
                            prev.live_category_value != curr.live_category_value,
                        ]
                        .into_iter()
                        .all(|ne| ne);

                        [
                            prev.live_title != curr.live_title,
                            // prev.status != curr.status,
                            // prev.paid_promotion != curr.paid_promotion,
                            prev.adult != curr.adult,
                            modified_category,
                            // prev.chat_active != curr.chat_active,
                            // prev.chat_available_group != curr.chat_available_group,
                            // prev.chat_available_condition != curr.chat_available_condition,
                        ]
                        .into_iter()
                        .any(|ne| ne)
                    }

                    let curr = GetLiveStatus {
                        channel_id: &channel_id,
                    }
                    .send(&auth)
                    .await;

                    match curr {
                        Ok(curr) if is_modified(prev_live.as_ref(), Some(&curr)) => {
                            match save_metadata(&save_directory, &curr, now).await {
                                Ok(_) => {
                                    tracing::info!("modified live status");
                                    prev_live.replace(curr);
                                }
                                Err(err) => {
                                    tracing::error!("{err}");
                                }
                            }
                        }
                        Err(err) => {
                            tracing::error!("save_metadata: {err}");
                        }
                        _ => {}
                    }
                }
            },
            None => {
                let (live_detail, encoder) = match (WatchStream {
                    auth: auth.as_ref(),
                    save_directory: &save_directory,
                    channel_id: &channel_id,
                    now,
                    ffmpeg: &ffmpeg,
                })
                .execute()
                .await
                {
                    Ok(r) => r.unzip(),
                    Err(err) => {
                        tracing::error!("{err}");
                        sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                };

                encoder_process = encoder;

                if let Some(live_detail) = live_detail {
                    match save_metadata(&save_directory, &live_detail, now).await {
                        Ok(_) => {
                            tracing::info!("created live status");
                            prev_live.replace(live_detail.into());
                        }
                        Err(err) => {
                            tracing::error!("save_metadata: {err}");
                        }
                    }
                }
            }
        }

        tokio::select! {
            _ = sleep(Duration::from_secs(5)) => {}
            _ = stop_signal(&mut sigterm) => {
                tracing::info!("received stop signal");
                if let Some(mut encoder) = encoder_process.take() {
                    encoder.wait().unwrap();
                }
                return;
            }
        }
    }
}

async fn stop_signal(sigterm: &mut Signal) {
    tokio::select! {
        _ = sigterm.recv() => {}
        _ = async { signal::ctrl_c().await.expect("failed to listen for ctrl_c event") } => {}
    };
}

#[tokio::main]
async fn main() {
    println!("Hello, world!");

    let log_level = get_log_level();

    let timer = tracing_subscriber::fmt::time::OffsetTime::new(
        offset!(+09:00:00),
        time::format_description::well_known::Rfc3339,
    );

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_line_number(true)
        .with_timer(timer)
        .finish();

    tracing::subscriber::set_global_default(subscriber).unwrap();

    run().await;
}

fn get_log_level() -> Level {
    let log_level = std::env::var("LOG_LEVEL").unwrap_or("info".to_string());

    match log_level.as_str() {
        "error" => Level::ERROR,
        "warn" => Level::WARN,
        "info" => Level::INFO,
        "debug" => Level::DEBUG,
        "trace" => Level::TRACE,
        _ => {
            println!("invalid LOG_LEVEL, set Level::INFO");

            Level::INFO
        }
    }
}
