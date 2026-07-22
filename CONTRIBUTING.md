# Contributing

Contributions are welcome through GitHub issues and pull requests.

## Before opening a pull request

Install the current stable Rust toolchain. Optionally install
[Lefthook](https://lefthook.dev/installation/) and run `lefthook install` to
enable the Conventional Commit check.

Run the same checks as CI:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

Tests bind a loopback socket and execute the bundled FFmpeg. Offline builds can
set `DROPCAST_FFMPEG_ARCHIVE` to the matching archive pinned in `build.rs`.

Keep changes focused, add tests for behavior changes, and update documentation
when user-visible behavior changes. Use a Conventional Commit subject such as
`fix(server): reject an invalid range` or `feat: add a device flag`.

## Pull requests

Describe the problem and solution, list the checks you ran, and call out any
release, network, dependency, or licensing impact. Do not commit media,
subtitles, build output, generated files, or credentials.

By contributing, you agree that your contribution is licensed under
GPL-3.0-or-later.

Report security concerns according to [SECURITY.md](SECURITY.md), not through a
public issue.
