use std::ffi::OsString;
use std::path::PathBuf;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

pub const HELP: &str = r#"dropcast — stream a local movie to a Cast-enabled TV

Usage:
  dropcast <movie> [options]

Options:
  -d, --device <name>     Select a device by name (case-insensitive)
  -s, --subtitle <file>   Add a subtitle file (repeatable)
      --scan-timeout <s>  Scan duration in seconds (default: 5)
  -p, --port <port>       Local HTTP port; 0 picks a free port (default: 0)
  -h, --help              Show this help
  -v, --version           Show the version

The computer and TV must be on the same network. Use the arrow keys and Enter
to choose a receiver and switch subtitles. Press Ctrl+C to stop casting."#;

#[derive(Debug, PartialEq, Eq)]
pub struct CliOptions {
    pub file: Option<PathBuf>,
    pub device: Option<String>,
    pub subtitles: Vec<PathBuf>,
    pub scan_timeout_secs: u64,
    pub port: u16,
    pub help: bool,
    pub version: bool,
}

impl Default for CliOptions {
    fn default() -> Self {
        Self {
            file: None,
            device: None,
            subtitles: Vec::new(),
            scan_timeout_secs: 5,
            port: 0,
            help: false,
            version: false,
        }
    }
}

fn value(args: &[OsString], index: usize, option: &str) -> Result<String, String> {
    let value = args
        .get(index + 1)
        .ok_or_else(|| format!("{option} requires a value"))?
        .to_str()
        .ok_or_else(|| format!("{option} requires valid UTF-8"))?;
    if value.starts_with('-') {
        return Err(format!("{option} requires a value"));
    }
    Ok(value.to_owned())
}

pub fn parse(args: impl IntoIterator<Item = OsString>) -> Result<CliOptions, String> {
    let args: Vec<_> = args.into_iter().collect();
    let mut options = CliOptions::default();
    let mut index = 0;

    while index < args.len() {
        let argument = args[index]
            .to_str()
            .ok_or_else(|| "Arguments must be valid UTF-8".to_owned())?;

        match argument {
            "-h" | "--help" => options.help = true,
            "-v" | "--version" => options.version = true,
            "-d" | "--device" => {
                options.device = Some(value(&args, index, argument)?);
                index += 1;
            }
            "-s" | "--subtitle" => {
                options
                    .subtitles
                    .push(PathBuf::from(value(&args, index, argument)?));
                index += 1;
            }
            "--scan-timeout" => {
                let parsed = value(&args, index, argument)?
                    .parse::<u64>()
                    .map_err(|_| "--scan-timeout must be an integer from 1 to 60".to_owned())?;
                if !(1..=60).contains(&parsed) {
                    return Err("--scan-timeout must be an integer from 1 to 60".to_owned());
                }
                options.scan_timeout_secs = parsed;
                index += 1;
            }
            "-p" | "--port" => {
                options.port = value(&args, index, argument)?
                    .parse::<u16>()
                    .map_err(|_| "--port must be an integer from 0 to 65535".to_owned())?;
                index += 1;
            }
            _ if argument.starts_with('-') => {
                return Err(format!("Unknown option: {argument}"));
            }
            _ => {
                if options.file.is_some() {
                    return Err("Only one movie file can be streamed at a time".to_owned());
                }
                options.file = Some(PathBuf::from(&args[index]));
            }
        }

        index += 1;
    }

    Ok(options)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn parses_movie_and_defaults() {
        let result = parse(strings(&["movie.mp4"])).unwrap();
        assert_eq!(result.file, Some(PathBuf::from("movie.mp4")));
        assert_eq!(result.scan_timeout_secs, 5);
        assert_eq!(result.port, 0);
        assert!(result.subtitles.is_empty());
    }

    #[test]
    fn parses_repeatable_subtitles_and_options() {
        let result = parse(strings(&[
            "movie.mp4",
            "-s",
            "en.srt",
            "--subtitle",
            "pl.vtt",
            "--device",
            "Living Room",
            "--scan-timeout",
            "8",
            "--port",
            "9123",
        ]))
        .unwrap();

        assert_eq!(
            result.subtitles,
            vec![PathBuf::from("en.srt"), PathBuf::from("pl.vtt")]
        );
        assert_eq!(result.device.as_deref(), Some("Living Room"));
        assert_eq!(result.scan_timeout_secs, 8);
        assert_eq!(result.port, 9123);
    }

    #[test]
    fn rejects_invalid_options_and_extra_movies() {
        assert!(parse(strings(&["movie.mp4", "--wat"])).is_err());
        assert!(parse(strings(&["one.mp4", "two.mp4"])).is_err());
        assert!(parse(strings(&["movie.mp4", "--scan-timeout", "0"])).is_err());
        assert!(parse(strings(&["movie.mp4", "--port", "70000"])).is_err());
    }
}
