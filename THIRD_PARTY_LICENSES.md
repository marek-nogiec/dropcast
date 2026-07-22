# Third-party notices

## FFmpeg

`dropcast` embeds a separately executed FFmpeg 6.1.6 program built from official
source by `scripts/build-ffmpeg.sh`. The build disables external libraries,
GPL components, and nonfree components; the resulting FFmpeg program is
licensed under LGPL 2.1 or later.

The exact corresponding source and reproducible build instructions are:

- Source: <https://ffmpeg.org/releases/ffmpeg-6.1.6.tar.xz>
- SHA-256: `d4fcb164028dd3beee5d92c0ac72e46aac6973c75ea12dc14de07bf8f407370a`
- Build script: [`scripts/build-ffmpeg.sh`](scripts/build-ffmpeg.sh)
- License text: [`LICENSES/FFmpeg-LGPL-2.1.txt`](LICENSES/FFmpeg-LGPL-2.1.txt)

Binary releases also attach the exact `ffmpeg-6.1.6.tar.xz` source archive used
for the bundled executable.

License and project information:

- [FFmpeg legal information](https://ffmpeg.org/legal.html)
- [FFmpeg source downloads](https://ffmpeg.org/download.html#get-sources)

Copyright belongs to the FFmpeg developers and other respective contributors.

## Rust dependencies

`dropcast` incorporates Rust crates under their respective licenses. In
particular, `smol-timeout` is licensed under MPL-2.0 and is available from
<https://crates.io/crates/smol-timeout>. Its source remains governed by the
[Mozilla Public License 2.0](LICENSES/MPL-2.0.txt). The unmodified source for
the exact version used by each release is identified by `Cargo.lock` and can be
downloaded from crates.io.
