use std::env;
use std::path::PathBuf;

use crate::DynError;

pub fn path() -> Result<PathBuf, DynError> {
    if let Some(path) = env::var_os("DROPCAST_FFMPEG") {
        return validate(PathBuf::from(path));
    }

    let executable = if cfg!(windows) {
        "dropcast-ffmpeg.exe"
    } else {
        "dropcast-ffmpeg"
    };
    let directory = env::current_exe()?
        .parent()
        .ok_or_else(|| std::io::Error::other("dropcast executable has no parent directory"))?
        .to_owned();
    let sibling = directory.join(executable);
    if sibling.is_file() {
        return Ok(sibling);
    }

    // Cargo puts test executables in target/<profile>/deps while build-script
    // artifacts live one directory higher.
    if directory.file_name().is_some_and(|name| name == "deps")
        && let Some(profile_dir) = directory.parent()
    {
        let test_sibling = profile_dir.join(executable);
        if test_sibling.is_file() {
            return Ok(test_sibling);
        }
    }
    validate(sibling)
}

fn validate(path: PathBuf) -> Result<PathBuf, DynError> {
    if path.metadata().is_ok_and(|metadata| metadata.is_file()) {
        return Ok(path);
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::NotFound,
        format!(
            "bundled FFmpeg was not found at {}; keep dropcast and dropcast-ffmpeg together",
            path.display()
        ),
    )
    .into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_ffmpeg_is_next_to_the_test_binary() {
        assert!(path().unwrap().metadata().unwrap().len() > 1_000_000);
    }
}
