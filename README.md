# dropcast

A small, fast Rust CLI for streaming a local movie to a Chromecast or a TV with
Google Cast built in.

`dropcast` discovers receivers on the local network, presents an arrow-key
device picker, and serves the movie with byte-range support for seeking. It also
discovers subtitle tracks and enables the first one automatically.

## Features

- Single native binary with no Node.js runtime
- Google Cast discovery over mDNS
- Keyboard-navigable receiver picker
- Direct LAN streaming with HTTP byte ranges
- Automatic matching sidecars such as `movie.en.srt`
- Embedded text subtitle discovery when `ffprobe` and `ffmpeg` are installed
- Repeatable explicit `--subtitle` files
- First subtitle track enabled by default

## Build

Install a current stable Rust toolchain, then:

```sh
cargo build --release
```

The standalone binary is created at `target/release/dropcast`. To install it in
Cargo's binary directory:

```sh
cargo install --path .
```

## Run

```sh
dropcast "/path/to/movie.mp4"
```

Use the arrow keys and Enter to choose the TV. Keep `dropcast` running while the
movie plays; press Ctrl+C to stop playback.

Skip the picker when a device name is unambiguous:

```sh
dropcast movie.mp4 --device "Living Room"
```

Add one or more subtitle files explicitly:

```sh
dropcast movie.mp4 --subtitle english.srt --subtitle polish.vtt
```

Explicit files are listed first, followed by matching sidecars, then embedded
text tracks. The TV's subtitle menu can switch between available tracks.

## Options

```text
-d, --device <name>       Select a device by name
-s, --subtitle <file>     Add a subtitle file; may be repeated
    --scan-timeout <secs> Scan duration in seconds (default: 5)
-p, --port <port>         Fixed streaming port (default: automatic)
-h, --help                Show command help
-v, --version             Show the version
```

## Subtitle support

WebVTT and SRT sidecars are handled natively. ASS and SSA sidecars, plus
embedded text tracks, require `ffmpeg` and `ffprobe` on `PATH`; keeping these
large media tools external lets the `dropcast` binary stay small. Bitmap
subtitles such as PGS and VobSub cannot be used through the Cast text-track API.

## Troubleshooting

- **No devices found:** confirm the computer and TV are on the same non-guest
  network. Client isolation, some VPNs, and firewalls block mDNS.
- **The receiver opens but playback fails:** the container or codecs are
  probably unsupported by the TV. H.264 video and AAC audio in MP4 is the
  safest choice.
- **Playback starts and immediately stops:** allow incoming `dropcast`
  connections through the computer's firewall. The TV fetches the movie from a
  randomized local URL.
- **Embedded subtitles are skipped:** install FFmpeg so both `ffmpeg` and
  `ffprobe` are available.

`dropcast` streams the movie directly and does not transcode it.
