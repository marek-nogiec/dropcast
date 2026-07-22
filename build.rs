use std::env;
use std::path::PathBuf;

const RELEASE: &str = "6.1.6";

fn target_asset() -> &'static str {
    let os = env::var("CARGO_CFG_TARGET_OS").expect("Cargo did not provide target OS");
    let arch =
        env::var("CARGO_CFG_TARGET_ARCH").expect("Cargo did not provide target architecture");

    match (os.as_str(), arch.as_str()) {
        ("macos", "aarch64") => "ffmpeg-darwin-arm64.gz",
        ("macos", "x86_64") => "ffmpeg-darwin-x64.gz",
        ("linux", "aarch64") => "ffmpeg-linux-arm64.gz",
        ("linux", "x86_64") => "ffmpeg-linux-x64.gz",
        ("windows", "x86_64") => "ffmpeg-win32-x64.gz",
        _ => panic!("dropcast has no bundled FFmpeg build for {arch}-{os}"),
    }
}

fn default_archive(asset: &str) -> PathBuf {
    PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo did not provide manifest dir"))
        .join("target/ffmpeg-bundles")
        .join(asset)
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=DROPCAST_FFMPEG_ARCHIVE");

    let asset = target_asset();
    let archive = env::var_os("DROPCAST_FFMPEG_ARCHIVE")
        .map(PathBuf::from)
        .unwrap_or_else(|| default_archive(asset));
    assert!(
        archive.is_file(),
        "FFmpeg bundle not found at {}; run scripts/build-ffmpeg.sh or set DROPCAST_FFMPEG_ARCHIVE",
        archive.display()
    );
    println!(
        "cargo:rustc-env=DROPCAST_FFMPEG_ARCHIVE={}",
        archive.display()
    );
    println!("cargo:rustc-env=DROPCAST_FFMPEG_RELEASE={RELEASE}");
}
