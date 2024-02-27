# ellier

윈도우 지원 안 함

## 설정

```jsonc
// config.json
{
    // 저장할 폴더 경로
    "path": "./.temp",
    // chzzk.naver.com 쿠키에서 가져옴
    // 필수 아님
    "auth": {
        "nid_ses": "",
        "nid_aut": "",
        "nid_jkl": ""
    },
    // ffmpeg 옵션
    // 필수 아님
    "ffmpeg": {
        // - hevc_videotoolbox
        // - copy
        "video_codec": "copy",
        // - aac_at
        // - copy
        "audio_codec": "copy",
        // - matroska
        // - mpegts
        // "output_format": "matroska"
    },
    // 필수 아님
    "timezone": {
        "hours": 9
        // "minutes": 0,
        // "seconds": 0
    },
    "channels": [
        {
            "channel_id": "",
            "channel_name": "" // 실제 이름과 상관 없이 임의로 지정
        },
        {
            "channel_id": "",
            "channel_name": "" // 실제 이름과 상관 없이 임의로 지정
        }
    ]
}
```

## 실행

```bash
cargo run -- --index=0
# 또는
cargo build --release
./target/release/ellier --index=0
```

### 옵션

- `--index=<number>`

`channels.json` 파일에 있는 채널 중 한 개의 채널을 지정함.
순서는 `0`부터 시작함.

- `--name=<string>`

`channels.json` 파일에 있는 채널 중 한 개의 채널을 지정함.
