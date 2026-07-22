use flate2::read::GzDecoder;
use std::env;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::DynError;

const ARCHIVE: &[u8] = include_bytes!(env!("DROPCAST_FFMPEG_ARCHIVE"));
const RELEASE: &str = env!("DROPCAST_FFMPEG_RELEASE");
static TEMPORARY_ID: AtomicU64 = AtomicU64::new(0);

fn cache_root() -> PathBuf {
    if let Some(path) = env::var_os("DROPCAST_CACHE_DIR") {
        return PathBuf::from(path);
    }

    #[cfg(target_os = "macos")]
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home).join("Library/Caches/dropcast");
    }

    #[cfg(target_os = "windows")]
    if let Some(path) = env::var_os("LOCALAPPDATA") {
        return PathBuf::from(path).join("dropcast");
    }

    if let Some(path) = env::var_os("XDG_CACHE_HOME") {
        return PathBuf::from(path).join("dropcast");
    }
    if let Some(home) = env::var_os("HOME") {
        return PathBuf::from(home).join(".cache/dropcast");
    }
    env::temp_dir().join("dropcast")
}

fn unpacked_size() -> Option<u64> {
    let bytes = ARCHIVE.get(ARCHIVE.len().checked_sub(4)?..)?;
    Some(u64::from(u32::from_le_bytes(bytes.try_into().ok()?)))
}

fn is_complete(path: &Path) -> bool {
    path.metadata()
        .ok()
        .filter(|metadata| metadata.is_file())
        .is_some_and(|metadata| Some(metadata.len()) == unpacked_size())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = path.metadata()?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

pub fn path() -> Result<PathBuf, DynError> {
    let directory = cache_root();
    fs::create_dir_all(&directory)?;
    let executable = if cfg!(windows) {
        "ffmpeg.exe"
    } else {
        "ffmpeg"
    };
    let destination = directory.join(format!("{executable}-{RELEASE}"));
    if is_complete(&destination) {
        make_executable(&destination)?;
        return Ok(destination);
    }

    let id = TEMPORARY_ID.fetch_add(1, Ordering::Relaxed);
    let temporary = directory.join(format!(
        ".{executable}-{RELEASE}-{}-{id}.tmp",
        std::process::id()
    ));
    let unpacked = (|| -> Result<(), DynError> {
        let mut decoder = GzDecoder::new(ARCHIVE);
        let mut output = File::create(&temporary)?;
        io::copy(&mut decoder, &mut output)?;
        output.sync_all()?;
        make_executable(&temporary)?;
        Ok(())
    })();
    if let Err(error) = unpacked {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }

    match fs::rename(&temporary, &destination) {
        Ok(()) => {}
        Err(_) if is_complete(&destination) => {
            let _ = fs::remove_file(&temporary);
        }
        Err(_) => {
            if destination.exists() {
                fs::remove_file(&destination)?;
            }
            fs::rename(&temporary, &destination)?;
        }
    }
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_archive_has_a_nonzero_unpacked_size() {
        assert!(unpacked_size().is_some_and(|size| size > 1_000_000));
    }
}
