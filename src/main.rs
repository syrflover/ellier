use std::{
    io,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::Duration,
};

use chrono::{DateTime, Utc};
use chzzk::{
    live::{
        get_live_detail::{self, GetLiveDetail},
        get_live_status::{self, GetLiveStatus},
    },
    model::{LiveDetail, LivePlaybackMedia, LiveStatusType},
    request::Auth,
};
use serde::Deserialize;
use tokio::{
    fs,
    signal::{
        self,
        unix::{Signal, SignalKind},
    },
    time::sleep,
};
use tracing::Level;

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

fn encode_stream(
    stream_url: &str,
    save_directory: impl AsRef<Path>,
    now: DateTime<Utc>,
) -> io::Result<Child> {
    let save_file_path = save_directory
        .as_ref()
        .join(format!("{}.mp4", now.format("%Y-%m-%d_%H-%M-%S")));

    Command::new("ffmpeg")
        .args(["-i", stream_url, "-c", "copy", "-bsf:a", "aac_adtstoasc"])
        .arg(save_file_path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::null())
        .stderr(Stdio::inherit())
        .spawn()
}

async fn get_stream(
    auth: Option<&Auth>,
    channel_id: &str,
) -> Result<Option<(LiveDetail, LivePlaybackMedia)>, Error> {
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

async fn watch_stream(
    auth: Option<&Auth>,
    save_dir: impl AsRef<Path>,
    channel_id: &str,
    now: DateTime<Utc>,
) -> Result<Option<(LiveDetail, Child)>, Error> {
    let Some((live_detail, stream)) = get_stream(auth, channel_id).await? else {
        return Ok(None);
    };

    fs::create_dir_all(&save_dir).await?;

    let encoder = encode_stream(&stream.path, &save_dir, now)?;

    Ok(Some((live_detail, encoder)))
}

async fn save_metadata(
    save_dir: impl AsRef<Path>,
    live: &LiveDetail,
    now: DateTime<Utc>,
) -> Result<(), Error> {
    let save_path = save_dir
        .as_ref()
        .join(format!("{}.json", now.format("%Y-%m-%d_%H-%M-%S")));

    let json = serde_json::to_vec(&live).map_err(Error::SerializeJson)?;

    fs::write(save_path, json).await?;

    Ok(())
}

async fn run() {
    #[derive(Deserialize)]
    struct Channel {
        channel_id: String,
        channel_name: String,
    }

    #[derive(Deserialize)]
    struct Config {
        path: String,
        auth: Option<Auth>,
        channels: Vec<Channel>,
    }

    let Config {
        path,
        auth,
        channels,
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

    let save_dir = PathBuf::from(&path).join(channel_name);

    let mut sigterm = signal::unix::signal(SignalKind::terminate()).unwrap();
    let mut encoder_process = None::<Child>;
    let mut prev_live = None::<LiveDetail>;

    loop {
        let now = Utc::now();

        match encoder_process.as_mut() {
            Some(encoder) => match encoder.try_wait() {
                Ok(Some(exit_code)) => {
                    tracing::info!("received signal {} from ffmpeg", exit_code);
                    encoder_process = None;
                    prev_live = None;
                }
                Err(err) => {
                    encoder_process = None;
                    prev_live = None;
                    tracing::error!("{err}");
                }
                _ => {}
            },
            None => {
                let (curr, encoder) =
                    match watch_stream(auth.as_ref(), &save_dir, &channel_id, now).await {
                        Ok(r) => r.unzip(),
                        Err(err) => {
                            tracing::error!("{err}");
                            sleep(Duration::from_secs(5)).await;
                            continue;
                        }
                    };

                encoder_process = encoder;

                if prev_live != curr {
                    if let Some(curr) = curr {
                        match save_metadata(&save_dir, &curr, now).await {
                            Ok(_) => {
                                prev_live.replace(curr);
                            }
                            Err(err) => {
                                tracing::error!("{err}");
                            }
                        }
                    }
                }
            }
        }

        tokio::select! {
            _ = sleep(Duration::from_secs(5)) => {}
            _ = stop_signal(&mut sigterm) => {
                if let Some(mut encoder) = encoder_process.take() {
                    encoder.wait().unwrap();
                }
                return ;
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

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_line_number(true)
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
