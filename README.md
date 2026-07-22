# dropcast

A small, fast Rust CLI for streaming a local movie to a Chromecast or a TV with
Google Cast built in.

`dropcast` discovers receivers on the local network, presents an arrow-key
device picker, and serves the movie with byte-range support for seeking. It also
discovers subtitle tracks and lets you switch between them during playback.

## Features

- Small native Rust executable with no Node.js runtime
- Google Cast discovery over mDNS
- Keyboard-navigable receiver picker
- Direct LAN streaming with HTTP byte ranges
- Automatic matching sidecars such as `movie.en.srt`
- Bundled FFmpeg for embedded text subtitle discovery and conversion
- Repeatable explicit `--subtitle` files
- Live keyboard subtitle picker with a `None` option selected by default

## Build

Install a current stable Rust toolchain, then:

```sh
cargo build --release
```

The build creates two files that must remain together:

```text
target/release/dropcast
target/release/dropcast-ffmpeg
```

The FFmpeg companion is unpacked during the build, not at runtime. A release
archive compresses it efficiently while installation and startup require no
cache extraction.

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
text tracks. During playback, use the arrow keys and Enter in `dropcast` to
switch tracks; `None` disables subtitles and is selected initially.

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

WebVTT and SRT sidecars are handled natively. The `dropcast-ffmpeg` companion
handles ASS/SSA and embedded text tracks. No system FFmpeg or ffprobe
installation is required.
Bitmap subtitles such as PGS and VobSub cannot be used through the Cast
text-track API.

## Troubleshooting

- **No devices found:** confirm the computer and TV are on the same non-guest
  network. Client isolation, some VPNs, and firewalls block mDNS.
- **The receiver opens but playback fails:** the container or codecs are
  probably unsupported by the TV. H.264 video and AAC audio in MP4 is the
  safest choice.
- **Playback starts and immediately stops:** allow incoming `dropcast`
  connections through the computer's firewall. The TV fetches the movie from a
  randomized local URL.
- **Bundled FFmpeg was not found:** keep `dropcast-ffmpeg` in the same directory
  as `dropcast`. If you move or install the app, move both files together.

`dropcast` streams the movie directly and does not transcode it.

## Bundled FFmpeg

The FFmpeg executable is downloaded from the pinned `ffmpeg-static` `b6.1.1`
release at compile time and placed beside `dropcast`. It is licensed separately
under the terms described in [THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).
