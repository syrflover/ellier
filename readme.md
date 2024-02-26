# ellier

윈도우 지원 안 함

## 설정

```jsonc
// auth.json - optional
// chzzk.naver.com 쿠키에서 가져옴
{
    "nid_ses": "",
    "nid_aut": "",
    "nid_jkl": ""
}
```

```jsonc
// channels.json - required
[
    {
        "channel_id": "",
        "channel_name": "" // 실제 이름과 상관 없이 임의로 지정
    },
    {
        "channel_id": "",
        "channel_name": "" // 실제 이름과 상관 없이 임의로 지정
    }
]
```

```jsonc
// config.json - required
{
    "path": "" // 저장할 폴더 경로
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
순서는 `0`부터 시작하며, 기본 값은 `0`.
