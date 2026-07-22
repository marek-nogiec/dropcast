# dropcast

A small, fast Rust CLI for streaming a local movie to a Chromecast or a TV with
Google Cast built in.

`dropcast` discovers receivers on the local network, presents an arrow-key
device picker, and serves the movie with byte-range support for seeking. It also
discovers subtitle tracks and lets you switch between them during playback.

## Features

- Single native executable with no Node.js runtime
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

The standalone executable is created at:

```text
target/release/dropcast
```

FFmpeg is compressed inside that executable. To install it in Cargo's binary
directory:

```sh
cargo install --path .
```

## Download

Each GitHub release contains a native macOS ARM64 build for Apple Silicon.
Download the `.tar.gz` archive and its matching `SHA256SUMS.txt` file into the
same directory, then verify and install it with:

```sh
shasum -a 256 -c dropcast-*-SHA256SUMS.txt
tar -xzf dropcast-*-macos-arm64.tar.gz
sudo install -m 0755 dropcast-*-macos-arm64/dropcast /usr/local/bin/dropcast
```

The release binaries are currently unsigned and not notarized.

## Releases

[Release Please](https://github.com/googleapis/release-please) maintains a
release pull request from Conventional Commits that change the Rust source or
build inputs. `fix` and `deps` produce a patch release, `feat` produces a minor
release, and a `!` or `BREAKING CHANGE` footer produces a major release. Other
commit types do not start a release by themselves.

Merging the release pull request updates `Cargo.toml`, `Cargo.lock`, this
changelog, and the release manifest; creates a `v<version>` tag and GitHub
release; and builds and attaches the macOS artifact.

## Development

Install [Cocogitto](https://docs.cocogitto.io/) and enable its commit-message
hook after cloning:

```sh
brew install cocogitto
cog install-hook commit-msg
```

Use commands such as `cog commit feat "add playback queue support" cast` to
create commits. The installed hook verifies commits made through regular Git as
well, and the same check runs in GitHub Actions for pushes and pull requests.
The project also accepts the custom `deps` type used by Release Please.

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

WebVTT and SRT sidecars are handled natively. The FFmpeg payload embedded in
`dropcast` handles ASS/SSA and embedded text tracks. No system FFmpeg or ffprobe
installation is required. It is unpacked into the user cache when first needed
because operating systems cannot execute a program directly from embedded
bytes.
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
- **The first subtitle scan takes a moment:** the embedded FFmpeg payload is
  unpacked into the user cache once; subsequent runs reuse it.

`dropcast` streams the movie directly and does not transcode it.

## Bundled FFmpeg

The FFmpeg executable is downloaded from the pinned `ffmpeg-static` `b6.1.1`
release at compile time and compressed into `dropcast`. It is licensed
separately under the terms described in
[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).
