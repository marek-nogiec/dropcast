use sha2::{Digest, Sha256};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

const RELEASE: &str = "b6.1.1";
const BASE_URL: &str = "https://github.com/eugeneware/ffmpeg-static/releases/download";

fn target_asset() -> (&'static str, &'static str) {
    let os = env::var("CARGO_CFG_TARGET_OS").expect("Cargo did not provide target OS");
    let arch =
        env::var("CARGO_CFG_TARGET_ARCH").expect("Cargo did not provide target architecture");

    match (os.as_str(), arch.as_str()) {
        ("macos", "aarch64") => (
            "ffmpeg-darwin-arm64.gz",
            "8923876afa8db5585022d7860ec7e589af192f441c56793971276d450ed3bbfa",
        ),
        ("macos", "x86_64") => (
            "ffmpeg-darwin-x64.gz",
            "929b375c1182d956c51f7ac25e0b2b0411fb01f6f407aa15c9758efeb4242106",
        ),
        ("linux", "aarch64") => (
            "ffmpeg-linux-arm64.gz",
            "754a678672298bc68156adff58aa7385a592c2b30b1d0ae8750c45c915c4bac0",
        ),
        ("linux", "x86_64") => (
            "ffmpeg-linux-x64.gz",
            "bfe8a8fc511530457b528c48d77b5737527b504a3797a9bc4866aeca69c2dffa",
        ),
        ("windows", "x86_64") => (
            "ffmpeg-win32-x64.gz",
            "8883a3dffbd0a16cf4ef95206ea05283f78908dbfb118f73c83f4951dcc06d77",
        ),
        _ => panic!("dropcast has no bundled FFmpeg build for {arch}-{os}"),
    }
}

fn verify(path: &Path, expected: &str) {
    let bytes = fs::read(path).expect("could not read bundled FFmpeg archive");
    let mut actual = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(&mut actual, "{byte:02x}").expect("writing to a string cannot fail");
    }
    assert_eq!(
        actual,
        expected,
        "bundled FFmpeg archive checksum mismatch; remove {} and rebuild",
        path.display()
    );
}

fn download(url: &str, destination: &Path) {
    println!("cargo:warning=Downloading pinned FFmpeg bundle from {url}");
    let mut response = ureq::get(url)
        .call()
        .unwrap_or_else(|error| panic!("could not download bundled FFmpeg: {error}"));
    let bytes = response
        .body_mut()
        .with_config()
        .limit(100 * 1024 * 1024)
        .read_to_vec()
        .expect("could not read bundled FFmpeg download");
    let temporary = destination.with_extension("gz.part");
    fs::write(&temporary, bytes).expect("could not write bundled FFmpeg archive");
    fs::rename(&temporary, destination).expect("could not cache bundled FFmpeg archive");
}

fn cache_path(asset: &str) -> PathBuf {
    let cargo_home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")))
        .expect("CARGO_HOME or HOME is required to cache bundled FFmpeg");
    cargo_home
        .join("dropcast-bundles")
        .join(RELEASE)
        .join(asset)
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=DROPCAST_FFMPEG_ARCHIVE");

    let (asset, checksum) = target_asset();
    let archive = if let Some(path) = env::var_os("DROPCAST_FFMPEG_ARCHIVE") {
        PathBuf::from(path)
    } else {
        let path = cache_path(asset);
        fs::create_dir_all(path.parent().expect("bundle cache has no parent"))
            .expect("could not create FFmpeg bundle cache");
        if !path.is_file() {
            download(&format!("{BASE_URL}/{RELEASE}/{asset}"), &path);
        }
        path
    };

    verify(&archive, checksum);
    println!(
        "cargo:rustc-env=DROPCAST_FFMPEG_ARCHIVE={}",
        archive.display()
    );
    println!("cargo:rustc-env=DROPCAST_FFMPEG_RELEASE={RELEASE}");
}
