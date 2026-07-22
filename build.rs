use sha2::{Digest, Sha256};
use std::env;
use std::fmt::Write as _;
use std::fs;
use std::fs::File;
use std::io;
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

fn unpacked_size(archive: &Path) -> u64 {
    let bytes = fs::read(archive).expect("could not read bundled FFmpeg archive");
    let trailer = bytes
        .get(bytes.len().checked_sub(4).expect("FFmpeg archive is empty")..)
        .expect("FFmpeg archive has no gzip size trailer");
    u64::from(u32::from_le_bytes(
        trailer.try_into().expect("gzip size trailer is four bytes"),
    ))
}

fn make_executable(path: &Path) -> io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = path.metadata()?.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)?;
    }
    Ok(())
}

fn install_ffmpeg(archive: &Path, destination: &Path) {
    let expected_size = unpacked_size(archive);
    if destination
        .metadata()
        .is_ok_and(|metadata| metadata.is_file() && metadata.len() == expected_size)
    {
        make_executable(destination).expect("could not mark bundled FFmpeg executable");
        return;
    }

    let temporary = destination.with_extension("tmp");
    let input = File::open(archive).expect("could not open bundled FFmpeg archive");
    let mut decoder = flate2::read::GzDecoder::new(input);
    let mut output = File::create(&temporary).expect("could not create bundled FFmpeg executable");
    io::copy(&mut decoder, &mut output).expect("could not unpack bundled FFmpeg executable");
    output
        .sync_all()
        .expect("could not flush bundled FFmpeg executable");
    make_executable(&temporary).expect("could not mark bundled FFmpeg executable");
    if destination.exists() {
        fs::remove_file(destination).expect("could not replace bundled FFmpeg executable");
    }
    fs::rename(temporary, destination).expect("could not install bundled FFmpeg executable");
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
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("Cargo did not provide OUT_DIR"));
    let profile_dir = out_dir
        .ancestors()
        .nth(3)
        .expect("unexpected Cargo OUT_DIR layout");
    let executable = if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        "dropcast-ffmpeg.exe"
    } else {
        "dropcast-ffmpeg"
    };
    install_ffmpeg(&archive, &profile_dir.join(executable));
}
