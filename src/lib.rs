mod bundled_ffmpeg;
pub mod cast;
pub mod discovery;
mod range;
mod server;
mod subtitles;

use std::error::Error;
use std::net::{IpAddr, UdpSocket};
use std::path::{Path, PathBuf};

pub use cast::{CastControl, CastEvent, CastIo, CastOutcome, PlaybackStatus};
pub use discovery::CastDevice;

pub type DynError = Box<dyn Error + Send + Sync>;

pub fn validate_movie(path: &Path) -> Result<PathBuf, DynError> {
    let path = std::fs::canonicalize(path)?;
    let metadata = std::fs::metadata(&path)?;
    if !metadata.is_file() {
        return Err(std::io::Error::other(format!("Not a file: {}", path.display())).into());
    }
    if metadata.len() == 0 {
        return Err(
            std::io::Error::other(format!("Movie file is empty: {}", path.display())).into(),
        );
    }
    Ok(path)
}

fn local_address_for(remote: IpAddr) -> Result<IpAddr, DynError> {
    let bind_address = if remote.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    let socket = UdpSocket::bind(bind_address)?;
    socket.connect((remote, 9))?;
    Ok(socket.local_addr()?.ip())
}

fn url_host(address: IpAddr) -> String {
    match address {
        IpAddr::V4(address) => address.to_string(),
        IpAddr::V6(address) => format!("[{address}]"),
    }
}

pub fn cast_movie(
    movie: &Path,
    device: &CastDevice,
    explicit_subtitles: &[PathBuf],
    port: u16,
    io: CastIo,
) -> Result<CastOutcome, DynError> {
    let movie = validate_movie(movie)?;
    let local_address = local_address_for(device.address)?;

    let prepared = subtitles::prepare(&movie, explicit_subtitles)?;
    if io.terminal_ui {
        for warning in &prepared.warnings {
            eprintln!("dropcast: {warning}");
        }
    }
    let server = server::MediaServer::start(&movie, port, &prepared.tracks)?;
    let base_url = format!("http://{}:{}", url_host(local_address), server.port);
    let media_url = format!("{base_url}{}", server.media_path);
    let cast_subtitles: Vec<_> = server
        .subtitles
        .iter()
        .map(|subtitle| cast::CastSubtitle {
            url: format!("{base_url}{}", subtitle.path),
            name: subtitle.name.clone(),
            language: subtitle.language.clone(),
        })
        .collect();
    let title = movie
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Movie")
        .to_owned();
    let content_type = server::movie_content_type(&movie).to_owned();

    let outcome = smol::block_on(cast::run(
        device,
        media_url,
        content_type,
        title,
        cast_subtitles,
        io,
    ))?;
    drop(server);
    drop(prepared);
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_a_non_empty_file() {
        let path = std::env::current_exe().unwrap();
        assert_eq!(validate_movie(&path).unwrap(), path.canonicalize().unwrap());
    }

    #[test]
    fn rejects_a_directory_as_a_movie() {
        let error = validate_movie(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap_err();
        assert!(error.to_string().contains("Not a file"));
    }
}
