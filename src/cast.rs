use async_channel::Receiver as SignalReceiver;
use cast_sender::namespace::media::{
    CaptionMimeType, IdleReason, LoadRequestData, Media, MediaInformation, MovieMediaMetadata,
    PlayerState, StreamType, TextTrackType, Track, TrackType,
};
use cast_sender::{AppId, MediaController, Payload, Receiver};

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
    let active_track_ids = (!tracks.is_empty()).then_some(vec![1]);
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
        tracks: (!tracks.is_empty()).then_some(tracks),
        ..Default::default()
    };

    LoadRequestData {
        active_track_ids,
        autoplay: Some(true),
        media,
        ..Default::default()
    }
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
    let controller = MediaController::new(app, receiver.clone())?;
    let subtitle_summary = subtitles
        .first()
        .map(|first| (subtitles.len(), first.name.clone()));
    controller
        .load(media_request(
            media_url,
            content_type,
            title.clone(),
            subtitles,
        ))
        .await?;

    println!("Now casting {title} to {receiver_name}.");
    if let Some((count, first)) = subtitle_summary {
        println!(
            "Subtitles: {count} track{}; {first} enabled.",
            if count == 1 { "" } else { "s" }
        );
    }
    println!("Press Ctrl+C to stop.");

    enum Event {
        Signal,
        Response(Box<Result<cast_sender::Response, cast_sender::Error>>),
    }

    loop {
        let event = smol::future::race(
            async {
                let _ = signal.recv().await;
                Event::Signal
            },
            async { Event::Response(Box::new(receiver.receive().await)) },
        )
        .await;

        match event {
            Event::Signal => {
                let _ = controller.stop().await;
                receiver.disconnect().await;
                return Ok(CastOutcome::Interrupted);
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
    fn advertises_all_subtitles_and_enables_the_first() {
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

        assert_eq!(request.active_track_ids, Some(vec![1]));
        assert_eq!(request.media.tracks.as_ref().unwrap().len(), 2);
        let json = serde_json::to_value(request).unwrap();
        assert_eq!(json["media"]["tracks"][0]["trackContentType"], "text/vtt");
        assert_eq!(json["media"]["tracks"][0]["subtype"], "SUBTITLES");
    }
}
