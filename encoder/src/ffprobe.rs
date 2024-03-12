/*
{
    "streams": [
        {
            "index": 0,
            "codec_name": "hevc",
            "codec_long_name": "H.265 / HEVC (High Efficiency Video Coding)",
            "profile": "Main",
            "codec_type": "video",
            "codec_tag_string": "[0][0][0][0]",
            "codec_tag": "0x0000",
            "width": 1920,
            "height": 1080,
            "coded_width": 1920,
            "coded_height": 1088,
            "closed_captions": 0,
            "film_grain": 0,
            "has_b_frames": 0,
            "sample_aspect_ratio": "1:1",
            "display_aspect_ratio": "16:9",
            "pix_fmt": "yuv420p",
            "level": 123,
            "color_range": "tv",
            "color_space": "bt709",
            "color_transfer": "bt709",
            "color_primaries": "bt709",
            "chroma_location": "left",
            "field_order": "progressive",
            "refs": 1,
            "r_frame_rate": "60/1",
            "avg_frame_rate": "60/1",
            "time_base": "1/1000",
            "start_pts": 150,
            "start_time": "0.150000",
            "extradata_size": 111,
            "disposition": {
                "default": 1,
                "dub": 0,
                "original": 0,
                "comment": 0,
                "lyrics": 0,
                "karaoke": 0,
                "forced": 0,
                "hearing_impaired": 0,
                "visual_impaired": 0,
                "clean_effects": 0,
                "attached_pic": 0,
                "timed_thumbnails": 0,
                "non_diegetic": 0,
                "captions": 0,
                "descriptions": 0,
                "metadata": 0,
                "dependent": 0,
                "still_image": 0
            },
            "tags": {
                "VARIANT_BITRATE": "8192000",
                "HANDLER_NAME": "Video Handler",
                "VENDOR_ID": "[0][0][0][0]",
                "COMPATIBLE_BRANDS": "isommp42dashavc1iso6",
                "MAJOR_BRAND": "msdh",
                "MINOR_VERSION": "0",
                "ENCODER": "Lavc60.31.102 hevc_videotoolbox"
            }
        },
        {
            "index": 1,
            "codec_name": "aac",
            "codec_long_name": "AAC (Advanced Audio Coding)",
            "profile": "LC",
            "codec_type": "audio",
            "codec_tag_string": "[0][0][0][0]",
            "codec_tag": "0x0000",
            "sample_fmt": "fltp",
            "sample_rate": "48000",
            "channels": 2,
            "channel_layout": "stereo",
            "bits_per_sample": 0,
            "initial_padding": 2112,
            "r_frame_rate": "0/0",
            "avg_frame_rate": "0/0",
            "time_base": "1/1000",
            "start_pts": -44,
            "start_time": "-0.044000",
            "extradata_size": 2,
            "disposition": {
                "default": 1,
                "dub": 0,
                "original": 0,
                "comment": 0,
                "lyrics": 0,
                "karaoke": 0,
                "forced": 0,
                "hearing_impaired": 0,
                "visual_impaired": 0,
                "clean_effects": 0,
                "attached_pic": 0,
                "timed_thumbnails": 0,
                "non_diegetic": 0,
                "captions": 0,
                "descriptions": 0,
                "metadata": 0,
                "dependent": 0,
                "still_image": 0
            },
            "tags": {
                "VARIANT_BITRATE": "320000",
                "HANDLER_NAME": "Sound Handler",
                "VENDOR_ID": "[0][0][0][0]",
                "MAJOR_BRAND": "msdh",
                "MINOR_VERSION": "0",
                "COMPATIBLE_BRANDS": "isommp42dashM4A iso6",
                "COMMENT": "audio.stream",
                "ENCODER": "Lavc60.31.102 aac_at"
            }
        }
    ],
    "format": {
        "filename": "2024-02-28_02-34-48.mkv",
        "nb_streams": 2,
        "nb_programs": 0,
        "format_name": "matroska,webm",
        "format_long_name": "Matroska / WebM",
        "start_time": "-0.044000",
        "duration": "609.964000",
        "size": "289736025",
        "bit_rate": "3800040",
        "probe_score": 100,
        "tags": {
            "ENCODER": "Lavf60.16.100"
        }
    }
}
*/

use std::{path::Path, process::Command};

use serde::Deserialize;

#[derive(Debug, Deserialize)]
#[serde(tag = "codec_type")]
pub enum FfprobeStream {
    #[serde(rename = "audio")]
    Audio(FfprobeAudioStream),
    #[serde(rename = "video")]
    Video(FfprobeVideoStream),
}

impl FfprobeStream {
    pub fn audio(&self) -> Option<&FfprobeAudioStream> {
        match self {
            FfprobeStream::Audio(x) => Some(x),
            _ => None,
        }
    }

    pub fn video(&self) -> Option<&FfprobeVideoStream> {
        match self {
            FfprobeStream::Video(x) => Some(x),
            _ => None,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct FfprobeAudioStream {
    pub codec_name: String,
    pub profile: String,
    pub sample_rate: String,
    pub channel_layout: String,
}

#[derive(Debug, Deserialize)]
pub struct FfprobeVideoStream {
    pub codec_name: String,
    pub profile: String,
    pub width: u16,
    pub height: u16,
    pub pix_fmt: String,
    pub avg_frame_rate: String,
}

#[derive(Debug, Deserialize)]
pub struct Ffprobe {
    pub streams: Vec<FfprobeStream>,
}

pub fn ffprobe(path: impl AsRef<Path>) -> crate::Result<Ffprobe> {
    let res = Command::new("ffprobe")
        .arg(path.as_ref())
        .args(["-v", "quiet", "-output_format", "json", "-show_streams"])
        .output()?;

    let json = serde_json::from_slice(&res.stdout).map_err(crate::Error::DeserializeJson)?;

    Ok(json)
}

// pub fn get_duration(p: &Path) -> io::Result<Option<Duration>> {
//     let o = Command::new("ffprobe")
//         // .arg("-i")
//         .arg(p)
//         .args([
//             "-show_entries",
//             "format=duration",
//             "-of",
//             "csv=\"p=0\"",
//             "-v",
//             "quiet",
//         ])
//         .output()?;

//     let duration = String::from_utf8(o.stdout)
//         .ok()
//         .and_then(|x| x.parse::<f64>().ok())
//         .map(Duration::from_secs_f64);

//     Ok(duration)
// }
