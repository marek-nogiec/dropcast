use std::collections::HashMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tiny_http::{Header, Method, Request, Response, Server, StatusCode};

use crate::DynError;
use crate::range::{self, ByteRange};
use crate::subtitles::SubtitleTrack;

#[derive(Clone, Debug)]
struct Resource {
    path: PathBuf,
    size: u64,
    content_type: String,
}

#[derive(Clone, Debug)]
pub struct ServedSubtitle {
    pub name: String,
    pub language: Option<String>,
    pub path: String,
}

pub struct MediaServer {
    pub port: u16,
    pub media_path: String,
    pub subtitles: Vec<ServedSubtitle>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

pub(crate) fn movie_content_type(path: &Path) -> &'static str {
    match path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_lowercase()
        .as_str()
    {
        "mp4" | "m4v" => "video/mp4",
        "webm" => "video/webm",
        "mkv" => "video/x-matroska",
        "mov" => "video/quicktime",
        "avi" => "video/x-msvideo",
        "mpeg" | "mpg" => "video/mpeg",
        "ts" | "m2ts" => "video/mp2t",
        "ogv" => "video/ogg",
        _ => "application/octet-stream",
    }
}

fn header(name: &str, value: &str) -> Header {
    Header::from_bytes(name.as_bytes(), value.as_bytes()).expect("valid static HTTP header")
}

fn common_headers(content_type: &str) -> Vec<Header> {
    vec![
        header("Accept-Ranges", "bytes"),
        header("Access-Control-Allow-Origin", "*"),
        header(
            "Access-Control-Expose-Headers",
            "Content-Length, Content-Range",
        ),
        header("Cache-Control", "no-store"),
        header("Content-Type", content_type),
    ]
}

fn request_range(request: &Request) -> Option<&str> {
    request
        .headers()
        .iter()
        .find(|header| header.field.equiv("Range"))
        .map(|header| header.value.as_str())
}

fn respond(request: Request, resources: &HashMap<String, Resource>) {
    let path = request.url().split('?').next().unwrap_or(request.url());
    let Some(resource) = resources.get(path) else {
        let _ = request.respond(Response::from_string("Not found").with_status_code(404));
        return;
    };

    if !matches!(request.method(), Method::Get | Method::Head) {
        let response = Response::from_string("Method not allowed")
            .with_status_code(405)
            .with_header(header("Allow", "GET, HEAD"));
        let _ = request.respond(response);
        return;
    }

    let byte_range = range::parse(request_range(&request), resource.size);
    let mut headers = common_headers(&resource.content_type);
    let (status, start, length) = match byte_range {
        ByteRange::Full => (200, 0, resource.size),
        ByteRange::Partial { start, end } => {
            headers.push(header(
                "Content-Range",
                &format!("bytes {start}-{end}/{}", resource.size),
            ));
            (206, start, end - start + 1)
        }
        ByteRange::Invalid => {
            headers.push(header(
                "Content-Range",
                &format!("bytes */{}", resource.size),
            ));
            let response = Response::new(StatusCode(416), headers, std::io::empty(), Some(0), None);
            let _ = request.respond(response);
            return;
        }
    };

    let Ok(mut file) = File::open(&resource.path) else {
        let _ = request.respond(Response::empty(500));
        return;
    };
    if file.seek(SeekFrom::Start(start)).is_err() {
        let _ = request.respond(Response::empty(500));
        return;
    }

    let reader: Box<dyn Read + Send> = if length == resource.size {
        Box::new(file)
    } else {
        Box::new(file.take(length))
    };
    let response = Response::new(
        StatusCode(status),
        headers,
        reader,
        usize::try_from(length).ok(),
        None,
    );
    let _ = request.respond(response);
}

impl MediaServer {
    pub fn start(
        movie: &Path,
        preferred_port: u16,
        subtitle_tracks: &[SubtitleTrack],
    ) -> Result<Self, DynError> {
        let movie_metadata = fs_metadata_file(movie)?;
        let server = Server::http(("0.0.0.0", preferred_port))?;
        let port = server
            .server_addr()
            .to_ip()
            .ok_or_else(|| std::io::Error::other("HTTP server did not bind an IP socket"))?
            .port();
        let token = format!(
            "{:x}{:x}",
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos(),
            std::process::id()
        );
        let media_path = format!("/{token}/media");
        let mut resources = HashMap::new();
        resources.insert(
            media_path.clone(),
            Resource {
                path: movie.to_owned(),
                size: movie_metadata,
                content_type: movie_content_type(movie).to_owned(),
            },
        );

        let mut subtitles = Vec::new();
        for (index, track) in subtitle_tracks.iter().enumerate() {
            let path = format!("/{token}/subtitle-{}.vtt", index + 1);
            resources.insert(
                path.clone(),
                Resource {
                    path: track.path.clone(),
                    size: fs_metadata_file(&track.path)?,
                    content_type: "text/vtt; charset=utf-8".to_owned(),
                },
            );
            subtitles.push(ServedSubtitle {
                name: track.name.clone(),
                language: track.language.clone(),
                path,
            });
        }

        let stop = Arc::new(AtomicBool::new(false));
        let thread_stop = stop.clone();
        let thread = thread::spawn(move || {
            while !thread_stop.load(Ordering::Relaxed) {
                if let Ok(Some(request)) = server.recv_timeout(Duration::from_millis(100)) {
                    respond(request, &resources);
                }
            }
        });

        Ok(Self {
            port,
            media_path,
            subtitles,
            stop,
            thread: Some(thread),
        })
    }
}

fn fs_metadata_file(path: &Path) -> Result<u64, DynError> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(
            std::io::Error::other(format!("Not a regular file: {}", path.display())).into(),
        );
    }
    Ok(metadata.len())
}

impl Drop for MediaServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        // Dropping the handle detaches the thread. This avoids blocking shutdown
        // if a receiver still has a large file response open; the process is
        // exiting and the socket will close immediately afterwards.
        let _ = self.thread.take();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::{Read, Write};
    use std::net::TcpStream;

    fn test_file() -> PathBuf {
        std::env::temp_dir().join(format!(
            "dropcast-server-test-{}-{}.mp4",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    fn request(port: u16, path: &str, range: Option<&str>) -> Vec<u8> {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        let range = range
            .map(|value| format!("Range: {value}\r\n"))
            .unwrap_or_default();
        write!(
            stream,
            "GET {path} HTTP/1.0\r\nHost: localhost\r\n{range}\r\n"
        )
        .unwrap();
        let mut response = Vec::new();
        stream.read_to_end(&mut response).unwrap();
        response
    }

    #[test]
    fn serves_full_files_and_byte_ranges() {
        let path = test_file();
        fs::write(&path, b"0123456789").unwrap();
        let server = MediaServer::start(&path, 0, &[]).unwrap();

        let full = request(server.port, &server.media_path, None);
        let full = String::from_utf8(full).unwrap();
        assert!(
            full.lines()
                .next()
                .is_some_and(|line| line.contains(" 200 "))
        );
        assert!(full.to_lowercase().contains("accept-ranges: bytes"));
        assert!(full.ends_with("0123456789"));

        let partial = request(server.port, &server.media_path, Some("bytes=3-6"));
        let partial = String::from_utf8(partial).unwrap();
        assert!(
            partial
                .lines()
                .next()
                .is_some_and(|line| line.contains(" 206 "))
        );
        assert!(partial.contains("Content-Range: bytes 3-6/10"));
        assert!(partial.ends_with("3456"));

        drop(server);
        let _ = fs::remove_file(path);
    }
}
