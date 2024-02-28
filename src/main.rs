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
    ffmpeg::{AudioCodec, Ffmpeg, VideoCodec},
};
use serde::Serialize;
use tap::Tap;
use tokio::{fs, signal, time::sleep};
use tracing::Level;

pub struct Encoder {
    // process: Child,
    streamlink: Child,
    ffmpeg: Option<Child>,
    // path: PathBuf,
    // started_at: SystemTime,
}

pub struct EncodeStream<'a> {
    stream_url: &'a str,
    save_directory: &'a Path,
    now: DateTime<FixedOffset>,

    ffmpeg_binary: &'a str,

    post_process: bool,
    video_codec: VideoCodec,
    audio_codec: AudioCodec,
}

impl<'a> EncodeStream<'a> {
    pub fn execute(self) -> io::Result<Encoder> {
        let Self {
            stream_url,
            save_directory,
            now,
            post_process,
            ffmpeg_binary,
            video_codec,
            audio_codec,
        } = self;

        let save_file_path =
            save_directory.join(format!("{}.mkv", now.format("%Y-%m-%d_%H-%M-%S")));

        let mut streamlink = {
            let mut streamlink = Command::new("streamlink");

            streamlink.args([
                stream_url,
                "best",
                "--loglevel",
                "info",
                "--ffmpeg-ffmpeg",
                ffmpeg_binary.trim(),
                "--ffmpeg-copyts",
                "--ffmpeg-fout",
                "matroska",
            ]);

            if post_process {
                streamlink
                    .arg("--stdout")
                    .stdin(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .stdout(Stdio::piped());
            } else {
                streamlink
                    .arg("-o")
                    .arg(save_file_path.as_os_str())
                    .stdin(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .stdout(Stdio::null());
            }

            streamlink
                .tap(|cmd| tracing::info!("Streamlink{:#?}", cmd.get_args()))
                .spawn()?
        };

        let ffmpeg = if post_process {
            Some(
                Command::new("ffmpeg")
                    .args([
                        "-loglevel",
                        "info",
                        "-i",
                        "pipe:",
                        "-c:v",
                        video_codec.as_str(),
                        "-c:a",
                        audio_codec.as_str(),
                    ])
                    .arg(save_file_path.as_os_str())
                    .stdin(streamlink.stdout.take().unwrap())
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .tap(|cmd| tracing::info!("Ffmpeg{:#?}", cmd.get_args()))
                    .spawn()?,
            )
        } else {
            None
        };

        Ok(Encoder {
            streamlink,
            ffmpeg,
            // path: save_file_path,
            // started_at: SystemTime::now(),
        })
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
    pub async fn execute(self) -> ellier::Result<Option<(LiveDetail, Encoder)>> {
        let Self {
            auth,
            save_directory,
            channel_id,
            now,
            ffmpeg:
                Ffmpeg {
                    post_process,
                    ffmpeg_binary,
                    video_codec,
                    audio_codec,
                    output_format: _,
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
            post_process: *post_process,
            ffmpeg_binary,
            video_codec: *video_codec,
            audio_codec: *audio_codec,
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

fn get_ffmpeg_binary() -> String {
    let buf = Command::new("which")
        .arg("ffmpeg")
        .output()
        .expect("can't get ffmpeg binary")
        .stdout;

    String::from_utf8(buf).unwrap()
}

async fn run() {
    let Config {
        path,
        auth,
        channels,
        mut ffmpeg,
        timezone,
    } = serde_json::from_slice::<Config>(&fs::read("./config.json").await.unwrap()).unwrap();

    //

    let log_level = get_log_level();

    let timer = tracing_subscriber::fmt::time::OffsetTime::new(
        timezone.into(),
        time::format_description::well_known::Rfc3339,
    );

    let subscriber = tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_line_number(true)
        .with_timer(timer)
        .finish();

    tracing::subscriber::set_global_default(subscriber).unwrap();

    //

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

    ffmpeg.ffmpeg_binary = get_ffmpeg_binary();

    #[cfg(unix)]
    let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate()).unwrap();
    #[cfg(target_os = "windows")]
    let mut ctrl_c = signal::windows::ctrl_c().unwrap();

    let save_directory = PathBuf::from(&path).join(channel_name);
    let mut encoder = None::<Encoder>;
    let mut prev_live = None::<LiveStatus>;

    loop {
        let now = Utc::now().with_timezone(&timezone.into());

        match encoder.as_mut() {
            Some(Encoder {
                streamlink,
                ffmpeg,
                // path,
                // started_at,
            }) => match streamlink.try_wait() {
                Ok(Some(_exit_code)) => {
                    if let Some(ffmpeg) = ffmpeg.as_mut() {
                        ffmpeg.try_wait().ok();
                    }
                    // print_metadata(path, &started_at).await;
                    tracing::info!("ended stream");
                    encoder = None;
                    prev_live = None;
                    continue; // 예상치 않은 종료가 발생할 수 있으므로 5초 기다리지 않음
                }
                Err(err) => {
                    if let Some(ffmpeg) = ffmpeg.as_mut() {
                        ffmpeg.try_wait().ok();
                    }
                    // print_metadata(path, &started_at).await;
                    tracing::error!("{err}");
                    encoder = None;
                    prev_live = None;
                    continue; // 예상치 않은 종료가 발생할 수 있으므로 5초 기다리지 않음
                }
                Ok(None) => {
                    fn is_modified(prev: Option<&LiveStatus>, curr: &LiveStatus) -> bool {
                        let Some((prev, curr)) = prev.zip(Some(curr)) else {
                            return false;
                        };

                        let modified_category = [
                            // prev.category_type != curr.category_type,
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
                        Ok(curr) if is_modified(prev_live.as_ref(), &curr) => {
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

                    // print_metadata(&path, started_at).await;
                }
            },
            None => {
                let (live_detail, new_encoder) = match (WatchStream {
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

                encoder = new_encoder;

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
            _ = stop_signal(#[cfg(unix)] &mut sigterm, #[cfg(target_os = "windows")] &mut ctrl_c) => {
                if let Some(Encoder {
                    mut streamlink,
                    mut ffmpeg,
                    // path,
                    // started_at
                 }) = encoder.take() {
                    // print_metadata(path, &started_at).await;
                    tracing::info!("received stop signal");
                    streamlink.wait().ok();
                    if let Some(ffmpeg) = ffmpeg.as_mut() {
                        ffmpeg.try_wait().ok();
                    }
                }
                return;
            }
        }
    }
}

// async fn print_metadata(path: impl AsRef<Path>, started_at: &SystemTime) {
//     let Ok(elapsed) = started_at.elapsed() else {
//         return;
//     };

//     let path = path.as_ref();

//     let metadata = match fs::metadata(path).await {
//         Ok(metadata) => metadata,
//         Err(err) => {
//             tracing::error!("{err}");
//             return;
//         }
//     };

//     let ffprobe = match ffprobe(path) {
//         Ok(ffprobe) => ffprobe,
//         Err(err) => {
//             tracing::error!("{err}");
//             return;
//         }
//     };

//     let audio = ffprobe.streams.iter().find_map(|stream| stream.audio());
//     let video = ffprobe.streams.iter().find_map(|stream| stream.video());

//     if let Some((audio, video)) = audio.zip(video) {
//         println!(
//             "size = {}mb; vcodec = {}; fps = {}; res = {}x{}; acodec = {}; time = {}s",
//             metadata.len() / 1024 / 1024,
//             video.codec_name,
//             video.avg_frame_rate,
//             video.width,
//             video.height,
//             audio.codec_name,
//             elapsed.as_secs(),
//         );
//     }
// }

async fn stop_signal(
    #[cfg(unix)] sigterm: &mut signal::unix::Signal,
    #[cfg(target_os = "windows")] ctrl_c: &mut signal::windows::CtrlC,
) {
    #[cfg(unix)]
    tokio::select! {
        _ = sigterm.recv() => {}
        _ = async { signal::ctrl_c().await.expect("failed to listen for ctrl_c event") } => {}
    };
    #[cfg(target_os = "windows")]
    ctrl_c.recv().await;
}

#[tokio::main]
async fn main() {
    println!("Hello, world!");

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
