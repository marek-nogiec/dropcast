# Repository guidance

`dropcast` is a Rust 2024 CLI. Application code lives in `src/`; `build.rs`
downloads a pinned FFmpeg archive and verifies its SHA-256 before embedding it.
Keep `Cargo.lock` committed and avoid committing media, subtitle, `target/`, or
other generated files.

Before submitting changes, run:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

Tests need permission to bind a local socket and execute the bundled FFmpeg.
For offline builds, set `DROPCAST_FFMPEG_ARCHIVE` to the matching pinned archive.
Add or update focused unit tests with behavior changes.

`.github/workflows/release.yml` runs Release Please after pushes to `main`.
Treat changes to release triggers, permissions, checksums, and packaging as
security-sensitive. Use Conventional Commit subjects so releases are versioned
correctly.
