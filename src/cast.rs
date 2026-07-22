use async_channel::{Receiver as SignalReceiver, Sender as SubtitleSender};
use cast_sender::namespace::media::{
    CaptionMimeType, EditTracksInfoRequestData, GetStatusRequestData, IdleReason, LoadRequestData,
    Media, MediaInformation, MovieMediaMetadata, PlayerState, StreamType, TextTrackType, Track,
    TrackType,
};
use cast_sender::{AppId, MediaController, Payload, Receiver};
use dialoguer::{Select, theme::ColorfulTheme};
use std::future::pending;
use std::thread;

use crate::DynError;

#[derive(Clone, Debug)]
pub struct CastSubtitle {
    pub url: String,
    pub name: String,
    pub language: Option<String>,
}

pub enum CastOutcome {
    Finished,
    Stopped(String),
    Interrupted,
}

fn media_request(
    media_url: String,
    content_type: String,
    title: String,
    subtitles: Vec<CastSubtitle>,
) -> LoadRequestData {
    let tracks: Vec<_> = subtitles
        .into_iter()
        .enumerate()
        .map(|(index, subtitle)| Track {
            track_id: (index + 1) as i32,
            type_: TrackType::Text,
            subtype: Some(TextTrackType::Subtitles),
            track_content_id: Some(subtitle.url),
            track_content_type: Some(CaptionMimeType::Other("text/vtt".to_owned())),
            name: Some(subtitle.name),
            language: subtitle.language,
            ..Default::default()
        })
        .collect();
    let has_subtitles = !tracks.is_empty();
    let media = MediaInformation {
        content_id: media_url,
        content_type,
        stream_type: StreamType::Buffered,
        metadata: Some(
            MovieMediaMetadata {
                title: Some(title),
                ..Default::default()
            }
            .into(),
        ),
        tracks: has_subtitles.then_some(tracks),
        ..Default::default()
    };

    LoadRequestData {
        active_track_ids: has_subtitles.then_some(Vec::new()),
        autoplay: Some(true),
        media,
        ..Default::default()
    }
}

fn subtitle_menu_labels(subtitles: &[CastSubtitle], active: Option<usize>) -> Vec<String> {
    std::iter::once(("None", active.is_none()))
        .chain(
            subtitles
                .iter()
                .enumerate()
                .map(|(index, subtitle)| (subtitle.name.as_str(), active == Some(index))),
        )
        .map(|(name, enabled)| format!("{} {name}", if enabled { "●" } else { "○" }))
        .collect()
}

fn spawn_subtitle_menu(
    subtitles: Vec<CastSubtitle>,
    sender: SubtitleSender<Option<usize>>,
) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        let mut active = None;
        loop {
            let labels = subtitle_menu_labels(&subtitles, active);
            let selection = Select::with_theme(&ColorfulTheme::default())
                .with_prompt("Subtitles — ↑/↓ to move, Enter to switch, q to close")
                .items(&labels)
                .default(active.map_or(0, |index| index + 1))
                .report(false)
                .clear(true)
                .interact_opt();
            let Ok(Some(selection)) = selection else {
                break;
            };
            active = selection.checked_sub(1);
            if sender.send_blocking(active).is_err() {
                break;
            }
        }
    })
}

fn edit_tracks_request(media_session_id: i32, active: Option<usize>) -> EditTracksInfoRequestData {
    EditTracksInfoRequestData {
        active_track_ids: Some(active.map_or_else(Vec::new, |index| vec![(index + 1) as i32])),
        enable_text_tracks: Some(active.is_some()),
        media_session_id: Some(media_session_id),
        ..Default::default()
    }
}

fn session_id(payload: &Payload) -> Option<i32> {
    let Payload::Media(Media::MediaStatus(statuses)) = payload else {
        return None;
    };
    statuses
        .status
        .first()
        .map(|status| status.media_session_id)
}

pub async fn run(
    receiver_address: &str,
    receiver_name: &str,
    media_url: String,
    content_type: String,
    title: String,
    subtitles: Vec<CastSubtitle>,
    signal: SignalReceiver<()>,
) -> Result<CastOutcome, DynError> {
    let receiver = Receiver::new();
    receiver.connect(receiver_address).await?;
    let app = receiver.launch_app(AppId::DefaultMediaReceiver).await?;
    let controller = MediaController::new(app.clone(), receiver.clone())?;
    let subtitle_count = subtitles.len();
    controller
        .load(media_request(
            media_url,
            content_type,
            title.clone(),
            subtitles.clone(),
        ))
        .await?;
    let status = receiver
        .send_request(&app, Media::GetStatus(GetStatusRequestData::default()))
        .await?;
    let media_session_id = session_id(&status.payload)
        .ok_or_else(|| std::io::Error::other("Cast receiver returned no media session"))?;

    println!("Now casting {title} to {receiver_name}.");
    if subtitle_count > 0 {
        println!(
            "Subtitles: {subtitle_count} track{} available; none enabled.",
            if subtitle_count == 1 { "" } else { "s" }
        );
    }
    if !subtitles.is_empty() {
        println!("Choose a subtitle below; q closes the subtitle menu.");
    }
    println!("Press Ctrl+C to stop.");

    let (subtitle_tx, subtitle_rx) = async_channel::unbounded();
    let _subtitle_menu =
        (!subtitles.is_empty()).then(|| spawn_subtitle_menu(subtitles.clone(), subtitle_tx));

    enum Event {
        Signal,
        Subtitle(Option<usize>),
        Response(Box<Result<cast_sender::Response, cast_sender::Error>>),
    }

    loop {
        let playback_event = smol::future::race(
            async {
                if subtitles.is_empty() {
                    pending::<Event>().await
                } else {
                    match subtitle_rx.recv().await {
                        Ok(selection) => Event::Subtitle(selection),
                        Err(_) => pending::<Event>().await,
                    }
                }
            },
            async { Event::Response(Box::new(receiver.receive().await)) },
        );
        let event = smol::future::race(
            async {
                let _ = signal.recv().await;
                Event::Signal
            },
            playback_event,
        )
        .await;

        match event {
            Event::Signal => {
                let _ = controller.stop().await;
                receiver.disconnect().await;
                return Ok(CastOutcome::Interrupted);
            }
            Event::Subtitle(active) => {
                let response = receiver
                    .send_request(
                        &app,
                        Media::EditTracksInfo(edit_tracks_request(media_session_id, active)),
                    )
                    .await?;
                if session_id(&response.payload).is_none() {
                    return Err(std::io::Error::other(
                        "Cast receiver rejected the subtitle change",
                    )
                    .into());
                }
            }
            Event::Response(response) if response.is_err() => {
                receiver.disconnect().await;
                return Err(response.unwrap_err().into());
            }
            Event::Response(response) => {
                let response = response.expect("error response handled above");
                let Payload::Media(Media::MediaStatus(statuses)) = response.payload else {
                    continue;
                };
                for status in statuses.status {
                    if !matches!(status.player_state, PlayerState::Idle) {
                        continue;
                    }
                    let Some(reason) = status.idle_reason else {
                        continue;
                    };
                    receiver.disconnect().await;
                    return match reason {
                        IdleReason::Finished => Ok(CastOutcome::Finished),
                        IdleReason::Error => Err(std::io::Error::other(
                            "the Cast receiver could not play this media",
                        )
                        .into()),
                        IdleReason::Cancelled => Ok(CastOutcome::Stopped("cancelled".to_owned())),
                        IdleReason::Interrupted => {
                            Ok(CastOutcome::Stopped("interrupted".to_owned()))
                        }
                    };
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn advertises_all_subtitles_and_starts_with_none_enabled() {
        let request = media_request(
            "http://127.0.0.1/movie".to_owned(),
            "video/mp4".to_owned(),
            "Movie".to_owned(),
            vec![
                CastSubtitle {
                    url: "http://127.0.0.1/en.vtt".to_owned(),
                    name: "English".to_owned(),
                    language: Some("en".to_owned()),
                },
                CastSubtitle {
                    url: "http://127.0.0.1/pl.vtt".to_owned(),
                    name: "Polish".to_owned(),
                    language: Some("pl".to_owned()),
                },
            ],
        );

        assert_eq!(request.active_track_ids, Some(Vec::new()));
        assert_eq!(request.media.tracks.as_ref().unwrap().len(), 2);
        let json = serde_json::to_value(request).unwrap();
        assert_eq!(json["media"]["tracks"][0]["trackContentType"], "text/vtt");
        assert_eq!(json["media"]["tracks"][0]["subtype"], "SUBTITLES");
    }

    #[test]
    fn subtitle_menu_marks_the_active_track_and_supports_none() {
        let subtitles = vec![
            CastSubtitle {
                url: String::new(),
                name: "English".to_owned(),
                language: Some("en".to_owned()),
            },
            CastSubtitle {
                url: String::new(),
                name: "Polish".to_owned(),
                language: Some("pl".to_owned()),
            },
        ];

        assert_eq!(
            subtitle_menu_labels(&subtitles, Some(1)),
            ["○ None", "○ English", "● Polish"]
        );
        assert_eq!(
            subtitle_menu_labels(&subtitles, None),
            ["● None", "○ English", "○ Polish"]
        );
    }

    #[test]
    fn builds_subtitle_switch_requests() {
        let enabled = serde_json::to_value(edit_tracks_request(42, Some(1))).unwrap();
        assert_eq!(enabled["mediaSessionId"], 42);
        assert_eq!(enabled["activeTrackIds"], serde_json::json!([2]));
        assert_eq!(enabled["enableTextTracks"], true);

        let disabled = serde_json::to_value(edit_tracks_request(42, None)).unwrap();
        assert_eq!(disabled["activeTrackIds"], serde_json::json!([]));
        assert_eq!(disabled["enableTextTracks"], false);
    }
}
