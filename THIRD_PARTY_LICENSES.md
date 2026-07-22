# Third-party notices

## FFmpeg

`dropcast` bundles a prebuilt FFmpeg executable from the
[`ffmpeg-static` b6.1.1 release](https://github.com/eugeneware/ffmpeg-static/releases/tag/b6.1.1).
FFmpeg is a separate program embedded in compressed form. `dropcast` extracts
and executes it for subtitle inspection and conversion.

FFmpeg is primarily licensed under LGPL 2.1 or later, while builds that enable
GPL components are governed by the GPL. The macOS ARM64 asset currently pinned
by `build.rs` reports `--enable-gpl`, `--enable-version3`, and
`--enable-nonfree` in its build configuration. FFmpeg documents binaries built
with `--enable-nonfree` as unredistributable.

The current asset may be downloaded and used for local builds, but neither it
nor a `dropcast` executable containing it may be redistributed. Automated
binary releases remain disabled until the asset is replaced with a
redistributable build and its corresponding-source obligations are documented.

Upstream license and source information:

- [FFmpeg legal information](https://ffmpeg.org/legal.html)
- [FFmpeg source](https://ffmpeg.org/download.html#get-sources)
- [`ffmpeg-static` build and release source](https://github.com/eugeneware/ffmpeg-static)

Copyright belongs to the FFmpeg developers and other respective contributors.

## Rust dependencies

`dropcast` incorporates Rust crates under their respective licenses. In
particular, `smol-timeout` is licensed under MPL-2.0 and is available from
<https://crates.io/crates/smol-timeout>. Its source remains governed by the
[Mozilla Public License 2.0](https://www.mozilla.org/MPL/2.0/).
