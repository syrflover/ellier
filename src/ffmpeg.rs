use std::str::FromStr;

use serde::Deserialize;

const fn post_process() -> bool {
    false
}

#[derive(Deserialize)]
pub struct Ffmpeg {
    #[serde(rename = "enable", default = "post_process")]
    pub post_process: bool,
    #[serde(default)]
    pub ffmpeg_binary: String,
    #[serde(default = "VideoCodec::default")]
    pub video_codec: VideoCodec,
    #[serde(default = "AudioCodec::default")]
    pub audio_codec: AudioCodec,
    // #[serde(default = "OutputFormat::default")]
    // pub output_format: OutputFormat,
}

impl Default for Ffmpeg {
    fn default() -> Self {
        #[cfg(target_os = "macos")]
        {
            Self {
                post_process: true,
                ffmpeg_binary: String::new(),
                video_codec: VideoCodec::HevcVideotoolbox,
                audio_codec: AudioCodec::Copy,
                // output_format: OutputFormat::Matroska,
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            Self {
                post_process: true,
                ffmpeg_binary: String::new(),
                video_codec: VideoCodec::Copy,
                audio_codec: AudioCodec::Copy,
                // output_format: OutputFormat::Matroska,
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub enum VideoCodec {
    #[cfg(target_os = "macos")]
    #[serde(rename = "hevc_videotoolbox")]
    HevcVideotoolbox,
    #[serde(rename = "copy")]
    Copy,
}

impl Default for VideoCodec {
    fn default() -> Self {
        #[cfg(target_os = "macos")]
        {
            VideoCodec::HevcVideotoolbox
        }
        #[cfg(not(target_os = "macos"))]
        {
            VideoCodec::Copy
        }
    }
}

impl VideoCodec {
    pub fn as_str(&self) -> &'static str {
        match self {
            #[cfg(target_os = "macos")]
            VideoCodec::HevcVideotoolbox => "hevc_videotoolbox",
            VideoCodec::Copy => "copy",
        }
    }
}

impl FromStr for VideoCodec {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let r = match s {
            #[cfg(target_os = "macos")]
            "hevc_videotoolbox" => VideoCodec::HevcVideotoolbox,
            "copy" => VideoCodec::Copy,
            _ => return Err(()),
        };

        Ok(r)
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub enum AudioCodec {
    #[cfg(target_os = "macos")]
    #[serde(rename = "aac_at")]
    AacAudiotoolbox,
    #[serde(rename = "copy")]
    Copy,
}

impl Default for AudioCodec {
    fn default() -> Self {
        AudioCodec::Copy
    }
}

impl AudioCodec {
    pub fn as_str(&self) -> &'static str {
        match self {
            #[cfg(target_os = "macos")]
            AudioCodec::AacAudiotoolbox => "aac_at",
            AudioCodec::Copy => "copy",
        }
    }
}

impl FromStr for AudioCodec {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let r = match s {
            #[cfg(target_os = "macos")]
            "aac_at" => AudioCodec::AacAudiotoolbox,
            "copy" => AudioCodec::Copy,
            _ => return Err(()),
        };

        Ok(r)
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
pub enum OutputFormat {
    #[serde(rename = "matroska")]
    Matroska,
    #[serde(rename = "mpegts")]
    Mpegts,
}

impl Default for OutputFormat {
    fn default() -> Self {
        OutputFormat::Matroska
    }
}

impl OutputFormat {
    pub fn as_str(&self) -> &'static str {
        match self {
            OutputFormat::Matroska => "matroska",
            OutputFormat::Mpegts => "mpegts",
        }
    }

    pub fn as_ext(&self) -> &'static str {
        match self {
            OutputFormat::Matroska => "mkv",
            OutputFormat::Mpegts => "ts",
        }
    }
}
