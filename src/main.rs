mod args;
mod bundled_ffmpeg;
mod cast;
mod discovery;
mod range;
mod server;
mod subtitles;

use async_channel::bounded;
use dialoguer::{Select, theme::ColorfulTheme};
use std::env;
use std::error::Error;
use std::net::{IpAddr, UdpSocket};
use std::path::{Path, PathBuf};
use std::time::Duration;

use args::{HELP, VERSION};
use cast::{CastOutcome, CastSubtitle};
use discovery::CastDevice;
use server::MediaServer;

type DynError = Box<dyn Error + Send + Sync>;

fn choose_device(
    devices: Vec<CastDevice>,
    requested: Option<&str>,
) -> Result<CastDevice, DynError> {
    let mut choices = devices;
    if let Some(requested) = requested {
        let requested = requested.to_lowercase();
        choices.retain(|device| device.name.to_lowercase().contains(&requested));
        if choices.is_empty() {
            return Err(
                std::io::Error::other(format!("No Cast device matched \"{requested}\"")).into(),
            );
        }
        if choices.len() == 1 {
            return Ok(choices.remove(0));
        }
    }

    let labels: Vec<_> = choices
        .iter()
        .map(|device| match &device.model {
            Some(model) => format!("{} ({model})", device.name),
            None => device.name.clone(),
        })
        .collect();
    let selection = Select::with_theme(&ColorfulTheme::default())
        .with_prompt(if requested.is_some() {
            "Multiple devices matched"
        } else {
            "Choose a Cast device"
        })
        .items(&labels)
        .default(0)
        .interact()?;
    Ok(choices.remove(selection))
}

fn validate_movie(path: &Path) -> Result<PathBuf, DynError> {
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

fn run() -> Result<(), DynError> {
    let options = args::parse(env::args_os().skip(1)).map_err(std::io::Error::other)?;
    if options.help {
        println!("{HELP}");
        return Ok(());
    }
    if options.version {
        println!("{VERSION}");
        return Ok(());
    }
    let movie = options
        .file
        .as_deref()
        .ok_or_else(|| std::io::Error::other(format!("A movie file is required.\n\n{HELP}")))?;
    let movie = validate_movie(movie)?;

    println!(
        "Searching for Cast devices for {}s…",
        options.scan_timeout_secs
    );
    let devices = discovery::discover(Duration::from_secs(options.scan_timeout_secs))?;
    if devices.is_empty() {
        return Err(std::io::Error::other(
            "No Cast devices found. Check that the TV and this computer are on the same network.",
        )
        .into());
    }
    let device = choose_device(devices, options.device.as_deref())?;
    let local_address = local_address_for(device.address)?;

    let prepared = subtitles::prepare(&movie, &options.subtitles)?;
    for warning in &prepared.warnings {
        eprintln!("dropcast: {warning}");
    }
    let server = MediaServer::start(&movie, options.port, &prepared.tracks)?;
    let base_url = format!("http://{}:{}", url_host(local_address), server.port);
    let media_url = format!("{base_url}{}", server.media_path);
    let cast_subtitles: Vec<_> = server
        .subtitles
        .iter()
        .map(|subtitle| CastSubtitle {
            url: format!("{base_url}{}", subtitle.path),
            name: subtitle.name.clone(),
            language: subtitle.language.clone(),
        })
        .collect();

    let (signal_tx, signal_rx) = bounded(1);
    ctrlc::set_handler(move || {
        let _ = signal_tx.try_send(());
    })?;

    println!("Connecting to {}…", device.name);
    let title = movie
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Movie")
        .to_owned();
    let content_type = server::movie_content_type(&movie).to_owned();

    let outcome = smol::block_on(cast::run(
        &device.address.to_string(),
        &device.name,
        media_url,
        content_type,
        title,
        cast_subtitles,
        signal_rx,
    ))?;
    match outcome {
        CastOutcome::Finished => println!("Playback finished."),
        CastOutcome::Stopped(reason) => println!("Playback stopped ({reason})."),
        CastOutcome::Interrupted => println!("Playback stopped."),
    }
    drop(server);
    drop(prepared);
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("dropcast: {error}");
        std::process::exit(1);
    }
}
