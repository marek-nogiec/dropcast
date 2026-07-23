# Dropcast

A native macOS app for streaming a local movie to a Chromecast or a TV with
Google Cast built in. Drop a movie onto the window, choose a TV, and control
playback without uploading the file anywhere.

Dropcast discovers receivers on the local network and serves the movie with
byte-range support for seeking. The app includes playback, volume, and subtitle
controls, follows the macOS light or dark appearance, and keeps the complete
casting flow in one native window.

The release also includes a command-line companion for terminal users.

## Features

- Native macOS drag-and-drop app for Apple Silicon
- Receiver picker with explicit confirmation before casting
- Playback, seeking, volume, stop, and subtitle controls
- Automatic light and dark appearances
- Google Cast discovery over mDNS
- Direct LAN streaming with HTTP byte ranges
- Automatic matching sidecars such as `movie.en.srt`
- Bundled FFmpeg for embedded text subtitle discovery and conversion
- Repeatable explicit `--subtitle` files
- Optional keyboard-driven CLI with receiver and live subtitle pickers

## Build from source

Install a current stable Rust toolchain and the standard C build tools, then
build the pinned FFmpeg source and the release binaries:

```sh
scripts/build-ffmpeg.sh
cargo build --release
```

On an Apple Silicon Mac, package the native app:

```sh
scripts/package-macos-app.sh
open target/macos/Dropcast.app
```

The app bundle is written to `target/macos/Dropcast.app`. To launch the UI
directly during development:

```sh
cargo run --release --bin dropcast-app
```

The packaging script supplies the Finder and Dock icon, local-network privacy
description, Bonjour service declaration, and an ad-hoc signature suitable for
local use. To sign with an installed Developer ID certificate instead, set
`CODE_SIGN_IDENTITY`:

```sh
CODE_SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)" \
  scripts/package-macos-app.sh
```

Distribution outside your own Mac also requires Apple notarization (normally
with `xcrun notarytool`) and stapling the accepted ticket to the app. The script
deliberately does not send artifacts or credentials to Apple.

The optional CLI is created at `target/release/dropcast`. After building the
FFmpeg bundle, install it in Cargo's binary directory with:

```sh
cargo install --path .
```

## Download

[GitHub releases](https://github.com/marek-nogiec/dropcast/releases/latest)
include a native macOS ARM64 app for Apple Silicon. Download the `.app.zip` and
matching `SHA256SUMS.txt`, verify it, then extract and move `Dropcast.app` to
Applications:

```sh
grep 'macos-arm64.app.zip' dropcast-*-SHA256SUMS.txt | shasum -a 256 -c -
ditto -x -k dropcast-*-macos-arm64.app.zip .
sudo mv Dropcast.app /Applications/
```

The release also includes the command-line version as a `.tar.gz`:

```sh
grep 'macos-arm64.tar.gz' dropcast-*-SHA256SUMS.txt | shasum -a 256 -c -
tar -xzf dropcast-*-macos-arm64.tar.gz
sudo install -m 0755 dropcast-*-macos-arm64/dropcast /usr/local/bin/dropcast
```

The app bundle is ad-hoc signed and the CLI is unsigned. Neither artifact is
notarized, so macOS may require explicit approval before first launch.

## Releases

[Release Please](https://github.com/googleapis/release-please) maintains a
release pull request from Conventional Commits that change the Rust source or
build inputs. `fix` and `deps` produce a patch release, `feat` produces a minor
release, and a `!` or `BREAKING CHANGE` footer produces a major release. Other
commit types do not start a release by themselves.

Merging the release pull request updates `Cargo.toml`, `Cargo.lock`, this
changelog, and the release manifest; creates a `v<version>` tag and GitHub
release; and builds and attaches both the macOS app and CLI artifacts.

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

## Command-line bonus

The CLI offers the same direct-LAN streaming core in a keyboard-driven flow:

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

The FFmpeg executable is built from pinned official FFmpeg 6.1.6 source with
external libraries, GPL components, and nonfree components disabled. The build
script verifies the source SHA-256 and records the configuration before
compressing the executable for embedding. See
[THIRD_PARTY_LICENSES.md](THIRD_PARTY_LICENSES.md).

## Contributing and security

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) before opening
a pull request. Please report suspected vulnerabilities using the process in
[SECURITY.md](SECURITY.md), not a public issue.

## License

`dropcast` is licensed under the GNU General Public License, version 3 or (at
your option) any later version. See [LICENSE](LICENSE). Third-party components
remain under their respective licenses.
