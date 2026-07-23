mod args;
use async_channel::bounded;
use dialoguer::{Select, theme::ColorfulTheme};
use dropcast::{CastDevice, CastIo, CastOutcome, DynError, cast_movie, discovery, validate_movie};
use std::env;
use std::time::Duration;

use args::{HELP, VERSION};

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

    let (signal_tx, signal_rx) = bounded(1);
    ctrlc::set_handler(move || {
        let _ = signal_tx.try_send(());
    })?;

    println!("Connecting to {}…", device.name);
    let outcome = cast_movie(
        &movie,
        &device,
        &options.subtitles,
        options.port,
        CastIo::terminal(signal_rx),
    )?;
    match outcome {
        CastOutcome::Finished => println!("Playback finished."),
        CastOutcome::Stopped(reason) => println!("Playback stopped ({reason})."),
        CastOutcome::Interrupted => println!("Playback stopped."),
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("dropcast: {error}");
        std::process::exit(1);
    }
}
