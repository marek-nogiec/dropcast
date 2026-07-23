#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("The Dropcast desktop app is currently supported on macOS only.");
}

#[cfg(target_os = "macos")]
mod macos {
    use async_channel::Sender as AsyncSender;
    use dropcast::{
        CastControl, CastDevice, CastEvent, CastIo, CastOutcome, PlaybackStatus, cast_movie,
        discovery, validate_movie,
    };
    use eframe::egui::{
        self, Align2, Color32, FontData, FontDefinitions, FontFamily, FontId, Pos2, Rect, Response,
        Sense, Shape, Stroke, StrokeKind, Vec2,
    };
    use std::net::{IpAddr, Ipv4Addr};
    use std::path::{Path, PathBuf};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::mpsc::{self, Receiver, Sender};
    use std::time::Duration;

    const WINDOW_WIDTH: f32 = 420.0;
    const WINDOW_HEIGHT: f32 = 596.0;

    const ACCENT: Color32 = Color32::from_rgb(244, 181, 105);
    const ACCENT_2: Color32 = Color32::from_rgb(235, 119, 82);
    const ACCENT_INK: Color32 = Color32::from_rgb(62, 40, 26);
    const RED: Color32 = Color32::from_rgb(255, 107, 107);

    #[derive(Clone, Copy)]
    struct Palette {
        bg: Color32,
        surface: Color32,
        surface_2: Color32,
        line: Color32,
        ink: Color32,
        ink_2: Color32,
        ink_3: Color32,
        movie_stripe: Color32,
    }

    const DARK_PALETTE: Palette = Palette {
        bg: Color32::from_rgb(31, 29, 27),
        surface: Color32::from_rgb(46, 43, 40),
        surface_2: Color32::from_rgb(61, 57, 53),
        line: Color32::from_rgba_premultiplied(23, 23, 23, 23),
        ink: Color32::from_rgb(246, 242, 236),
        ink_2: Color32::from_rgb(188, 179, 168),
        ink_3: Color32::from_rgb(137, 128, 118),
        movie_stripe: Color32::from_rgb(67, 62, 57),
    };

    const LIGHT_PALETTE: Palette = Palette {
        bg: Color32::from_rgb(246, 244, 240),
        surface: Color32::from_rgb(253, 251, 248),
        surface_2: Color32::from_rgb(233, 230, 225),
        line: Color32::from_rgba_premultiplied(0, 0, 0, 26),
        ink: Color32::from_rgb(62, 57, 52),
        ink_2: Color32::from_rgb(105, 98, 91),
        ink_3: Color32::from_rgb(145, 137, 128),
        movie_stripe: Color32::from_rgb(218, 213, 207),
    };

    static DARK_MODE: AtomicBool = AtomicBool::new(true);

    fn palette_for(dark_mode: bool) -> Palette {
        if dark_mode {
            DARK_PALETTE
        } else {
            LIGHT_PALETTE
        }
    }

    fn palette() -> Palette {
        palette_for(DARK_MODE.load(Ordering::Relaxed))
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Screen {
        Drop,
        Scanning,
        Devices,
        Connecting,
        Playing,
        NoDevices,
        PlaybackFailed,
    }

    enum WorkerEvent {
        Devices {
            movie: PathBuf,
            devices: Vec<CastDevice>,
        },
        DiscoveryFailed(String),
        CastFinished(Result<CastOutcome, String>),
    }

    struct DropcastApp {
        screen: Screen,
        movie: Option<PathBuf>,
        devices: Vec<CastDevice>,
        selected_device: Option<usize>,
        target_device: Option<CastDevice>,
        playback: PlaybackStatus,
        subtitles: Vec<String>,
        subtitle_sheet: bool,
        discovery_error: Option<String>,
        playback_error: Option<String>,
        worker_tx: Sender<WorkerEvent>,
        worker_rx: Receiver<WorkerEvent>,
        cast_event_rx: Option<Receiver<CastEvent>>,
        signal_tx: Option<AsyncSender<()>>,
        control_tx: Option<AsyncSender<CastControl>>,
        preview: bool,
    }

    impl DropcastApp {
        fn new(
            context: &egui::Context,
            preview: Option<Screen>,
            forced_theme: Option<egui::Theme>,
        ) -> Self {
            configure_context(context, forced_theme);
            let (worker_tx, worker_rx) = mpsc::channel();
            let mut app = Self {
                screen: preview.unwrap_or(Screen::Drop),
                movie: None,
                devices: Vec::new(),
                selected_device: None,
                target_device: None,
                playback: preview_playback(),
                subtitles: Vec::new(),
                subtitle_sheet: false,
                discovery_error: None,
                playback_error: None,
                worker_tx,
                worker_rx,
                cast_event_rx: None,
                signal_tx: None,
                control_tx: None,
                preview: preview.is_some(),
            };
            if let Some(screen) = preview {
                app.load_preview(screen);
            }
            app
        }

        fn load_preview(&mut self, screen: Screen) {
            self.screen = screen;
            self.movie = Some(PathBuf::from("Dune.Part.Two.2024.2160p.mkv"));
            self.devices = preview_devices();
            self.selected_device = (screen != Screen::Devices).then_some(0);
            self.target_device = Some(self.devices[0].clone());
            self.subtitles = vec![
                "English · movie.en.srt".to_owned(),
                "Polish · movie.pl.vtt".to_owned(),
                "English (SDH) · embedded".to_owned(),
                "Spanish · embedded".to_owned(),
            ];
            self.discovery_error = Some(
                "Make sure your TV is on and connected to the same Wi‑Fi network as this Mac."
                    .to_owned(),
            );
            self.playback_error = Some(
                "Living Room TV couldn’t play this file. It may use a codec your TV doesn’t support."
                    .to_owned(),
            );
        }

        fn begin_drop(&mut self, movie: PathBuf, context: egui::Context) {
            if !matches!(
                self.screen,
                Screen::Drop | Screen::NoDevices | Screen::PlaybackFailed | Screen::Devices
            ) {
                return;
            }
            self.movie = Some(movie.clone());
            self.selected_device = None;
            self.discovery_error = None;
            self.screen = Screen::Scanning;
            self.spawn_discovery(movie, context);
        }

        fn spawn_discovery(&self, movie: PathBuf, context: egui::Context) {
            if self.preview {
                return;
            }
            let events = self.worker_tx.clone();
            std::thread::spawn(move || {
                let result = validate_movie(&movie).and_then(|movie| {
                    discovery::discover(Duration::from_secs(5)).map(|devices| (movie, devices))
                });
                let event = match result {
                    Ok((_movie, devices)) if devices.is_empty() => {
                        WorkerEvent::DiscoveryFailed(
                            "Make sure your TV is on and connected to the same Wi‑Fi network as this Mac. Guest networks can block casting."
                                .to_owned(),
                        )
                    }
                    Ok((movie, devices)) => WorkerEvent::Devices { movie, devices },
                    Err(error) => WorkerEvent::DiscoveryFailed(error.to_string()),
                };
                let _ = events.send(event);
                context.request_repaint();
            });
        }

        fn rescan(&mut self, context: egui::Context) {
            let Some(movie) = self.movie.clone() else {
                self.screen = Screen::Drop;
                return;
            };
            self.selected_device = None;
            self.discovery_error = None;
            self.screen = Screen::Scanning;
            if self.preview {
                self.screen = Screen::Devices;
            } else {
                self.spawn_discovery(movie, context);
            }
        }

        fn begin_cast(&mut self, context: egui::Context) {
            let (Some(movie), Some(index)) = (self.movie.clone(), self.selected_device) else {
                return;
            };
            let Some(device) = self.devices.get(index).cloned() else {
                return;
            };
            self.target_device = Some(device.clone());
            self.playback_error = None;
            self.screen = Screen::Connecting;
            if self.preview {
                return;
            }

            let (signal_tx, signal_rx) = async_channel::bounded(1);
            let (control_tx, control_rx) = async_channel::unbounded();
            let (cast_event_tx, cast_event_rx) = mpsc::channel();
            self.signal_tx = Some(signal_tx);
            self.control_tx = Some(control_tx);
            self.cast_event_rx = Some(cast_event_rx);

            let worker_events = self.worker_tx.clone();
            std::thread::spawn(move || {
                let io = CastIo {
                    signal: signal_rx,
                    controls: Some(control_rx),
                    events: Some(cast_event_tx),
                    terminal_ui: false,
                };
                let result =
                    cast_movie(&movie, &device, &[], 0, io).map_err(|error| error.to_string());
                let _ = worker_events.send(WorkerEvent::CastFinished(result));
                context.request_repaint();
            });
        }

        fn handle_events(&mut self) {
            while let Ok(event) = self.worker_rx.try_recv() {
                match event {
                    WorkerEvent::Devices { movie, devices } if self.screen == Screen::Scanning => {
                        self.movie = Some(movie);
                        self.devices = devices;
                        self.selected_device = None;
                        self.screen = Screen::Devices;
                    }
                    WorkerEvent::DiscoveryFailed(error) if self.screen == Screen::Scanning => {
                        self.discovery_error = Some(error);
                        self.screen = Screen::NoDevices;
                    }
                    WorkerEvent::CastFinished(Ok(_))
                        if matches!(self.screen, Screen::Playing | Screen::Connecting) =>
                    {
                        self.clear_session();
                        self.screen = Screen::Devices;
                    }
                    WorkerEvent::CastFinished(Err(error))
                        if matches!(self.screen, Screen::Playing | Screen::Connecting) =>
                    {
                        self.playback_error = Some(error);
                        self.clear_session();
                        self.screen = Screen::PlaybackFailed;
                    }
                    _ => {}
                }
            }

            let mut cast_events = Vec::new();
            if let Some(receiver) = &self.cast_event_rx {
                while let Ok(event) = receiver.try_recv() {
                    cast_events.push(event);
                }
            }
            for event in cast_events {
                match event {
                    CastEvent::Connected { subtitles } => {
                        self.subtitles = subtitles;
                        self.screen = Screen::Playing;
                    }
                    CastEvent::Status(status) => self.playback = status,
                }
            }
        }

        fn clear_session(&mut self) {
            self.signal_tx = None;
            self.control_tx = None;
            self.cast_event_rx = None;
            self.subtitle_sheet = false;
        }

        fn cancel_session(&mut self) {
            if let Some(signal) = &self.signal_tx {
                let _ = signal.try_send(());
            }
            self.clear_session();
            self.screen = Screen::Devices;
        }

        fn send_control(&self, control: CastControl) {
            if let Some(sender) = &self.control_tx {
                let _ = sender.try_send(control);
            }
        }

        fn reset_to_drop(&mut self) {
            if self.signal_tx.is_some() {
                self.cancel_session();
            }
            self.screen = Screen::Drop;
            self.movie = None;
            self.devices.clear();
            self.selected_device = None;
            self.target_device = None;
            self.subtitle_sheet = false;
        }

        fn file_name(&self) -> String {
            self.movie
                .as_deref()
                .map(file_name)
                .unwrap_or_else(|| "Dune.Part.Two.2024.2160p.mkv".to_owned())
        }

        fn target_name(&self) -> &str {
            self.target_device
                .as_ref()
                .map_or("Living Room TV", |device| device.name.as_str())
        }

        fn draw(&mut self, ui: &mut egui::Ui) {
            DARK_MODE.store(ui.visuals().dark_mode, Ordering::Relaxed);
            let content = ui.max_rect();
            ui.painter().rect_filled(content, 0.0, palette().bg);
            let time = ui.input(|input| input.time) as f32;

            match self.screen {
                Screen::Drop => self.draw_drop(ui, content),
                Screen::Scanning => self.draw_scanning(ui, content, time),
                Screen::Devices => self.draw_devices(ui, content),
                Screen::Connecting => self.draw_connecting(ui, content, time),
                Screen::Playing => self.draw_playing(ui, content, time),
                Screen::NoDevices => self.draw_no_devices(ui, content),
                Screen::PlaybackFailed => self.draw_playback_failed(ui, content),
            }
        }

        fn draw_drop(&mut self, ui: &mut egui::Ui, content: Rect) {
            let zone = content.shrink(18.0);
            let hovering = ui.input(|input| !input.raw.hovered_files.is_empty());
            if hovering {
                ui.painter().rect_filled(
                    zone,
                    18.0,
                    Color32::from_rgba_unmultiplied(244, 181, 105, 20),
                );
            }
            paint_dashed_rect(
                ui.painter(),
                zone,
                if hovering { ACCENT } else { palette().line },
            );
            let response = ui.allocate_rect(zone, Sense::click());

            let logo = Rect::from_center_size(
                Pos2::new(zone.center().x, zone.center().y - 80.0),
                Vec2::splat(86.0),
            );
            paint_logo(ui.painter(), logo);
            ui.painter().text(
                Pos2::new(zone.center().x, zone.center().y + 6.0),
                Align2::CENTER_CENTER,
                if hovering {
                    "Drop it — ready to cast"
                } else {
                    "Drop a movie to cast"
                },
                FontId::proportional(18.0),
                palette().ink,
            );
            ui.painter().text(
                Pos2::new(zone.center().x, zone.center().y + 36.0),
                Align2::CENTER_CENTER,
                "MP4, MKV or MOV — streamed straight",
                FontId::proportional(13.0),
                palette().ink_3,
            );
            ui.painter().text(
                Pos2::new(zone.center().x, zone.center().y + 56.0),
                Align2::CENTER_CENTER,
                "to your TV, no upload.",
                FontId::proportional(13.0),
                palette().ink_3,
            );

            let browse = Rect::from_center_size(
                Pos2::new(zone.center().x, zone.center().y + 103.0),
                Vec2::new(122.0, 35.0),
            );
            let browse_response = paint_button(
                ui,
                browse,
                "Browse files…",
                palette().surface,
                ACCENT,
                999.0,
                Some(ACCENT),
            );
            if (response.clicked() || browse_response.clicked())
                && let Some(path) = rfd::FileDialog::new()
                    .add_filter("Movies", &["mp4", "mkv", "mov", "m4v", "webm", "avi"])
                    .pick_file()
            {
                self.begin_drop(path, ui.ctx().clone());
            }
        }

        fn draw_scanning(&mut self, ui: &mut egui::Ui, content: Rect, time: f32) {
            let center = Pos2::new(content.center().x, content.center().y - 24.0);
            paint_ripples(ui.painter(), center, time, 2.4);
            paint_tv(ui.painter(), center, Vec2::new(66.0, 48.0), ACCENT);
            ui.painter().text(
                Pos2::new(center.x, center.y + 112.0),
                Align2::CENTER_CENTER,
                "Looking for devices…",
                FontId::proportional(17.0),
                palette().ink,
            );
            ui.painter().text(
                Pos2::new(center.x, center.y + 139.0),
                Align2::CENTER_CENTER,
                "Scanning your Wi‑Fi for Cast‑enabled TVs.",
                FontId::proportional(13.0),
                palette().ink_3,
            );
            let cancel = local_rect(
                content,
                18.0,
                content.height() - 62.0,
                content.width() - 36.0,
                44.0,
            );
            if paint_outline_button(ui, cancel, "Cancel").clicked() {
                self.reset_to_drop();
            }
        }

        fn draw_devices(&mut self, ui: &mut egui::Ui, content: Rect) {
            let left = content.left() + 18.0;
            let right = content.right() - 18.0;
            ui.painter().text(
                Pos2::new(left, content.top() + 28.0),
                Align2::LEFT_CENTER,
                "Cast to…",
                FontId::proportional(16.0),
                palette().ink,
            );
            ui.painter().text(
                Pos2::new(right, content.top() + 28.0),
                Align2::RIGHT_CENTER,
                format!("{} found", self.devices.len()),
                FontId::proportional(12.0),
                palette().ink_3,
            );

            let mut clicked = None;
            for (index, device) in self.devices.iter().take(4).enumerate() {
                let row = local_rect(
                    content,
                    18.0,
                    50.0 + index as f32 * 82.0,
                    content.width() - 36.0,
                    70.0,
                );
                let selected = self.selected_device == Some(index);
                let response = ui.allocate_rect(row, Sense::click());
                let fill = if response.hovered() {
                    palette().surface_2
                } else {
                    palette().surface
                };
                ui.painter().rect_filled(row, 13.0, fill);
                ui.painter().rect_stroke(
                    row,
                    13.0,
                    Stroke::new(1.5, if selected { ACCENT } else { palette().line }),
                    StrokeKind::Inside,
                );
                let tv_center = Pos2::new(row.left() + 34.0, row.center().y - 2.0);
                paint_tv(
                    ui.painter(),
                    tv_center,
                    Vec2::new(44.0, 32.0),
                    palette().ink_3,
                );
                ui.painter().text(
                    Pos2::new(row.left() + 68.0, row.center().y - 10.0),
                    Align2::LEFT_CENTER,
                    &device.name,
                    FontId::proportional(14.0),
                    palette().ink,
                );
                let detail = match &device.model {
                    Some(model) => format!("{model} · {}", device.address),
                    None => device.address.to_string(),
                };
                ui.painter().text(
                    Pos2::new(row.left() + 68.0, row.center().y + 12.0),
                    Align2::LEFT_CENTER,
                    detail,
                    FontId::proportional(11.5),
                    palette().ink_3,
                );
                if selected {
                    paint_check(ui.painter(), Pos2::new(row.right() - 23.0, row.center().y));
                }
                if response.clicked() {
                    clicked = Some(index);
                }
            }
            if let Some(index) = clicked {
                self.selected_device = Some(index);
            }

            let buttons_y = content.height() - 62.0;
            let rescan = local_rect(content, 18.0, buttons_y, 46.0, 44.0);
            let cast = local_rect(content, 74.0, buttons_y, content.width() - 92.0, 44.0);
            if paint_icon_button(ui, rescan, Icon::Rescan).clicked() {
                self.rescan(ui.ctx().clone());
            }
            let label = self
                .selected_device
                .and_then(|index| self.devices.get(index))
                .map_or_else(
                    || "Select a device".to_owned(),
                    |device| format!("Cast to {}", device.name),
                );
            let enabled = self.selected_device.is_some();
            let response = paint_primary_button(ui, cast, &label, enabled);
            if enabled && response.clicked() {
                self.begin_cast(ui.ctx().clone());
            }
        }

        fn draw_connecting(&mut self, ui: &mut egui::Ui, content: Rect, time: f32) {
            let center = Pos2::new(content.center().x, content.center().y - 25.0);
            paint_ripples(ui.painter(), center, time * 1.25, 1.8);
            paint_connecting_tv(ui.painter(), center, time);
            ui.painter().text(
                Pos2::new(center.x, center.y + 112.0),
                Align2::CENTER_CENTER,
                format!("Connecting to {}…", self.target_name()),
                FontId::proportional(17.0),
                palette().ink,
            );
            ui.painter().text(
                Pos2::new(center.x, center.y + 139.0),
                Align2::CENTER_CENTER,
                format!("Handing off {}", truncate(&self.file_name(), 26)),
                FontId::proportional(13.0),
                palette().ink_3,
            );
            let cancel = local_rect(
                content,
                18.0,
                content.height() - 62.0,
                content.width() - 36.0,
                44.0,
            );
            if paint_outline_button(ui, cancel, "Cancel").clicked() {
                self.cancel_session();
            }
        }

        fn draw_playing(&mut self, ui: &mut egui::Ui, content: Rect, time: f32) {
            let still = local_rect(content, 18.0, 18.0, content.width() - 36.0, 216.0);
            paint_movie_still(ui.painter(), still);
            paint_casting_badge(ui.painter(), still, self.target_name(), time);
            ui.painter().text(
                Pos2::new(still.left() + 13.0, still.bottom() - 34.0),
                Align2::LEFT_CENTER,
                movie_title(&self.file_name()),
                FontId::proportional(15.0),
                Color32::WHITE,
            );
            ui.painter().text(
                Pos2::new(still.left() + 13.0, still.bottom() - 15.0),
                Align2::LEFT_CENTER,
                movie_meta(&self.file_name(), self.playback.duration),
                FontId::proportional(11.5),
                Color32::from_rgba_unmultiplied(255, 255, 255, 180),
            );

            let progress = self
                .playback
                .duration
                .filter(|duration| *duration > 0.0)
                .map_or(0.0, |duration| {
                    (self.playback.current_time / duration).clamp(0.0, 1.0) as f32
                });
            let scrub = local_rect(content, 18.0, 250.0, content.width() - 36.0, 16.0);
            let response = paint_slider(
                ui,
                scrub,
                progress,
                (ACCENT_2, ACCENT),
                5.0,
                6.5,
                Color32::WHITE,
            );
            if (response.clicked() || response.dragged())
                && let (Some(pointer), Some(duration)) =
                    (response.interact_pointer_pos(), self.playback.duration)
            {
                let target = media_time_at_pointer(pointer.x, scrub, duration);
                self.playback.current_time = target;
                self.send_control(CastControl::SeekTo(target));
            }
            ui.painter().text(
                Pos2::new(scrub.left(), scrub.bottom() + 7.0),
                Align2::LEFT_TOP,
                format_time(self.playback.current_time),
                FontId::proportional(11.0),
                palette().ink_3,
            );
            ui.painter().text(
                Pos2::new(scrub.right(), scrub.bottom() + 7.0),
                Align2::RIGHT_TOP,
                format!(
                    "-{}",
                    format_time(
                        self.playback
                            .duration
                            .map_or(0.0, |duration| duration - self.playback.current_time)
                    )
                ),
                FontId::proportional(11.0),
                palette().ink_3,
            );

            let controls_y = content.top() + 332.0;
            let back = Rect::from_center_size(
                Pos2::new(content.center().x - 73.0, controls_y),
                Vec2::splat(33.0),
            );
            let play = Rect::from_center_size(
                Pos2::new(content.center().x, controls_y),
                Vec2::splat(60.0),
            );
            let forward = Rect::from_center_size(
                Pos2::new(content.center().x + 73.0, controls_y),
                Vec2::splat(33.0),
            );
            if paint_skip_button(ui, back, -10).clicked() {
                let target = (self.playback.current_time - 10.0).max(0.0);
                self.playback.current_time = target;
                self.send_control(CastControl::SeekTo(target));
            }
            if paint_play_button(ui, play, self.playback.is_playing).clicked() {
                self.playback.is_playing = !self.playback.is_playing;
                self.send_control(if self.playback.is_playing {
                    CastControl::Play
                } else {
                    CastControl::Pause
                });
            }
            if paint_skip_button(ui, forward, 10).clicked() {
                let target = self
                    .playback
                    .duration
                    .map_or(self.playback.current_time + 10.0, |duration| {
                        (self.playback.current_time + 10.0).min(duration)
                    });
                self.playback.current_time = target;
                self.send_control(CastControl::SeekTo(target));
            }

            let divider_y = content.bottom() - 64.0;
            ui.painter().line_segment(
                [
                    Pos2::new(content.left() + 18.0, divider_y),
                    Pos2::new(content.right() - 18.0, divider_y),
                ],
                Stroke::new(1.0, palette().line),
            );
            let mute = Rect::from_min_size(
                Pos2::new(content.left() + 18.0, divider_y + 12.0),
                Vec2::new(34.0, 34.0),
            );
            if paint_icon_button(
                ui,
                mute,
                if self.playback.muted {
                    Icon::Muted
                } else {
                    Icon::Volume
                },
            )
            .clicked()
            {
                self.playback.muted = !self.playback.muted;
                self.send_control(CastControl::SetVolume {
                    level: self.playback.volume,
                    muted: self.playback.muted,
                });
            }
            let volume = Rect::from_min_size(
                Pos2::new(mute.right() + 14.0, divider_y + 21.0),
                Vec2::new(234.0, 16.0),
            );
            let volume_response = paint_slider(
                ui,
                volume,
                if self.playback.muted {
                    0.0
                } else {
                    self.playback.volume as f32
                },
                (palette().ink_2, palette().ink_2),
                4.0,
                5.5,
                palette().ink,
            );
            if (volume_response.clicked() || volume_response.dragged())
                && let Some(pointer) = volume_response.interact_pointer_pos()
            {
                let level = ((pointer.x - volume.left()) / volume.width()).clamp(0.0, 1.0);
                self.playback.volume = f64::from(level);
                self.playback.muted = false;
                self.send_control(CastControl::SetVolume {
                    level: self.playback.volume,
                    muted: false,
                });
            }
            let cc = Rect::from_min_size(
                Pos2::new(volume.right() + 14.0, divider_y + 12.0),
                Vec2::new(40.0, 34.0),
            );
            if paint_cc_button(ui, cc, self.playback.active_subtitle.is_some()).clicked() {
                self.subtitle_sheet = true;
            }
            let stop = Rect::from_min_size(
                Pos2::new(cc.right() + 14.0, divider_y + 12.0),
                Vec2::new(34.0, 34.0),
            );
            if paint_icon_button(ui, stop, Icon::Stop).clicked() {
                self.cancel_session();
            }

            if self.subtitle_sheet {
                self.draw_subtitle_sheet(ui, content);
            }
        }

        fn draw_subtitle_sheet(&mut self, ui: &mut egui::Ui, content: Rect) {
            let overlay = ui.allocate_rect(content, Sense::click());
            ui.painter()
                .rect_filled(content, 0.0, Color32::from_rgba_unmultiplied(0, 0, 0, 118));
            let sheet_height = 286.0_f32.min(content.height() - 30.0);
            let sheet = Rect::from_min_max(
                Pos2::new(content.left(), content.bottom() - sheet_height),
                content.right_bottom(),
            );
            ui.painter().rect_filled(sheet, 16.0, palette().surface);
            ui.painter().line_segment(
                [sheet.left_top(), sheet.right_top()],
                Stroke::new(1.0, palette().line),
            );
            let handle = Rect::from_center_size(
                Pos2::new(sheet.center().x, sheet.top() + 15.0),
                Vec2::new(36.0, 4.0),
            );
            ui.painter().rect_filled(handle, 3.0, palette().line);
            ui.painter().text(
                Pos2::new(sheet.left() + 16.0, sheet.top() + 43.0),
                Align2::LEFT_CENTER,
                "Subtitles",
                FontId::proportional(14.0),
                palette().ink,
            );

            let mut selected = None;
            let labels = std::iter::once("Off".to_owned())
                .chain(self.subtitles.iter().cloned())
                .collect::<Vec<_>>();
            for (index, label) in labels.iter().take(5).enumerate() {
                let row = Rect::from_min_size(
                    Pos2::new(
                        sheet.left() + 16.0,
                        sheet.top() + 61.0 + index as f32 * 42.0,
                    ),
                    Vec2::new(sheet.width() - 32.0, 38.0),
                );
                let active = if index == 0 {
                    self.playback.active_subtitle.is_none()
                } else {
                    self.playback.active_subtitle == Some(index - 1)
                };
                let response = ui.allocate_rect(row, Sense::click());
                if active || response.hovered() {
                    ui.painter().rect_filled(row, 10.0, palette().surface_2);
                }
                ui.painter().text(
                    Pos2::new(row.left() + 12.0, row.center().y),
                    Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(14.0),
                    palette().ink,
                );
                if active {
                    paint_small_check(ui.painter(), Pos2::new(row.right() - 17.0, row.center().y));
                }
                if response.clicked() {
                    selected = Some(index.checked_sub(1));
                }
            }
            if let Some(active) = selected {
                self.playback.active_subtitle = active;
                self.send_control(CastControl::SelectSubtitle(active));
                self.subtitle_sheet = false;
            } else if overlay.clicked()
                && overlay
                    .interact_pointer_pos()
                    .is_some_and(|pointer| !sheet.contains(pointer))
            {
                self.subtitle_sheet = false;
            }
        }

        fn draw_no_devices(&mut self, ui: &mut egui::Ui, content: Rect) {
            let center = Pos2::new(content.center().x, content.center().y - 58.0);
            paint_error_tile(ui.painter(), center, false);
            ui.painter().text(
                Pos2::new(center.x, center.y + 75.0),
                Align2::CENTER_CENTER,
                "No devices found",
                FontId::proportional(17.0),
                palette().ink,
            );
            paint_multiline_center(
                ui.painter(),
                Pos2::new(center.x, center.y + 108.0),
                &[
                    "Make sure your TV is on and connected to the",
                    "same Wi‑Fi network as this Mac. Guest networks",
                    "can block casting.",
                ],
                13.0,
                palette().ink_3,
                20.0,
            );
            let primary = local_rect(
                content,
                18.0,
                content.height() - 115.0,
                content.width() - 36.0,
                44.0,
            );
            let secondary = local_rect(
                content,
                18.0,
                content.height() - 62.0,
                content.width() - 36.0,
                44.0,
            );
            if paint_primary_button(ui, primary, "Scan again", true).clicked() {
                self.rescan(ui.ctx().clone());
            }
            if paint_outline_button(ui, secondary, "Choose a different file").clicked() {
                self.reset_to_drop();
            }
        }

        fn draw_playback_failed(&mut self, ui: &mut egui::Ui, content: Rect) {
            let center = Pos2::new(content.center().x, content.center().y - 58.0);
            paint_error_tile(ui.painter(), center, true);
            ui.painter().text(
                Pos2::new(center.x, center.y + 75.0),
                Align2::CENTER_CENTER,
                "Playback stopped",
                FontId::proportional(17.0),
                palette().ink,
            );
            paint_multiline_center(
                ui.painter(),
                Pos2::new(center.x, center.y + 108.0),
                &[
                    &format!(
                        "{} couldn’t play this file. It may use a",
                        self.target_name()
                    ),
                    "codec your TV doesn’t support — H.264 video with",
                    "AAC audio works best.",
                ],
                13.0,
                palette().ink_3,
                20.0,
            );
            let primary = local_rect(
                content,
                18.0,
                content.height() - 115.0,
                content.width() - 36.0,
                44.0,
            );
            let secondary = local_rect(
                content,
                18.0,
                content.height() - 62.0,
                content.width() - 36.0,
                44.0,
            );
            if paint_primary_button(ui, primary, "Try again", true).clicked() {
                self.begin_cast(ui.ctx().clone());
            }
            if paint_outline_button(ui, secondary, "Choose another device").clicked() {
                self.screen = Screen::Devices;
            }
        }
    }

    impl eframe::App for DropcastApp {
        fn logic(&mut self, context: &egui::Context, _frame: &mut eframe::Frame) {
            self.handle_events();
            if matches!(
                self.screen,
                Screen::Scanning | Screen::Connecting | Screen::Playing
            ) {
                context.request_repaint_after(Duration::from_millis(16));
            }
        }

        fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
            let dropped = ui.input(|input| input.raw.dropped_files.clone());
            if let Some(path) = dropped.into_iter().find_map(|file| file.path) {
                self.begin_drop(path, ui.ctx().clone());
            }
            self.draw(ui);
        }

        fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
            if let Some(signal) = &self.signal_tx {
                let _ = signal.try_send(());
            }
        }

        fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
            palette_for(visuals.dark_mode).bg.to_normalized_gamma_f32()
        }
    }

    fn configure_context(context: &egui::Context, forced_theme: Option<egui::Theme>) {
        context.set_theme(
            forced_theme.map_or(egui::ThemePreference::System, egui::ThemePreference::from),
        );
        for (theme, palette) in [
            (egui::Theme::Dark, DARK_PALETTE),
            (egui::Theme::Light, LIGHT_PALETTE),
        ] {
            let mut style = (*context.style_of(theme)).clone();
            style.visuals.override_text_color = Some(palette.ink);
            style.visuals.panel_fill = palette.bg;
            style.visuals.window_fill = palette.bg;
            style.visuals.faint_bg_color = palette.surface;
            style.visuals.extreme_bg_color = palette.surface_2;
            style.spacing.button_padding = Vec2::new(14.0, 10.0);
            context.set_style_of(theme, style);
        }

        if let Ok(bytes) = std::fs::read("/System/Library/Fonts/SFNS.ttf") {
            let mut fonts = FontDefinitions::default();
            fonts
                .font_data
                .insert("sf-pro".to_owned(), FontData::from_owned(bytes).into());
            fonts
                .families
                .entry(FontFamily::Proportional)
                .or_default()
                .insert(0, "sf-pro".to_owned());
            context.set_fonts(fonts);
        }
    }

    fn preview_playback() -> PlaybackStatus {
        PlaybackStatus {
            current_time: 3_540.0,
            duration: Some(9_960.0),
            is_playing: true,
            volume: 0.7,
            muted: false,
            active_subtitle: None,
        }
    }

    fn preview_devices() -> Vec<CastDevice> {
        [
            ("Living Room TV", "Google TV Streamer", [192, 168, 1, 42]),
            ("Bedroom", "Chromecast (3rd gen)", [192, 168, 1, 51]),
            ("Kitchen Display", "Nest Hub", [192, 168, 1, 66]),
            (
                "Patio Projector",
                "Chromecast w/ Google TV",
                [192, 168, 1, 73],
            ),
        ]
        .into_iter()
        .map(|(name, model, address)| CastDevice {
            name: name.to_owned(),
            model: Some(model.to_owned()),
            address: IpAddr::V4(Ipv4Addr::from(address)),
        })
        .collect()
    }

    fn file_name(path: &Path) -> String {
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Movie")
            .to_owned()
    }

    fn truncate(text: &str, max_chars: usize) -> String {
        let mut chars = text.chars();
        let shortened = chars.by_ref().take(max_chars).collect::<String>();
        if chars.next().is_some() {
            format!("{shortened}…")
        } else {
            shortened
        }
    }

    fn movie_title(file_name: &str) -> String {
        let stem = Path::new(file_name)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .unwrap_or("Movie");
        truncate(&stem.replace(['.', '_'], " "), 42)
    }

    fn movie_meta(file_name: &str, duration: Option<f64>) -> String {
        let kind = Path::new(file_name)
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or("video")
            .to_uppercase();
        duration.map_or(kind.clone(), |duration| {
            format!("{kind} · {}", format_time(duration))
        })
    }

    fn format_time(seconds: f64) -> String {
        let seconds = seconds.max(0.0).round() as u64;
        let hours = seconds / 3600;
        let minutes = (seconds % 3600) / 60;
        let seconds = seconds % 60;
        if hours > 0 {
            format!("{hours}:{minutes:02}:{seconds:02}")
        } else {
            format!("{minutes}:{seconds:02}")
        }
    }

    fn media_time_at_pointer(pointer_x: f32, track: Rect, duration: f64) -> f64 {
        let ratio = ((pointer_x - track.left()) / track.width()).clamp(0.0, 1.0);
        f64::from(ratio) * duration.max(0.0)
    }

    fn local_rect(content: Rect, x: f32, y: f32, width: f32, height: f32) -> Rect {
        Rect::from_min_size(
            Pos2::new(content.left() + x, content.top() + y),
            Vec2::new(width, height),
        )
    }

    fn paint_button(
        ui: &mut egui::Ui,
        rect: Rect,
        label: &str,
        fill: Color32,
        text: Color32,
        radius: f32,
        border: Option<Color32>,
    ) -> Response {
        let response = ui.allocate_rect(rect, Sense::click());
        let fill = if response.hovered() {
            fill.gamma_multiply(1.15)
        } else {
            fill
        };
        ui.painter().rect_filled(rect, radius, fill);
        if let Some(border) = border {
            ui.painter().rect_stroke(
                rect,
                radius,
                Stroke::new(1.0, border.gamma_multiply(0.55)),
                StrokeKind::Inside,
            );
        }
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(14.0),
            text,
        );
        response
    }

    fn paint_primary_button(ui: &mut egui::Ui, rect: Rect, label: &str, enabled: bool) -> Response {
        let response = ui.allocate_rect(
            rect,
            if enabled {
                Sense::click()
            } else {
                Sense::hover()
            },
        );
        if enabled {
            let multiplier = if response.hovered() { 1.08 } else { 1.0 };
            paint_gradient_rect(
                ui.painter(),
                rect,
                11.0,
                ACCENT.gamma_multiply(multiplier),
                ACCENT_2.gamma_multiply(multiplier),
            );
        } else {
            ui.painter().rect_filled(rect, 11.0, palette().surface_2);
        }
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::proportional(14.0),
            if enabled { ACCENT_INK } else { palette().ink_3 },
        );
        response
    }

    fn paint_outline_button(ui: &mut egui::Ui, rect: Rect, label: &str) -> Response {
        paint_button(
            ui,
            rect,
            label,
            Color32::TRANSPARENT,
            palette().ink,
            11.0,
            Some(palette().line),
        )
    }

    #[derive(Clone, Copy)]
    enum Icon {
        Rescan,
        Volume,
        Muted,
        Stop,
    }

    fn paint_icon_button(ui: &mut egui::Ui, rect: Rect, icon: Icon) -> Response {
        let response = ui.allocate_rect(rect, Sense::click());
        if response.hovered() {
            ui.painter().rect_filled(rect, 9.0, palette().surface);
        }
        ui.painter().rect_stroke(
            rect,
            9.0,
            Stroke::new(1.0, palette().line),
            StrokeKind::Inside,
        );
        let center = rect.center();
        match icon {
            Icon::Rescan => {
                paint_rescan_icon(ui.painter(), center, palette().ink_2);
            }
            Icon::Volume | Icon::Muted => {
                paint_volume_icon(
                    ui.painter(),
                    center,
                    palette().ink_2,
                    matches!(icon, Icon::Volume),
                );
                if matches!(icon, Icon::Muted) {
                    ui.painter().line_segment(
                        [
                            Pos2::new(center.x + 7.0, center.y - 6.0),
                            Pos2::new(center.x + 13.0, center.y + 6.0),
                        ],
                        Stroke::new(1.7, palette().ink_2),
                    );
                    ui.painter().line_segment(
                        [
                            Pos2::new(center.x + 13.0, center.y - 6.0),
                            Pos2::new(center.x + 7.0, center.y + 6.0),
                        ],
                        Stroke::new(1.7, palette().ink_2),
                    );
                }
            }
            Icon::Stop => {
                ui.painter().rect_filled(
                    Rect::from_center_size(center, Vec2::splat(12.0)),
                    2.5,
                    if response.hovered() {
                        RED
                    } else {
                        palette().ink_2
                    },
                );
            }
        }
        response
    }

    fn paint_rescan_icon(painter: &egui::Painter, center: Pos2, color: Color32) {
        let radius = 5.625;
        let arc = (0..=28)
            .map(|index| {
                let angle = std::f32::consts::TAU * 0.875 * index as f32 / 28.0;
                Pos2::new(
                    center.x + angle.cos() * radius,
                    center.y + angle.sin() * radius,
                )
            })
            .collect::<Vec<_>>();
        painter.add(Shape::line(arc, Stroke::new(1.4, color)));
        painter.add(Shape::line(
            vec![
                Pos2::new(center.x + 5.625, center.y - 5.0),
                Pos2::new(center.x + 5.625, center.y - 1.875),
                Pos2::new(center.x + 2.5, center.y - 1.875),
            ],
            Stroke::new(1.4, color),
        ));
    }

    fn paint_volume_icon(painter: &egui::Painter, center: Pos2, color: Color32, show_wave: bool) {
        let scale = 0.75;
        painter.add(Shape::convex_polygon(
            vec![
                Pos2::new(center.x - 8.0 * scale, center.y - 3.0 * scale),
                Pos2::new(center.x - 8.0 * scale, center.y + 3.0 * scale),
                Pos2::new(center.x - 4.0 * scale, center.y + 3.0 * scale),
                Pos2::new(center.x + scale, center.y + 7.0 * scale),
                Pos2::new(center.x + scale, center.y - 7.0 * scale),
                Pos2::new(center.x - 4.0 * scale, center.y - 3.0 * scale),
            ],
            color,
            Stroke::NONE,
        ));
        if show_wave {
            let arc = (0..=12)
                .map(|index| {
                    let angle =
                        -std::f32::consts::FRAC_PI_2 + std::f32::consts::PI * index as f32 / 12.0;
                    Pos2::new(
                        center.x + (4.0 + 3.5 * angle.cos()) * scale,
                        center.y + 3.5 * angle.sin() * scale,
                    )
                })
                .collect::<Vec<_>>();
            painter.add(Shape::line(arc, Stroke::new(1.35, color)));
        }
    }

    fn paint_logo(painter: &egui::Painter, rect: Rect) {
        paint_gradient_rect(painter, rect, 20.0, ACCENT, ACCENT_2);
        let center = rect.center();
        painter.circle_stroke(
            center,
            rect.width() * 0.18,
            Stroke::new(2.4, Color32::from_rgba_unmultiplied(255, 255, 255, 80)),
        );
        painter.circle_stroke(
            center,
            rect.width() * 0.26,
            Stroke::new(2.4, Color32::from_rgba_unmultiplied(255, 255, 255, 38)),
        );
        let scale = rect.width() / 166.0;
        painter.add(Shape::convex_polygon(
            vec![
                Pos2::new(center.x - 8.0 * scale, center.y - 14.0 * scale),
                Pos2::new(center.x - 8.0 * scale, center.y + 14.0 * scale),
                Pos2::new(center.x + 16.0 * scale, center.y),
            ],
            Color32::WHITE,
            Stroke::NONE,
        ));
    }

    fn paint_gradient_rect(
        painter: &egui::Painter,
        rect: Rect,
        radius: f32,
        left: Color32,
        right: Color32,
    ) {
        let strip_width = 1.0;
        let mut x = rect.left();
        while x < rect.right() {
            let strip_right = (x + strip_width).min(rect.right());
            let center_x = (x + strip_right) * 0.5;
            let edge_distance = (center_x - rect.left())
                .min(rect.right() - center_x)
                .clamp(0.0, radius);
            let corner_inset =
                radius - (radius.mul_add(radius, -(radius - edge_distance).powi(2))).sqrt();
            let progress = ((center_x - rect.left()) / rect.width()).clamp(0.0, 1.0);
            painter.rect_filled(
                Rect::from_min_max(
                    Pos2::new(x, rect.top() + corner_inset),
                    Pos2::new(strip_right, rect.bottom() - corner_inset),
                ),
                0.0,
                lerp_color(left, right, progress),
            );
            x += strip_width;
        }
    }

    fn lerp_color(left: Color32, right: Color32, progress: f32) -> Color32 {
        let [lr, lg, lb, la] = left.to_array();
        let [rr, rg, rb, ra] = right.to_array();
        let lerp = |start: u8, end: u8| {
            (f32::from(start) + (f32::from(end) - f32::from(start)) * progress).round() as u8
        };
        Color32::from_rgba_unmultiplied(lerp(lr, rr), lerp(lg, rg), lerp(lb, rb), lerp(la, ra))
    }

    fn paint_dashed_rect(painter: &egui::Painter, rect: Rect, color: Color32) {
        let radius = 18.0;
        let mut points = vec![Pos2::new(rect.left() + radius, rect.top())];
        let corners = [
            (
                Pos2::new(rect.right() - radius, rect.top() + radius),
                -std::f32::consts::FRAC_PI_2,
            ),
            (
                Pos2::new(rect.right() - radius, rect.bottom() - radius),
                0.0,
            ),
            (
                Pos2::new(rect.left() + radius, rect.bottom() - radius),
                std::f32::consts::FRAC_PI_2,
            ),
            (
                Pos2::new(rect.left() + radius, rect.top() + radius),
                std::f32::consts::PI,
            ),
        ];
        for (center, start) in corners {
            for step in 0..=6 {
                let angle = start + std::f32::consts::FRAC_PI_2 * step as f32 / 6.0;
                points.push(Pos2::new(
                    center.x + angle.cos() * radius,
                    center.y + angle.sin() * radius,
                ));
            }
        }
        points.push(Pos2::new(rect.left() + radius, rect.top()));
        painter.add(Shape::dashed_line(
            &points,
            Stroke::new(2.0, color),
            8.0,
            6.0,
        ));
    }

    fn paint_ripples(painter: &egui::Painter, center: Pos2, time: f32, period: f32) {
        for offset in [0.0_f32, period / 3.0, period * 2.0 / 3.0] {
            let phase = ((time + offset) % period) / period;
            let radius = 24.0 + phase * 74.0;
            let alpha = ((1.0 - phase) * 100.0) as u8;
            painter.circle_stroke(
                center,
                radius,
                Stroke::new(2.0, Color32::from_rgba_unmultiplied(244, 181, 105, alpha)),
            );
        }
    }

    fn paint_tv(painter: &egui::Painter, center: Pos2, size: Vec2, accent: Color32) {
        let rect = Rect::from_center_size(center, size);
        painter.rect_filled(rect, 8.0, palette().surface_2);
        painter.rect_stroke(
            rect,
            8.0,
            Stroke::new(1.5, palette().line),
            StrokeKind::Inside,
        );
        let scale = size.y / 48.0;
        painter.add(Shape::convex_polygon(
            vec![
                Pos2::new(center.x - 6.0 * scale, center.y - 10.0 * scale),
                Pos2::new(center.x - 6.0 * scale, center.y + 10.0 * scale),
                Pos2::new(center.x + 11.0 * scale, center.y),
            ],
            accent,
            Stroke::NONE,
        ));
        painter.rect_filled(
            Rect::from_center_size(
                Pos2::new(center.x, rect.bottom() + 4.0),
                Vec2::new(size.x * 0.28, 3.0),
            ),
            2.0,
            palette().line,
        );
    }

    fn paint_connecting_tv(painter: &egui::Painter, center: Pos2, time: f32) {
        let rect = Rect::from_center_size(center, Vec2::new(78.0, 56.0));
        painter.rect_filled(rect, 9.0, palette().surface_2);
        painter.rect_stroke(
            rect,
            9.0,
            Stroke::new(1.5, ACCENT.gamma_multiply(0.6)),
            StrokeKind::Inside,
        );
        let start = time * 3.2;
        let radius = 15.0;
        let segments = 28;
        let points = (0..=segments)
            .map(|index| {
                let angle = start + std::f32::consts::TAU * index as f32 / segments as f32 * 0.72;
                Pos2::new(
                    center.x + angle.cos() * radius,
                    center.y + angle.sin() * radius,
                )
            })
            .collect::<Vec<_>>();
        painter.add(Shape::line(points, Stroke::new(3.0, ACCENT)));
    }

    fn paint_check(painter: &egui::Painter, center: Pos2) {
        painter.circle_filled(center, 11.0, ACCENT);
        painter.line_segment(
            [
                Pos2::new(center.x - 5.0, center.y),
                Pos2::new(center.x - 1.0, center.y + 4.0),
            ],
            Stroke::new(2.3, ACCENT_INK),
        );
        painter.line_segment(
            [
                Pos2::new(center.x - 1.0, center.y + 4.0),
                Pos2::new(center.x + 6.0, center.y - 5.0),
            ],
            Stroke::new(2.3, ACCENT_INK),
        );
    }

    fn paint_small_check(painter: &egui::Painter, center: Pos2) {
        painter.line_segment(
            [
                Pos2::new(center.x - 6.0, center.y),
                Pos2::new(center.x - 2.0, center.y + 4.0),
            ],
            Stroke::new(2.2, ACCENT),
        );
        painter.line_segment(
            [
                Pos2::new(center.x - 2.0, center.y + 4.0),
                Pos2::new(center.x + 7.0, center.y - 6.0),
            ],
            Stroke::new(2.2, ACCENT),
        );
    }

    fn paint_movie_still(painter: &egui::Painter, rect: Rect) {
        let clipped = painter.with_clip_rect(rect);
        clipped.rect_filled(rect, 12.0, palette().surface_2);
        let mut x = rect.left() - rect.height();
        while x < rect.right() {
            clipped.line_segment(
                [
                    Pos2::new(x, rect.bottom()),
                    Pos2::new(x + rect.height(), rect.top()),
                ],
                Stroke::new(10.0, palette().movie_stripe),
            );
            x += 22.0;
        }
        clipped.text(
            rect.center(),
            Align2::CENTER_CENTER,
            "MOVIE STILL",
            FontId::monospace(11.0),
            palette().ink_3,
        );
        let shade =
            Rect::from_min_max(Pos2::new(rect.left(), rect.center().y), rect.right_bottom());
        clipped.rect_filled(shade, 0.0, Color32::from_rgba_unmultiplied(0, 0, 0, 118));
        painter.rect_stroke(
            rect,
            12.0,
            Stroke::new(1.0, palette().line),
            StrokeKind::Inside,
        );
    }

    fn paint_casting_badge(painter: &egui::Painter, still: Rect, target_name: &str, time: f32) {
        let width = 104.0 + target_name.len() as f32 * 3.4;
        let badge = Rect::from_min_size(
            Pos2::new(still.left() + 10.0, still.top() + 10.0),
            Vec2::new(width, 27.0),
        );
        painter.rect_filled(badge, 999.0, Color32::from_rgba_unmultiplied(0, 0, 0, 135));
        painter.rect_stroke(
            badge,
            999.0,
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 30)),
            StrokeKind::Inside,
        );
        let pulse = 0.65 + (time * 4.0).sin().abs() * 0.35;
        painter.circle_filled(
            Pos2::new(badge.left() + 13.0, badge.center().y),
            3.5 * pulse,
            ACCENT,
        );
        painter.text(
            Pos2::new(badge.left() + 23.0, badge.center().y),
            Align2::LEFT_CENTER,
            format!("Casting to {target_name}"),
            FontId::proportional(11.0),
            Color32::WHITE,
        );
    }

    fn paint_slider(
        ui: &mut egui::Ui,
        rect: Rect,
        progress: f32,
        fill: (Color32, Color32),
        track_height: f32,
        thumb_radius: f32,
        thumb_color: Color32,
    ) -> Response {
        let response = ui.allocate_rect(rect, Sense::click_and_drag());
        let track = Rect::from_center_size(rect.center(), Vec2::new(rect.width(), track_height));
        ui.painter().rect_filled(track, 3.0, palette().surface_2);
        let filled = Rect::from_min_max(
            track.left_top(),
            Pos2::new(
                track.left() + track.width() * progress.clamp(0.0, 1.0),
                track.bottom(),
            ),
        );
        if filled.width() > 0.0 {
            paint_gradient_rect(
                ui.painter(),
                filled,
                (track_height * 0.5).min(filled.width() * 0.5),
                fill.0,
                fill.1,
            );
        }
        ui.painter().circle_filled(
            Pos2::new(filled.right(), track.center().y),
            thumb_radius,
            thumb_color,
        );
        response
    }

    fn paint_skip_button(ui: &mut egui::Ui, rect: Rect, seconds: i32) -> Response {
        let response = ui.allocate_rect(rect, Sense::click());
        let center = rect.center();
        let direction = seconds.signum() as f32;
        let scale = 25.0 / 24.0;
        let start_angle = if direction < 0.0 {
            -3.0 * std::f32::consts::FRAC_PI_4
        } else {
            -std::f32::consts::FRAC_PI_4
        };
        let sweep = if direction < 0.0 {
            std::f32::consts::TAU * 0.875
        } else {
            -std::f32::consts::TAU * 0.875
        };
        let arc = (0..=28)
            .map(|index| {
                let angle = start_angle + sweep * index as f32 / 28.0;
                Pos2::new(
                    center.x + angle.cos() * 8.0 * scale,
                    center.y + angle.sin() * 8.0 * scale,
                )
            })
            .collect::<Vec<_>>();
        ui.painter()
            .add(Shape::line(arc, Stroke::new(1.9, palette().ink)));

        let arrow_x = center.x + direction * 5.5 * scale;
        let arrow_y = center.y - 4.0 * scale;
        ui.painter().add(Shape::line(
            vec![
                Pos2::new(arrow_x - direction * 3.0 * scale, arrow_y - 3.0 * scale),
                Pos2::new(arrow_x, arrow_y),
                Pos2::new(arrow_x - direction * 3.0 * scale, arrow_y + 3.0 * scale),
            ],
            Stroke::new(1.9, palette().ink),
        ));
        ui.painter().text(
            Pos2::new(center.x - direction * 0.5, center.y + 3.2),
            Align2::CENTER_CENTER,
            "10",
            FontId::proportional(7.5),
            palette().ink,
        );
        response
    }

    fn paint_play_button(ui: &mut egui::Ui, rect: Rect, playing: bool) -> Response {
        let response = ui.allocate_rect(rect, Sense::click());
        paint_gradient_rect(ui.painter(), rect, rect.width() * 0.5, ACCENT, ACCENT_2);
        if playing {
            for offset in [-4.0_f32, 4.0] {
                ui.painter().rect_filled(
                    Rect::from_center_size(
                        Pos2::new(rect.center().x + offset, rect.center().y),
                        Vec2::new(4.0, 16.0),
                    ),
                    1.5,
                    ACCENT_INK,
                );
            }
        } else {
            ui.painter().add(Shape::convex_polygon(
                vec![
                    Pos2::new(rect.center().x - 7.0, rect.center().y - 10.0),
                    Pos2::new(rect.center().x - 7.0, rect.center().y + 10.0),
                    Pos2::new(rect.center().x + 11.0, rect.center().y),
                ],
                ACCENT_INK,
                Stroke::NONE,
            ));
        }
        response
    }

    fn paint_cc_button(ui: &mut egui::Ui, rect: Rect, active: bool) -> Response {
        let response = ui.allocate_rect(rect, Sense::click());
        ui.painter().rect_filled(
            rect,
            9.0,
            if active { ACCENT } else { Color32::TRANSPARENT },
        );
        ui.painter().rect_stroke(
            rect,
            9.0,
            Stroke::new(1.0, if active { ACCENT } else { palette().line }),
            StrokeKind::Inside,
        );
        ui.painter().text(
            rect.center(),
            Align2::CENTER_CENTER,
            "CC",
            FontId::proportional(12.0),
            if active { ACCENT_INK } else { palette().ink_2 },
        );
        response
    }

    fn paint_error_tile(painter: &egui::Painter, center: Pos2, playback: bool) {
        let tile = Rect::from_center_size(center, Vec2::splat(84.0));
        painter.rect_filled(tile, 22.0, palette().surface);
        painter.rect_stroke(
            tile,
            22.0,
            Stroke::new(1.0, palette().line),
            StrokeKind::Inside,
        );
        if playback {
            painter.circle_stroke(center, 18.0, Stroke::new(1.8, ACCENT_2));
            painter.line_segment(
                [
                    Pos2::new(center.x, center.y - 10.0),
                    Pos2::new(center.x, center.y + 3.0),
                ],
                Stroke::new(2.0, ACCENT_2),
            );
            painter.circle_filled(Pos2::new(center.x, center.y + 11.0), 1.8, ACCENT_2);
        } else {
            for radius in [8.0_f32, 16.0] {
                let points = (0..=16)
                    .map(|index| {
                        let angle = std::f32::consts::PI * (1.15 + 0.7 * index as f32 / 16.0);
                        Pos2::new(
                            center.x + angle.cos() * radius,
                            center.y + angle.sin() * radius,
                        )
                    })
                    .collect::<Vec<_>>();
                painter.add(Shape::line(points, Stroke::new(1.8, palette().ink_3)));
            }
            painter.circle_filled(Pos2::new(center.x, center.y + 15.0), 2.0, palette().ink_3);
            painter.line_segment(
                [
                    Pos2::new(center.x - 19.0, center.y - 19.0),
                    Pos2::new(center.x + 19.0, center.y + 19.0),
                ],
                Stroke::new(2.2, ACCENT_2),
            );
        }
    }

    fn paint_multiline_center(
        painter: &egui::Painter,
        origin: Pos2,
        lines: &[&str],
        size: f32,
        color: Color32,
        line_height: f32,
    ) {
        for (index, line) in lines.iter().enumerate() {
            painter.text(
                Pos2::new(origin.x, origin.y + index as f32 * line_height),
                Align2::CENTER_CENTER,
                *line,
                FontId::proportional(size),
                color,
            );
        }
    }

    fn parse_preview() -> Option<Screen> {
        let mut args = std::env::args().skip(1);
        while let Some(argument) = args.next() {
            if argument == "--preview" {
                return args.next().and_then(|state| match state.as_str() {
                    "drop" => Some(Screen::Drop),
                    "scanning" => Some(Screen::Scanning),
                    "devices" => Some(Screen::Devices),
                    "connecting" => Some(Screen::Connecting),
                    "playing" => Some(Screen::Playing),
                    "subtitles" => Some(Screen::Playing),
                    "no-devices" => Some(Screen::NoDevices),
                    "playback-failed" => Some(Screen::PlaybackFailed),
                    _ => None,
                });
            }
        }
        None
    }

    fn parse_appearance() -> Option<egui::Theme> {
        let mut args = std::env::args().skip(1);
        while let Some(argument) = args.next() {
            if argument == "--appearance" {
                return args
                    .next()
                    .and_then(|appearance| match appearance.as_str() {
                        "dark" => Some(egui::Theme::Dark),
                        "light" => Some(egui::Theme::Light),
                        _ => None,
                    });
            }
        }
        None
    }

    pub fn run() -> eframe::Result {
        let preview_name = std::env::args()
            .skip(1)
            .collect::<Vec<_>>()
            .windows(2)
            .find(|pair| pair[0] == "--preview")
            .map(|pair| pair[1].clone());
        let preview = parse_preview();
        let forced_theme = parse_appearance();
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default()
                .with_title("Dropcast")
                .with_app_id("dev.mareknogiec.dropcast")
                .with_inner_size([WINDOW_WIDTH, WINDOW_HEIGHT])
                .with_min_inner_size([WINDOW_WIDTH, WINDOW_HEIGHT])
                .with_max_inner_size([WINDOW_WIDTH, WINDOW_HEIGHT])
                .with_resizable(false)
                .with_drag_and_drop(true)
                .with_icon(icon_data()),
            centered: true,
            renderer: eframe::Renderer::Glow,
            ..Default::default()
        };
        eframe::run_native(
            "Dropcast",
            options,
            Box::new(move |context| {
                let mut app = DropcastApp::new(&context.egui_ctx, preview, forced_theme);
                if preview_name.as_deref() == Some("subtitles") {
                    app.subtitle_sheet = true;
                }
                Ok(Box::new(app))
            }),
        )
    }

    fn icon_data() -> egui::IconData {
        const SIZE: u32 = 128;
        let mut rgba = Vec::with_capacity((SIZE * SIZE * 4) as usize);
        for y in 0..SIZE {
            for x in 0..SIZE {
                let px = x as f32 + 0.5;
                let py = y as f32 + 0.5;
                let inside = rounded_rect_contains(px, py, 8.0, 8.0, 112.0, 112.0, 27.0);
                let radius = ((px - 64.0).powi(2) + (py - 64.0).powi(2)).sqrt();
                let ring = (radius - 36.0).abs() <= 3.0 || (radius - 49.0).abs() <= 2.5;
                let play = (51.0..=83.0).contains(&px)
                    && (44.0..=84.0).contains(&py)
                    && px <= 51.0 + (py - 44.0).min(84.0 - py) * 1.6;
                let color = if play {
                    Color32::WHITE
                } else if ring && inside {
                    Color32::from_rgba_unmultiplied(255, 255, 255, 70)
                } else if inside {
                    lerp_color(ACCENT, ACCENT_2, ((px - 8.0) / 112.0).clamp(0.0, 1.0))
                } else {
                    Color32::TRANSPARENT
                };
                rgba.extend_from_slice(&color.to_array());
            }
        }
        egui::IconData {
            rgba,
            width: SIZE,
            height: SIZE,
        }
    }

    fn rounded_rect_contains(
        x: f32,
        y: f32,
        left: f32,
        top: f32,
        width: f32,
        height: f32,
        radius: f32,
    ) -> bool {
        let nearest_x = x.clamp(left + radius, left + width - radius);
        let nearest_y = y.clamp(top + radius, top + height - radius);
        (x - nearest_x).powi(2) + (y - nearest_y).powi(2) <= radius.powi(2)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_single_device_still_requires_confirmation() {
            let mut app = DropcastApp::new(&egui::Context::default(), None, None);
            app.screen = Screen::Scanning;
            app.worker_tx
                .send(WorkerEvent::Devices {
                    movie: PathBuf::from("movie.mp4"),
                    devices: vec![preview_devices().remove(0)],
                })
                .unwrap();
            app.handle_events();
            assert_eq!(app.screen, Screen::Devices);
            assert_eq!(app.devices.len(), 1);
            assert_eq!(app.selected_device, None);
        }

        #[test]
        fn preview_states_have_deterministic_data() {
            let app = DropcastApp::new(&egui::Context::default(), Some(Screen::Playing), None);
            assert_eq!(app.devices.len(), 4);
            assert_eq!(app.playback.duration, Some(9_960.0));
            assert_eq!(app.target_name(), "Living Room TV");
        }

        #[test]
        fn appearance_defaults_to_system_and_can_be_overridden_for_previews() {
            let system_context = egui::Context::default();
            configure_context(&system_context, None);
            assert_eq!(
                system_context.options(|options| options.theme_preference),
                egui::ThemePreference::System
            );

            let light_context = egui::Context::default();
            configure_context(&light_context, Some(egui::Theme::Light));
            assert_eq!(light_context.theme(), egui::Theme::Light);
            assert_eq!(
                light_context
                    .style_of(egui::Theme::Light)
                    .visuals
                    .panel_fill,
                LIGHT_PALETTE.bg
            );
        }

        #[test]
        fn scrubber_maps_and_clamps_pointer_positions() {
            let track = Rect::from_min_size(Pos2::new(10.0, 0.0), Vec2::new(200.0, 16.0));
            assert_eq!(media_time_at_pointer(10.0, track, 1_000.0), 0.0);
            assert_eq!(media_time_at_pointer(110.0, track, 1_000.0), 500.0);
            assert_eq!(media_time_at_pointer(250.0, track, 1_000.0), 1_000.0);
        }
    }
}

#[cfg(target_os = "macos")]
fn main() -> eframe::Result {
    macos::run()
}
