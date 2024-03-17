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
use tap::Tap;
use tokio::{fs, signal, time::sleep};

pub struct AddMetadata {
    /// <channel>/<date>
    directory: PathBuf,
    chapters: Vec<Chapter>,
}

pub struct Chapters(pub String);

impl Chapters {
    pub fn new() -> Self {
        let mut s = String::new();

        s.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
        s.push_str(r#"<!DOCTYPE Chapters SYSTEM "matroskachapters.dtd">"#);
        s.push_str("<Chapters><EditionEntry>");

        Self(s)
    }

    pub fn build(mut self) -> String {
        self.0.push_str("</EditionEntry></Chapters>");

        self.0
    }

    pub fn add_chapter(&mut self, title: &str, start: Time) {
        /* <?xml version="1.0" encoding="ISO-8859-1"?>
        <!DOCTYPE Chapters SYSTEM "matroskachapters.dtd">
        <Chapters>
          <EditionEntry>
            <ChapterAtom>
              <ChapterTimeStart>00:00:30.000</ChapterTimeStart>
              <ChapterTimeEnd>00:01:20.000</ChapterTimeEnd>
              <ChapterDisplay>
                <ChapterString>A short chapter</ChapterString>
                <ChapterLanguage>eng</ChapterLanguage>
              </ChapterDisplay>
            </ChapterAtom>
            <ChapterAtom>
                <ChapterTimeStart>00:00:46.000</ChapterTimeStart>
                <ChapterTimeEnd>00:01:10.000</ChapterTimeEnd>
                <ChapterDisplay>
                  <ChapterString>A part of that short chapter</ChapterString>
                  <ChapterLanguage>eng</ChapterLanguage>
                </ChapterDisplay>
              </ChapterAtom>
          </EditionEntry>
        </Chapters> */

        self.0.push_str("<ChapterAtom>");

        self.0.push_str("<ChapterTimeStart>");
        self.0.push_str(&start.to_readable(":"));
        self.0.push_str(".000");
        self.0.push_str("</ChapterTimeStart>");

        // self.0.push_str("<ChapterTimeEnd>");
        // self.0.push_str(&end.to_readable(":"));
        // self.0.push_str(".000");
        // self.0.push_str("</ChapterTimeEnd>");

        self.0.push_str("<ChapterDisplay>");
        self.0.push_str("<ChapterString>");
        self.0.push_str(title);
        self.0.push_str("</ChapterString>");
        self.0.push_str("<ChapterLanguage>");
        self.0.push_str("ko");
        self.0.push_str("</ChapterLanguage>");
        self.0.push_str("</ChapterDisplay>");

        self.0.push_str("</ChapterAtom>");
    }
}

impl AddMetadata {
    /// returns stdout if error
    pub async fn execute(self) -> io::Result<Option<String>> {
        // 기존 방식 마이그레이션
        // 1. read dir
        // 2. 00-00-00.json 형태의 파일만 필터링
        // 3. AddMetadata {}.execute()

        // https://mkvtoolnix.download/doc/mkvmerge.html#mkvmerge.chapters

        let Self {
            directory,
            mut chapters,
        } = self;

        chapters.sort_by_key(|chapter| chapter.0.as_secs());

        let mut builder = Chapters::new();
        let mut chapters = chapters.into_iter();
        let mut prev_chapter = None::<Chapter>;

        while let Some(chapter) = chapters.next() {
            let start = prev_chapter
                .as_ref()
                .map(|chapter| chapter.0)
                .unwrap_or_default();

            builder.add_chapter(
                &format!(
                    "{} Playing {}",
                    chapter.1.live_title,
                    chapter
                        .1
                        .live_category
                        .as_deref()
                        .unwrap_or("unknown")
                        .replace("_", " ")
                ),
                start,
            );

            prev_chapter.replace(chapter);
        }

        let metadata_file = directory.join("metadata.xml");

        fs::write(&metadata_file, builder.build()).await?;

        let mut mkvpropedit = Command::new("mkvpropedit");

        mkvpropedit
            .arg(directory.join("index.mkv"))
            // .args(["--edit", "info", "--set", &format!("title={}", live_title)])
            .args(["--edit", "track:a1", "--set", "language=ko"])
            .arg("--chapters")
            .arg(&metadata_file);

        let res = mkvpropedit.output()?;

        // fs::remove_file(&metadata_file).await.ok();

        if res.status.success() {
            Ok(None)
        } else {
            let err = String::from_utf8(res.stdout).unwrap_or("unknown error".to_owned());
            Ok(Some(err))
        }
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Chapter(pub Time, pub LiveStatus);

impl PartialOrd for Chapter {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.0.as_secs().partial_cmp(&other.0.as_secs())
    }
}

impl Ord for Chapter {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.as_secs().cmp(&other.0.as_secs())
    }
}

pub struct Encoder {
    streamlink: Child,
    ffmpeg: Option<Child>,
    /// <channel>
    directory: PathBuf,
    #[allow(dead_code)]
    started_at: DateTime<FixedOffset>,
    time: Instant,

    chapters: Vec<Chapter>,
}

pub struct EncodeStream<'a> {
    auth: Option<&'a Auth>,
    stream_url: &'a str,
    save_directory: &'a Path,
    timezone: Timezone,

    title: &'a str,
    artist: &'a str,

    ffmpeg_binary: &'a str,

    post_process: bool,
    video_codec: VideoCodec,
    audio_codec: AudioCodec,
}

impl<'a> EncodeStream<'a> {
    pub fn execute(self) -> io::Result<Encoder> {
        let Self {
            auth,
            stream_url,
            save_directory,
            timezone,
            title,
            artist,
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
                "--progress",
                "no",
                "--ffmpeg-ffmpeg",
                ffmpeg_binary.trim(),
                "--ffmpeg-copyts",
                "--ffmpeg-fout",
                "matroska",
            ]);

            if let Some(Auth {
                nid_ses,
                nid_aut,
                nid_jkl,
            }) = auth
            {
                streamlink.args([
                    "--http-cookie",
                    &format!("NID_AUT={}", nid_aut),
                    "--http-cookie",
                    &format!("NID_SES={}", nid_ses),
                    "--http-cookie",
                    &format!("NID_JKL={}", nid_jkl),
                ]);
            }

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
                .tap(|streamlink| println!("streamlink.pid={}", streamlink.id()))
        };

        let ffmpeg = if post_process {
            fn escape_special_chars(s: &str) -> String {
                // (‘=’, ‘;’, ‘#’, ‘\’ and a newline) must be escaped with a backslash ‘\’.

                [
                    ('=', "\\="),
                    (';', "\\;"),
                    ('#', "\\#"),
                    ('\\', "\\\\"),
                    ('\n', " "),
                ]
                .into_iter()
                .fold(s.to_owned(), |s, (a, b)| s.replace(a, b))
            }

            Some(
                Command::new("ffmpeg")
                    .args([
                        "-hide_banner",
                        "-nostats",
                        "-loglevel",
                        "info",
                        "-i",
                        "pipe:",
                        "-c:v",
                        video_codec.as_str(),
                        "-c:a",
                        audio_codec.as_str(),
                        "-map_metadata",
                        "0",
                        "-metadata",
                        &format!("title=\"{}\"", escape_special_chars(&title)),
                        "-metadata",
                        &format!("artist=\"{}\"", escape_special_chars(&artist)),
                    ])
                    .arg(save_file_path.as_os_str())
                    .stdin(streamlink.stdout.take().unwrap())
                    .stdout(Stdio::null())
                    .stderr(Stdio::inherit())
                    .tap(|cmd| println!("Ffmpeg{:#?}", cmd.get_args()))
                    .spawn()?
                    .tap(|ffmpeg| println!("ffmpeg.pid={}", ffmpeg.id())),
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
            chapters: Vec::new(),
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
            auth,
            stream_url: &stream.path,
            save_directory,
            timezone,
            title: &live_detail.inherit.live_title,
            artist: &live_detail.inherit.channel.channel_name,
            post_process: *post_process,
            ffmpeg_binary,
            video_codec: *video_codec,
            audio_codec: *audio_codec,
        }
        .execute()?;

        Ok(Some((live_detail, encoder)))
    }
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

/// return: is modified chapter
fn push_or_modify_chapter(chapters: &mut Vec<Chapter>, curr: Chapter) -> bool {
    if curr.1.status == LiveStatusType::Close {
        return false;
    }

    let prev = match chapters.last().cloned() {
        Some(prev) => prev,
        None => {
            chapters.push(curr);
            return true;
        }
    };

    // 1분내에 이전에 사용했던 타이틀이 있으면 거르기.
    // 단, 카테고리가 바뀌었다면 이전 챕터의 카테고리를 최신 챕터의 카테고리로 수정하면됨
    // 1분이라는 기준은 나중에 문제되는 거 같으면 늘릴 수 있음
    let oldest_chapter_before_60s_with_same_title = chapters
        .iter_mut()
        .filter(|prev| curr.0.as_secs() - prev.0.as_secs() <= 60)
        .filter(|prev| prev.1.live_title == curr.1.live_title)
        .min_by_key(|chapter| chapter.0.as_secs());

    if let Some(prev) = oldest_chapter_before_60s_with_same_title {
        if prev.1.live_category != curr.1.live_category {
            prev.1.live_category = curr.1.live_category;
            return true;
        }
        return false;
    }

    let modified = [
        prev.1.live_title != curr.1.live_title,
        prev.1.live_category != curr.1.live_category,
    ]
    .into_iter()
    .any(|ne| ne);

    if modified {
        chapters.push(curr);
        return true;
    }

    false
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
    // let mut prev_live = None::<LiveStatus>;

    loop {
        if slave {
            auth = get_chzzk_auth(&http, master_url.as_deref().unwrap()).await;
        }

        match encoder.as_mut() {
            Some(Encoder {
                streamlink,
                ffmpeg,
                directory,
                started_at: _,
                time,
                chapters,
            }) => match streamlink.try_wait() {
                Ok(Some(_exit_code)) => {
                    let time = Time::from(time.elapsed());

                    if let Some(ffmpeg) = ffmpeg.as_mut() {
                        ffmpeg.try_wait().ok();
                    }

                    if time.as_secs() >= 15 {
                        let added_metadata = AddMetadata {
                            directory: directory.clone(),
                            chapters: chapters.clone(),
                        }
                        .execute()
                        .await;

                        match added_metadata {
                            Ok(None) => {
                                println!("{} - closed live stream", time.to_readable(":"));
                            }
                            Ok(Some(err)) => {
                                eprintln!("{err}");
                            }
                            Err(err) => {
                                eprintln!("{err}");
                            }
                        }
                    } else {
                        match fs::remove_dir_all(directory).await {
                            Ok(_) => {
                                println!(
                                    "{} - removed this live stream, because duration less than 15 secs",
                                    time.to_readable(":")
                                );
                            }
                            Err(err) => eprintln!("remove_dir_all: {err}"),
                        }
                    }

                    encoder = None;
                    continue; // 예상치 않은 종료가 발생할 수 있으므로 5초 기다리지 않음
                }
                Err(err) => {
                    if let Some(ffmpeg) = ffmpeg.as_mut() {
                        ffmpeg.try_wait().ok();
                    }
                    eprintln!("{err}");

                    encoder = None;
                    continue; // 예상치 않은 종료가 발생할 수 있으므로 5초 기다리지 않음
                }
                Ok(None) => {
                    let curr = GetLiveStatus {
                        channel_id: &channel_id,
                    }
                    .send(&auth)
                    .await;
                    let time = Time::from(time.elapsed());

                    match curr.map(|x| Chapter(time, x)) {
                        Ok(curr) => {
                            let modified = push_or_modify_chapter(chapters, curr.clone());

                            if modified {
                                println!(
                                    "{} - {:?} Playing {}",
                                    time.to_readable(":"),
                                    curr.1.live_title,
                                    curr.1.live_category.as_deref().unwrap_or("unknown")
                                );
                            }
                        }
                        Err(err) => {
                            eprintln!("{err}");
                        }
                    }
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

                if let Some((live_detail, encoder)) = live_detail.zip(encoder.as_mut()) {
                    let LiveDetail {
                        inherit:
                            Live {
                                live_title,
                                live_category,
                                ..
                            },
                        ..
                    } = &live_detail;

                    println!(
                        "{} - {:?} Playing {}",
                        time.to_readable(":"),
                        live_title,
                        live_category.as_deref().unwrap_or("unknown")
                    );
                    encoder.chapters.push(Chapter(time, live_detail.into()));
                }
            }
        }

        tokio::select! {
            _ = sleep(Duration::from_secs(5)) => {
                continue;
            }
            _ = stop_signal(#[cfg(unix)] &mut sigterm, #[cfg(target_os = "windows")] &mut ctrl_c) => {}
        }

        if let Some(Encoder {
            mut streamlink,
            mut ffmpeg,
            directory,
            started_at: _,
            chapters,
            time,
        }) = encoder.take()
        {
            let time = Time::from(time.elapsed());

            println!("{} - received stop signal", time.to_readable(":"));

            streamlink.kill().expect("failed to kill streamlink");

            streamlink.wait().ok();

            if let Some(ffmpeg) = ffmpeg.as_mut() {
                ffmpeg.try_wait().ok();

                let added_metadata = AddMetadata {
                    directory: directory.clone(),
                    chapters: chapters.clone(),
                }
                .execute()
                .await;

                match added_metadata {
                    Ok(None) => {}
                    Ok(Some(err)) => {
                        eprintln!("{err}");
                    }
                    Err(err) => {
                        eprintln!("{err}");
                    }
                }
            }
        }
        return;
    }
}

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
