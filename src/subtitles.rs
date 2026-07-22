use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::DynError;
use crate::bundled_ffmpeg;

const SIDECAR_EXTENSIONS: &[&str] = &["ass", "srt", "ssa", "vtt"];
const TEXT_CODECS: &[&str] = &[
    "ass", "mov_text", "ssa", "srt", "subrip", "text", "ttml", "webvtt",
];

#[derive(Clone, Debug)]
pub struct SubtitleTrack {
    pub path: PathBuf,
    pub name: String,
    pub language: Option<String>,
}

pub struct PreparedSubtitles {
    pub tracks: Vec<SubtitleTrack>,
    pub warnings: Vec<String>,
    temp_dir: PathBuf,
}

impl Drop for PreparedSubtitles {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.temp_dir);
    }
}

#[derive(Debug, PartialEq, Eq)]
struct ProbeStream {
    index: u32,
    codec_name: String,
    tags: HashMap<String, String>,
}

fn unique_temp_dir() -> Result<PathBuf, DynError> {
    let base = std::env::temp_dir();
    for attempt in 0..100u32 {
        let nanos = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let path = base.join(format!(
            "dropcast-subtitles-{}-{nanos:x}-{attempt}",
            std::process::id()
        ));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(std::io::Error::other("could not create a temporary subtitle directory").into())
}

fn extension(path: &Path) -> String {
    path.extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_lowercase()
}

fn language_from_sidecar(movie: &Path, subtitle: &Path) -> Option<String> {
    let movie_name = movie.file_stem()?.to_str()?;
    let subtitle_name = subtitle.file_stem()?.to_str()?;
    let suffix = subtitle_name
        .strip_prefix(movie_name)?
        .strip_prefix('.')
        .unwrap_or_default();
    (!suffix.is_empty()).then(|| suffix.split('.').next().unwrap_or(suffix).to_owned())
}

pub fn find_sidecars(movie: &Path) -> Result<Vec<PathBuf>, DynError> {
    let directory = movie.parent().unwrap_or_else(|| Path::new("."));
    let movie_name = movie
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or_default()
        .to_lowercase();
    let mut paths = Vec::new();

    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if !SIDECAR_EXTENSIONS.contains(&extension(&path).as_str()) {
            continue;
        }
        let candidate = path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or_default()
            .to_lowercase();
        if candidate == movie_name || candidate.starts_with(&format!("{movie_name}.")) {
            paths.push(path);
        }
    }

    paths.sort();
    Ok(paths)
}

pub fn srt_to_vtt(input: &str) -> String {
    let normalized = input.trim_start_matches('\u{feff}').replace("\r\n", "\n");
    let mut output = String::from("WEBVTT\n\n");
    for line in normalized.lines() {
        if line.contains("-->") {
            output.push_str(&line.replace(',', "."));
        } else {
            output.push_str(line);
        }
        output.push('\n');
    }
    output
}

fn convert_external(ffmpeg: &Path, source: &Path, output: &Path) -> Result<bool, DynError> {
    match extension(source).as_str() {
        "vtt" => {
            fs::copy(source, output)?;
            Ok(true)
        }
        "srt" => {
            let contents = fs::read_to_string(source)?;
            fs::write(output, srt_to_vtt(&contents))?;
            Ok(true)
        }
        _ => match Command::new(ffmpeg)
            .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i"])
            .arg(source)
            .args(["-c:s", "webvtt"])
            .arg(output)
            .status()
        {
            Ok(status) => Ok(status.success()),
            Err(error) => Err(error.into()),
        },
    }
}

fn parse_stream_listing(output: &str) -> Vec<ProbeStream> {
    let mut streams = Vec::new();
    for line in output.lines() {
        let Some(descriptor) = line.trim().strip_prefix("Stream #") else {
            continue;
        };
        let Some((stream_id, codec_text)) = descriptor.split_once(": Subtitle: ") else {
            continue;
        };
        let Some((_, stream_detail)) = stream_id.split_once(':') else {
            continue;
        };
        let index_text: String = stream_detail
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        let Ok(index) = index_text.parse::<u32>() else {
            continue;
        };
        let codec_name = codec_text
            .split(|character: char| character == ',' || character.is_ascii_whitespace())
            .next()
            .unwrap_or_default()
            .to_lowercase();
        if !TEXT_CODECS.contains(&codec_name.as_str()) {
            continue;
        }

        let mut tags = HashMap::new();
        if let Some(start) = stream_detail.find('(')
            && let Some(end) = stream_detail[start + 1..].find(')')
        {
            tags.insert(
                "language".to_owned(),
                stream_detail[start + 1..start + 1 + end].to_owned(),
            );
        }
        streams.push(ProbeStream {
            index,
            codec_name,
            tags,
        });
    }
    streams.sort_by_key(|stream| stream.index);
    streams
}

fn probe_embedded(ffmpeg: &Path, movie: &Path) -> Result<Vec<ProbeStream>, DynError> {
    let output = Command::new(ffmpeg)
        .args(["-nostdin", "-hide_banner", "-i"])
        .arg(movie)
        .output()?;
    Ok(parse_stream_listing(&String::from_utf8_lossy(
        &output.stderr,
    )))
}

fn extract_embedded(
    ffmpeg: &Path,
    movie: &Path,
    streams: &[ProbeStream],
    temp_dir: &Path,
) -> Result<Vec<SubtitleTrack>, DynError> {
    if streams.is_empty() {
        return Ok(Vec::new());
    }

    let mut command = Command::new(ffmpeg);
    command.args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i"]);
    command.arg(movie);
    let mut outputs = Vec::new();

    for (position, stream) in streams.iter().enumerate() {
        let output = temp_dir.join(format!("embedded-{}.vtt", position + 1));
        command
            .args(["-map", &format!("0:{}", stream.index), "-c:s", "webvtt"])
            .arg(&output);
        outputs.push(output);
    }

    let status = command.status()?;
    if !status.success() {
        return Ok(Vec::new());
    }

    Ok(streams
        .iter()
        .zip(outputs)
        .enumerate()
        .map(|(position, (stream, path))| {
            let language = stream.tags.get("language").cloned();
            let name = stream
                .tags
                .get("title")
                .cloned()
                .or_else(|| language.as_ref().map(|value| format!("{value} (embedded)")))
                .unwrap_or_else(|| format!("Embedded subtitle {}", position + 1));
            SubtitleTrack {
                path,
                name,
                language,
            }
        })
        .collect())
}

pub fn prepare(movie: &Path, explicit: &[PathBuf]) -> Result<PreparedSubtitles, DynError> {
    let temp_dir = unique_temp_dir()?;
    let result = (|| {
        let ffmpeg = bundled_ffmpeg::path()?;
        let mut warnings = Vec::new();
        let mut external = Vec::new();
        let mut seen = HashSet::new();

        for path in explicit {
            let path = fs::canonicalize(path)?;
            let metadata = fs::metadata(&path)?;
            if !metadata.is_file() || metadata.len() == 0 {
                return Err(std::io::Error::other(format!(
                    "Subtitle is not a non-empty file: {}",
                    path.display()
                ))
                .into());
            }
            if seen.insert(path.clone()) {
                external.push(path);
            }
        }
        for path in find_sidecars(movie)? {
            let path = fs::canonicalize(path)?;
            if seen.insert(path.clone()) {
                external.push(path);
            }
        }

        let mut tracks = Vec::new();
        for (position, source) in external.iter().enumerate() {
            let output = temp_dir.join(format!("external-{}.vtt", position + 1));
            if convert_external(&ffmpeg, source, &output)? {
                tracks.push(SubtitleTrack {
                    path: output,
                    name: source
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("Subtitle")
                        .to_owned(),
                    language: language_from_sidecar(movie, source),
                });
            } else {
                warnings.push(format!("Could not convert subtitle {}.", source.display()));
            }
        }

        let streams = probe_embedded(&ffmpeg, movie)?;
        match extract_embedded(&ffmpeg, movie, &streams, &temp_dir)? {
            embedded if !streams.is_empty() && embedded.is_empty() => warnings
                .push("Embedded text subtitles were found but could not be converted.".to_owned()),
            embedded => tracks.extend(embedded),
        }

        Ok(PreparedSubtitles {
            tracks,
            warnings,
            temp_dir: temp_dir.clone(),
        })
    })();

    if result.is_err() {
        let _ = fs::remove_dir_all(temp_dir);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_dir() -> PathBuf {
        std::env::temp_dir().join(format!(
            "dropcast-embedded-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn converts_srt_timestamps_to_webvtt() {
        let input = "\u{feff}1\r\n00:00:00,000 --> 00:00:01,500\r\nHello\r\n";
        let output = srt_to_vtt(input);
        assert_eq!(
            output,
            "WEBVTT\n\n1\n00:00:00.000 --> 00:00:01.500\nHello\n"
        );
    }

    #[test]
    fn identifies_sidecar_language() {
        assert_eq!(
            language_from_sidecar(Path::new("/tmp/movie.mp4"), Path::new("/tmp/movie.en.srt")),
            Some("en".to_owned())
        );
        assert_eq!(
            language_from_sidecar(Path::new("/tmp/movie.mp4"), Path::new("/tmp/captions.srt")),
            None
        );
    }

    #[test]
    fn parses_text_subtitle_streams_and_ignores_bitmap_streams() {
        let listing = r#"
          Stream #0:4(pol): Subtitle: subrip (default)
          Stream #0:2[0x3](eng): Subtitle: mov_text (tx3g)
          Stream #0:5: Subtitle: hdmv_pgs_subtitle
        "#;
        assert_eq!(
            parse_stream_listing(listing),
            vec![
                ProbeStream {
                    index: 2,
                    codec_name: "mov_text".to_owned(),
                    tags: HashMap::from([("language".to_owned(), "eng".to_owned())]),
                },
                ProbeStream {
                    index: 4,
                    codec_name: "subrip".to_owned(),
                    tags: HashMap::from([("language".to_owned(), "pol".to_owned())]),
                },
            ]
        );
    }

    #[test]
    fn discovers_and_extracts_an_embedded_subtitle_with_bundled_ffmpeg() {
        let directory = fixture_dir();
        fs::create_dir(&directory).unwrap();
        let captions = directory.join("captions.srt");
        let movie = directory.join("movie.mp4");
        fs::write(
            &captions,
            "1\n00:00:00,000 --> 00:00:00,800\nBundled subtitle works\n",
        )
        .unwrap();

        let ffmpeg = bundled_ffmpeg::path().unwrap();
        let status = Command::new(ffmpeg)
            .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y", "-i"])
            .arg(&captions)
            .args([
                "-map",
                "0:0",
                "-c:s",
                "mov_text",
                "-metadata:s:s:0",
                "language=eng",
            ])
            .arg(&movie)
            .status()
            .unwrap();
        assert!(status.success());

        let prepared = prepare(&movie, &[]).unwrap();
        assert_eq!(prepared.tracks.len(), 1);
        assert_eq!(prepared.tracks[0].language.as_deref(), Some("eng"));
        assert!(
            fs::read_to_string(&prepared.tracks[0].path)
                .unwrap()
                .contains("Bundled subtitle works")
        );

        drop(prepared);
        fs::remove_dir_all(directory).unwrap();
    }
}
