use std::{
    io,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use chrono::{DateTime, FixedOffset, Utc};
use chzzk::{
    live::{get_live_detail::GetLiveDetail, get_live_status::GetLiveStatus},
    model::{Live, LiveDetail, LivePlaybackMedia, LiveStatus, LiveStatusType},
    request::Auth,
};
use encoder::{
    config::{Channel, Config, Timezone},
    ffmpeg::{AudioCodec, Ffmpeg, VideoCodec},
    time::Time,
};
use serde::Serialize;
use tap::Tap;
use tokio::{fs, signal, time::sleep};

pub struct Encoder {
    // process: Child,
    streamlink: Child,
    ffmpeg: Option<Child>,
    directory: PathBuf,
    started_at: DateTime<FixedOffset>,
    time: Instant,
}

pub struct EncodeStream<'a> {
    stream_url: &'a str,
    save_directory: &'a Path,
    timezone: Timezone,

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
            timezone,
            post_process,
            ffmpeg_binary,
            video_codec,
            audio_codec,
        } = self;

        let started_at = Utc::now().with_timezone(&timezone.into());

        let save_directory =
            save_directory.join(started_at.format("%Y-%m-%d_%H-%M-%S").to_string());

        std::fs::create_dir_all(&save_directory)?;

        let save_file_path = save_directory.join("index.mkv");

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
                    .stdin(Stdio::null())
                    .stderr(Stdio::inherit())
                    .stdout(Stdio::piped());
            } else {
                streamlink
                    .arg("-o")
                    .arg(save_file_path.as_os_str())
                    .stdin(Stdio::null())
                    .stderr(Stdio::inherit())
                    .stdout(Stdio::null());
            }

            streamlink
                .tap(|cmd| println!("Streamlink{:#?}", cmd.get_args()))
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
                    .tap(|cmd| println!("Ffmpeg{:#?}", cmd.get_args()))
                    .spawn()?,
            )
        } else {
            None
        };

        Ok(Encoder {
            streamlink,
            ffmpeg,
            directory: save_directory,
            started_at,
            time: Instant::now(),
        })
    }
}

pub struct GetStream<'a> {
    auth: Option<&'a Auth>,
    channel_id: &'a str,
}

impl<'a> GetStream<'a> {
    pub async fn execute(self) -> encoder::Result<Option<(LiveDetail, LivePlaybackMedia)>> {
        let Self { auth, channel_id } = self;

        let live_status = GetLiveStatus { channel_id }.send(auth).await?;

        if let LiveStatusType::Open = live_status.status {
            let live_detail = GetLiveDetail { channel_id }.send(auth).await?;

            if live_detail.inherit.adult && live_detail.inherit.live_playback.is_none() {
                eprintln!("YOU'RE NOT AN ADULT");
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
    timezone: Timezone,
    channel_id: &'a str,
    ffmpeg: &'a Ffmpeg,
}

impl<'a> WatchStream<'a> {
    pub async fn execute(self) -> encoder::Result<Option<(LiveDetail, Encoder)>> {
        let Self {
            auth,
            save_directory,
            timezone,
            channel_id,
            ffmpeg:
                Ffmpeg {
                    post_process,
                    ffmpeg_binary,
                    video_codec,
                    audio_codec,
                },
        } = self;

        let Some((live_detail, stream)) = GetStream { auth, channel_id }.execute().await? else {
            return Ok(None);
        };

        fs::create_dir_all(&save_directory).await?;

        let encoder = EncodeStream {
            stream_url: &stream.path,
            save_directory,
            timezone,
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
    started_at: &DateTime<FixedOffset>,
    time: &Time,
) -> encoder::Result<()> {
    let save_path = save_dir
        .as_ref()
        .join(started_at.format("%Y-%m-%d_%H-%M-%S").to_string());

    let json = serde_json::to_vec(&live).map_err(encoder::Error::SerializeJson)?;

    fs::write(
        save_path.join(format!("{}.json", time.to_readable("-"))),
        json,
    )
    .await?;

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

async fn get_chzzk_auth(http: &reqwest::Client, master_url: &str) -> Option<Auth> {
    if let Ok(resp) = http.get(format!("{}/chzzk-auth", master_url)).send().await {
        if resp.status().is_success() {
            return resp.json::<Auth>().await.ok();
        }
    }
    None
}

async fn run() {
    #[cfg(debug_assertions)]
    {
        dotenv::dotenv().ok();
    }

    let index = std::env::args()
        .find(|arg| arg.starts_with("--index="))
        .and_then(|x| x["--index=".len()..].trim().parse::<usize>().ok());

    let name = std::env::args()
        .find(|arg| arg.starts_with("--name="))
        .map(|x| x["--name=".len()..].trim().to_owned());

    let Config {
        path,
        mut auth,
        channels,
        mut ffmpeg,
        timezone,
        slave,
        master_url,
    } = if index.is_some() || name.is_some() {
        Config::from_file().unwrap()
    } else {
        Config::from_env().unwrap()
    }
    .tap(|config| {
        println!("save_directory = {:?}", config.path);
        println!("post_process.enable = {:#?}", config.ffmpeg.post_process);
        println!(
            "post_process.video_codec = {:#?}",
            config.ffmpeg.video_codec
        );
        println!(
            "post_process.audio_codec = {:#?}",
            config.ffmpeg.audio_codec
        );
    });

    let http = reqwest::Client::builder()
        .timeout(Duration::from_millis(30))
        .build()
        .unwrap();

    if slave {
        auth = get_chzzk_auth(&http, master_url.as_deref().unwrap()).await;
    }

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
        channels.into_iter().next().expect("please set channel")
    };

    let display_channel_name = (GetLiveDetail {
        channel_id: &channel_id,
    })
    .send(&auth)
    .await
    .unwrap()
    .inherit
    .channel
    .channel_name;

    println!("channel_id = {:?}", channel_id);
    println!(
        "channel_name = {:?} / {:?}",
        channel_name, display_channel_name
    );

    ffmpeg.ffmpeg_binary = get_ffmpeg_binary();

    #[cfg(unix)]
    let mut sigterm = signal::unix::signal(signal::unix::SignalKind::terminate()).unwrap();
    #[cfg(target_os = "windows")]
    let mut ctrl_c = signal::windows::ctrl_c().unwrap();

    let save_directory = PathBuf::from(&path).join(channel_name);
    let mut encoder = None::<Encoder>;
    let mut prev_live = None::<LiveStatus>;

    loop {
        if slave {
            auth = get_chzzk_auth(&http, master_url.as_deref().unwrap()).await;
        }

        match encoder.as_mut() {
            Some(Encoder {
                streamlink,
                ffmpeg,
                directory,
                started_at,
                time,
            }) => match streamlink.try_wait() {
                Ok(Some(_exit_code)) => {
                    let time = Time::from(time.elapsed());
                    if let Some(ffmpeg) = ffmpeg.as_mut() {
                        ffmpeg.try_wait().ok();
                    }
                    // print_metadata(path, &started_at).await;

                    if time.as_secs() >= 10 {
                        println!("{} - closed live stream", time.to_readable(":"));
                    } else {
                        match fs::remove_dir_all(directory).await {
                            Ok(_) => {
                                println!(
                                    "{} - removed this live stream, because duration less than 10 secs",
                                    time.to_readable(":")
                                );
                            }
                            Err(err) => eprintln!("remove_dir_all: {err}"),
                        }
                    }

                    encoder = None;
                    prev_live = None;
                    continue; // 예상치 않은 종료가 발생할 수 있으므로 5초 기다리지 않음
                }
                Err(err) => {
                    if let Some(ffmpeg) = ffmpeg.as_mut() {
                        ffmpeg.try_wait().ok();
                    }
                    // print_metadata(path, &started_at).await;
                    eprintln!("{err}");
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
                    let time = Time::from(time.elapsed());

                    match curr {
                        Ok(curr) if is_modified(prev_live.as_ref(), &curr) => {
                            match save_metadata(&save_directory, &curr, started_at, &time).await {
                                Ok(_) => {
                                    println!(
                                        "{} - {:?} Playing {} | ",
                                        time.to_readable(":"),
                                        curr.live_title,
                                        curr.live_category.as_deref().unwrap_or("unknown")
                                    );
                                    prev_live.replace(curr);
                                }
                                Err(err) => {
                                    eprintln!("{err}");
                                }
                            }
                        }
                        Err(err) => {
                            eprintln!("{err}");
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
                    timezone,
                    channel_id: &channel_id,
                    ffmpeg: &ffmpeg,
                })
                .execute()
                .await
                {
                    Ok(r) => r.unzip(),
                    Err(err) => {
                        eprintln!("{err}");
                        sleep(Duration::from_secs(5)).await;
                        continue;
                    }
                };

                encoder = new_encoder;

                let time = Time(0, 0, 0);

                if let Some((live_detail, encoder)) = live_detail.zip(encoder.as_ref()) {
                    let LiveDetail {
                        inherit:
                            Live {
                                live_title,
                                live_category,
                                ..
                            },
                        ..
                    } = &live_detail;

                    match save_metadata(&save_directory, &live_detail, &encoder.started_at, &time)
                        .await
                    {
                        Ok(_) => {
                            println!(
                                "{} - {:?} Playing {}",
                                time.to_readable(":"),
                                live_title,
                                live_category.as_deref().unwrap_or("unknown")
                            );
                            prev_live.replace(live_detail.into());
                        }
                        Err(err) => {
                            eprintln!("{err}");
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
                    directory: _,
                    started_at: _,
                    time
                 }) = encoder.take() {
                    let time = Time::from(time.elapsed());
                    println!("{} - received stop signal", time.to_readable(":"));
                    // print_metadata(path, &started_at).await;

                    streamlink.kill().expect("failed to kill streamlink");

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

// TODO: stream 끝나면 영상 메타데이터에 chapter 정보 넣기
// - 특정 시간 안에 같은 제목 또는 카테고리는 스킵

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
