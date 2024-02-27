use serde::Deserialize;

const fn crf() -> u8 {
    23
}

#[derive(Deserialize)]
pub struct Ffmpeg {
    #[serde(default = "VideoCodec::default")]
    pub video_codec: VideoCodec,
    // #[serde(default = "AudioCodec::default")]
    // pub audio_codec: AudioCodec,
    #[serde(default = "OutputFormat::default")]
    pub output_format: OutputFormat,
    #[serde(default = "crf")]
    pub crf: u8,
}

impl Default for Ffmpeg {
    fn default() -> Self {
        #[cfg(target_os = "macos")]
        {
            Self {
                video_codec: VideoCodec::HevcVideotoolbox,
                // audio_codec: AudioCodec::Copy,
                output_format: OutputFormat::Matroska,
                crf: crf(),
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            Self {
                video_codec: VideoCodec::Copy,
                // audio_codec: AudioCodec::Copy,
                output_format: OutputFormat::Matroska,
                crf: crf(),
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
        #[cfg(target_os = "macos")]
        {
            AudioCodec::AacAudiotoolbox
        }
        #[cfg(not(target_os = "macos"))]
        {
            AudioCodec::Copy
        }
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
