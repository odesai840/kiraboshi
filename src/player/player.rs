use crate::audio::AudioEngine;
use eframe::egui;
use rand::seq::IndexedRandom;
use std::path::{Path, PathBuf};

#[derive(PartialEq, Clone, Copy)]
enum LoopMode {
    Off,
    One,
    All,
}

fn exe_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|p| p.to_path_buf()))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn load_icon() -> Option<egui::IconData> {
    let icon_path = exe_dir().join("assets/icon.ico");
    let img = image::open(&icon_path).ok()?;
    let rgba = img.to_rgba8();
    let (w, h) = rgba.dimensions();
    Some(egui::IconData {
        rgba: rgba.into_raw(),
        width: w,
        height: h,
    })
}

pub fn run(file_arg: Option<PathBuf>) -> Result<(), eframe::Error> {
    let standalone = file_arg.is_some();
    let window_size = if standalone { [600.0, 320.0] } else { [900.0, 620.0] };

    let mut viewport = egui::ViewportBuilder::default()
        .with_inner_size(window_size)
        .with_title("Kiraboshi")
        .with_decorations(false)
        .with_resizable(false);

    if let Some(icon) = load_icon() {
        viewport = viewport.with_icon(std::sync::Arc::new(icon));
    }

    let options = eframe::NativeOptions {
        centered: true,
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Kiraboshi",
        options,
        Box::new(move |cc| Ok(Box::new(KiraboshiApp::new(cc, file_arg)))),
    )
}

pub struct KiraboshiApp {
    audio: AudioEngine,
    volume: f32,
    error_message: Option<String>,
    seeking: bool,
    seek_position: f64,
    seek_cooldown: u8,
    playlist: Vec<PathBuf>,
    was_playing: bool,
    drag_index: Option<usize>,
    loop_mode: LoopMode,
    shuffle: bool,
    title_icon: Option<egui::TextureHandle>,
    expected_size: Option<egui::Vec2>,
    standalone: bool,
}

impl KiraboshiApp {
    pub fn new(cc: &eframe::CreationContext<'_>, file_arg: Option<PathBuf>) -> Self {
        let title_icon = Self::load_title_icon(&cc.egui_ctx);
        let standalone = file_arg.is_some();

        let mut visuals = egui::Visuals::dark();
        visuals.selection.bg_fill = egui::Color32::from_rgb(170, 120, 25);
        visuals.selection.stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(220, 175, 55));
        visuals.widgets.inactive.bg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(145, 115, 35));
        visuals.widgets.inactive.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(160, 135, 60));
        visuals.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(195, 158, 50));
        visuals.widgets.hovered.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(215, 175, 65));
        visuals.widgets.active.bg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(220, 178, 60));
        visuals.widgets.active.fg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(230, 190, 75));
        cc.egui_ctx.set_visuals(visuals);
        let mut app = Self {
            audio: AudioEngine::new(),
            volume: 0.5,
            error_message: None,
            seeking: false,
            seek_position: 0.0,
            seek_cooldown: 0,
            playlist: if standalone { Vec::new() } else { Self::load_playlist() },
            was_playing: false,
            drag_index: None,
            loop_mode: LoopMode::Off,
            shuffle: false,
            title_icon,
            expected_size: None,
            standalone,
        };
        app.audio.set_volume(app.volume);
        if let Some(path) = file_arg {
            let _ = app.audio.play_song(&path);
        } else {
            app.scan_songs();
        }
        app
    }

    fn load_title_icon(ctx: &egui::Context) -> Option<egui::TextureHandle> {
        let icon_path = exe_dir().join("assets/icon.ico");
        let img = image::open(&icon_path).ok()?;
        let rgba = img.to_rgba8();
        let (w, h) = rgba.dimensions();
        let color_image = egui::ColorImage::from_rgba_unmultiplied(
            [w as usize, h as usize],
            &rgba.into_raw(),
        );
        Some(ctx.load_texture("title_icon", color_image, egui::TextureOptions::LINEAR))
    }

    fn format_time(seconds: f64) -> String {
        let mins = (seconds / 60.0) as i32;
        let secs = (seconds % 60.0) as i32;
        format!("{:02}:{:02}", mins, secs)
    }

    fn display_name(path: &Path) -> String {
        path.file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string()
    }

    fn data_dir() -> PathBuf {
        PathBuf::from("data")
    }

    fn playlist_file() -> PathBuf {
        Self::data_dir().join(".kiraboshi")
    }

    fn load_playlist() -> Vec<PathBuf> {
        let path = Self::playlist_file();
        std::fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.is_empty())
            .map(PathBuf::from)
            .collect()
    }

    fn save_playlist(&self) {
        let contents: String = self.playlist
            .iter()
            .filter_map(|p| p.to_str())
            .collect::<Vec<_>>()
            .join("\n");
        let _ = std::fs::write(Self::playlist_file(), contents);
    }

    fn scan_songs(&mut self) {
        let dir = Self::data_dir();
        let extensions = ["mp3", "wav", "ogg", "flac"];
        let mut on_disk: Vec<PathBuf> = std::fs::read_dir(&dir)
            .into_iter()
            .flatten()
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| extensions.contains(&ext.to_lowercase().as_str()))
                    .unwrap_or(false)
            })
            .collect();
        on_disk.sort();
        self.playlist.retain(|p| on_disk.contains(p));
        let mut changed = false;
        for path in &on_disk {
            if !self.playlist.contains(path) {
                self.playlist.push(path.clone());
                changed = true;
            }
        }
        if changed {
            self.save_playlist();
        }
    }

    fn copy_to_data(&self, source: &PathBuf) -> Result<PathBuf, String> {
        let dir = Self::data_dir();
        std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create data dir: {}", e))?;
        let file_name = source.file_name().ok_or("Invalid file name")?;
        let dest = dir.join(file_name);
        if dest != *source {
            std::fs::copy(source, &dest)
                .map_err(|e| format!("Failed to copy file: {}", e))?;
        }
        Ok(dest)
    }

    fn play_next(&mut self) {
        if self.playlist.is_empty() {
            return;
        }
        if self.loop_mode == LoopMode::One {
            if let Some(current) = self.audio.current_file().cloned() {
                let _ = self.audio.play_song(&current);
            }
            return;
        }
        if self.shuffle {
            let current = self.audio.current_file().cloned();
            let candidates: Vec<&PathBuf> = self
                .playlist
                .iter()
                .filter(|p| current.as_ref() != Some(*p) || self.playlist.len() == 1)
                .collect();
            if let Some(next) = candidates.choose(&mut rand::rng()) {
                let next = (*next).clone();
                let _ = self.audio.play_song(&next);
            }
            return;
        }
        if let Some(current) = self.audio.current_file().cloned() {
            if let Some(idx) = self.playlist.iter().position(|p| *p == current) {
                let next_idx = idx + 1;
                if next_idx < self.playlist.len() {
                    let next = self.playlist[next_idx].clone();
                    let _ = self.audio.play_song(&next);
                } else if self.loop_mode == LoopMode::All {
                    let next = self.playlist[0].clone();
                    let _ = self.audio.play_song(&next);
                }
            }
        }
    }
}

impl eframe::App for KiraboshiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let current_size = ctx.input(|i| {
            i.viewport().inner_rect.map(|r| r.size())
        });
        if let Some(size) = current_size {
            match self.expected_size {
                None => self.expected_size = Some(size),
                Some(expected) => {
                    let diff = (size.x - expected.x).abs() + (size.y - expected.y).abs();
                    if diff > 1.0 {
                        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(expected));
                    }
                }
            }
        }

        ctx.request_repaint();

        if !self.standalone && self.was_playing && self.audio.is_finished() {
            self.play_next();
        }
        if self.standalone && self.was_playing && self.audio.is_finished() {
            if self.loop_mode == LoopMode::One {
                if let Some(current) = self.audio.current_file().cloned() {
                    let _ = self.audio.play_song(&current);
                }
            }
        }
        self.was_playing = self.audio.is_playing();

        egui::TopBottomPanel::top("title_bar")
            .exact_height(30.0)
            .frame(egui::Frame::NONE.fill(egui::Color32::from_gray(25)))
            .show(ctx, |ui| {
                ui.set_clip_rect(ui.max_rect());
                ui.horizontal_centered(|ui| {
                    ui.add_space(8.0);
                    if let Some(icon) = &self.title_icon {
                        let icon_size = egui::vec2(20.0, 20.0);
                        ui.image(egui::load::SizedTexture::new(icon.id(), icon_size));
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let btn_size = egui::vec2(46.0, 30.0);

                        let (close_rect, close_resp) = ui.allocate_exact_size(btn_size, egui::Sense::click());
                        if close_resp.hovered() {
                            ui.painter().rect_filled(close_rect, 0.0, egui::Color32::from_rgb(210, 100, 20));
                        }
                        let cc = close_rect.center();
                        let x_color = if close_resp.hovered() { egui::Color32::from_rgb(255, 225, 120) } else { egui::Color32::from_rgb(185, 155, 65) };
                        let s = 5.0;
                        ui.painter().line_segment([egui::pos2(cc.x - s, cc.y - s), egui::pos2(cc.x + s, cc.y + s)], egui::Stroke::new(1.5, x_color));
                        ui.painter().line_segment([egui::pos2(cc.x + s, cc.y - s), egui::pos2(cc.x - s, cc.y + s)], egui::Stroke::new(1.5, x_color));
                        if close_resp.clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                        }

                        let (min_rect, min_resp) = ui.allocate_exact_size(btn_size, egui::Sense::click());
                        if min_resp.hovered() {
                            ui.painter().rect_filled(min_rect, 0.0, egui::Color32::from_rgba_premultiplied(50, 35, 5, 30));
                        }
                        let nc = min_rect.center();
                        let min_color = if min_resp.hovered() { egui::Color32::from_rgb(255, 220, 100) } else { egui::Color32::from_rgb(185, 155, 65) };
                        ui.painter().line_segment([egui::pos2(nc.x - 5.0, nc.y), egui::pos2(nc.x + 5.0, nc.y)], egui::Stroke::new(1.5, min_color));
                        if min_resp.clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
                        }
                    });

                    let bar = ui.max_rect();
                    let buttons_width = 46.0 * 3.0;
                    let drag_rect = egui::Rect::from_min_max(
                        bar.min,
                        egui::pos2(bar.max.x - buttons_width, bar.max.y),
                    );
                    let title_bar_response = ui.interact(
                        drag_rect,
                        ui.id().with("title_bar_drag"),
                        egui::Sense::click_and_drag(),
                    );
                    if title_bar_response.dragged() {
                        ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                    }
                    if title_bar_response.double_clicked() {
                        let is_maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!is_maximized));
                    }
                });
            });

        let panel_width = 560.0;

        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(24.0);
                {
                    let t = ctx.input(|i| i.time);
                    let text = "Kiraboshi";
                    let mut job = egui::text::LayoutJob::default();
                    for (i, ch) in text.chars().enumerate() {
                        let phase = (t * 3.0 - i as f64 * 0.5) as f32;
                        let wave = phase.sin() * 0.5 + 0.5;
                        let g = (150.0 + wave * 105.0) as u8;
                        let b = (wave * 30.0) as u8;
                        job.append(
                            &ch.to_string(),
                            0.0,
                            egui::TextFormat {
                                font_id: egui::FontId::new(28.0, egui::FontFamily::Proportional),
                                color: egui::Color32::from_rgb(255, g, b),
                                ..Default::default()
                            },
                        );
                    }
                    ui.label(job);
                }
                ui.add_space(24.0);

                ui.allocate_ui(egui::vec2(panel_width, 56.0), |ui| {
                    ui.vertical_centered(|ui| {
                        if let Some(path) = self.audio.current_file() {
                            ui.label(
                                egui::RichText::new("Now Playing")
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(190, 155, 65))
                            );
                            ui.label(
                                egui::RichText::new(Self::display_name(path))
                                    .size(18.0)
                                    .color(egui::Color32::WHITE),
                            );
                        } else {
                            ui.label(
                                egui::RichText::new("Now Playing")
                                    .size(12.0)
                                    .color(egui::Color32::from_rgb(190, 155, 65))
                            );
                            ui.label(
                                egui::RichText::new("No track loaded")
                                    .size(18.0)
                                    .color(egui::Color32::GRAY),
                            );
                        }
                    });
                });

                ui.add_space(8.0);

                let position = self.audio.get_position();
                let duration = self.audio.get_duration();
                if self.seek_cooldown > 0 {
                    self.seek_cooldown -= 1;
                } else if !self.seeking && self.audio.is_playing() {
                    self.seek_position = position;
                }

                ui.allocate_ui(egui::vec2(panel_width, 20.0), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            egui::RichText::new(Self::format_time(self.seek_position))
                                .monospace()
                                .size(12.0),
                        );
                        ui.spacing_mut().slider_width = panel_width - 110.0;
                        let slider = ui.add(
                            egui::Slider::new(
                                &mut self.seek_position,
                                0.0..=duration.max(0.001),
                            )
                            .show_value(false),
                        );
                        if slider.drag_started() {
                            self.seeking = true;
                        }
                        if slider.drag_stopped() {
                            self.audio.seek(self.seek_position);
                            self.seeking = false;
                            self.seek_cooldown = 5;
                        }
                        if slider.changed() && !self.seeking {
                            self.audio.seek(self.seek_position);
                            self.seek_cooldown = 5;
                        }
                        ui.label(
                            egui::RichText::new(Self::format_time(duration))
                                .monospace()
                                .size(12.0),
                        );
                    });
                });

                ui.add_space(12.0);

                let btn = egui::vec2(80.0, 28.0);
                let btn_spacing = 4.0;
                let btn_count = if self.standalone { 3.0 } else { 4.0 };
                let total_w = btn.x * btn_count + btn_spacing * (btn_count - 1.0);
                ui.allocate_ui(egui::vec2(panel_width, 32.0), |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space((panel_width - total_w) / 2.0);
                        ui.spacing_mut().item_spacing.x = btn_spacing;

                        let play_text =
                            if self.audio.is_playing() { "Pause" } else { "Play" };
                        if ui.add_sized(btn, egui::Button::new(egui::RichText::new(play_text).color(egui::Color32::from_gray(175)))).clicked() {
                            if self.audio.is_playing() {
                                self.audio.pause();
                            } else {
                                self.audio.play();
                                self.seek_cooldown = 5;
                            }
                        }

                        if ui.add_sized(btn, egui::Button::new(egui::RichText::new("Stop").color(egui::Color32::from_gray(175)))).clicked() {
                            self.audio.stop();
                            self.seek_position = 0.0;
                        }

                        if self.standalone {
                            let loop_text = if self.loop_mode == LoopMode::One { "Loop On" } else { "Loop" };
                            if ui.add_sized(btn, egui::Button::new(egui::RichText::new(loop_text).color(egui::Color32::from_gray(175)))).clicked() {
                                self.loop_mode = if self.loop_mode == LoopMode::One { LoopMode::Off } else { LoopMode::One };
                            }
                        } else {
                            let loop_text = match self.loop_mode {
                                LoopMode::Off => "Loop",
                                LoopMode::One => "Loop One",
                                LoopMode::All => "Loop All",
                            };
                            if ui.add_sized(btn, egui::Button::new(egui::RichText::new(loop_text).color(egui::Color32::from_gray(175)))).clicked() {
                                self.loop_mode = match self.loop_mode {
                                    LoopMode::Off => LoopMode::One,
                                    LoopMode::One => LoopMode::All,
                                    LoopMode::All => LoopMode::Off,
                                };
                            }

                            let shuf_text = if self.shuffle { "Shuffle On" } else { "Shuffle" };
                            if ui.add_sized(btn, egui::Button::new(egui::RichText::new(shuf_text).color(egui::Color32::from_gray(175)))).clicked() {
                                self.shuffle = !self.shuffle;
                            }
                        }
                    });
                });

                ui.add_space(12.0);

                ui.allocate_ui(egui::vec2(panel_width, 20.0), |ui| {
                    ui.horizontal(|ui| {
                        ui.add_space((panel_width - 280.0) / 2.0);
                        ui.label(egui::RichText::new("Volume").size(12.0));
                        ui.spacing_mut().slider_width = 180.0;
                        if ui
                            .add(
                                egui::Slider::new(&mut self.volume, 0.0..=2.0)
                                    .step_by(0.01)
                                    .show_value(false),
                            )
                            .changed()
                        {
                            self.audio.set_volume(self.volume);
                        }
                        ui.label(
                            egui::RichText::new(format!("{}%", (self.volume * 100.0) as i32))
                                .size(12.0),
                        );
                    });
                });

                if !self.standalone {
                ui.add_space(20.0);
                ui.separator();
                ui.add_space(8.0);

                self.scan_songs();
                let current_file = self.audio.current_file().cloned();

                ui.allocate_ui(egui::vec2(panel_width, 20.0), |ui| {
                    let rect = ui.available_rect_before_wrap();
                    ui.painter().text(
                        egui::pos2(rect.center().x, rect.center().y),
                        egui::Align2::CENTER_CENTER,
                        "Playlist",
                        egui::FontId::new(14.0, egui::FontFamily::Proportional),
                        egui::Color32::from_rgb(190, 155, 65),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(egui::RichText::new("+ Add Song").color(egui::Color32::from_gray(175))).clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Audio Files", &["mp3", "wav", "ogg", "flac"])
                                .pick_file()
                            {
                                match self.copy_to_data(&path) {
                                    Ok(_) => {
                                        self.error_message = None;
                                        self.scan_songs();
                                    }
                                    Err(e) => self.error_message = Some(e),
                                }
                            }
                        }
                    });
                });

                ui.add_space(4.0);

                let drag_handle_width = 24.0;

                let remaining = (ui.available_height() - 24.0).max(60.0);
                egui::ScrollArea::vertical()
                    .max_height(remaining)
                    .show(ui, |ui| {
                        ui.set_min_width(panel_width);
                        if self.playlist.is_empty() {
                            ui.add_space(24.0);
                            ui.vertical_centered(|ui| {
                                ui.label(
                                    egui::RichText::new("No songs found in playlist")
                                        .size(13.0)
                                        .color(egui::Color32::GRAY),
                                );
                            });
                        } else {
                            let songs: Vec<PathBuf> = self.playlist.clone();
                            let mut row_rects: Vec<egui::Rect> = Vec::new();
                            let mut remove_index: Option<usize> = None;
                            let delete_btn_width = 28.0;

                            for (i, song) in songs.iter().enumerate() {
                                let name = Self::display_name(song);
                                let is_current = current_file.as_ref() == Some(song);
                                let is_dragged = self.drag_index == Some(i);

                                let row_width = ui.available_width();
                                let row_height = 32.0;

                                let (handle_rect, handle_response) = ui.allocate_exact_size(
                                    egui::vec2(row_width, row_height),
                                    egui::Sense::click_and_drag(),
                                );
                                row_rects.push(handle_rect);

                                if ui.is_rect_visible(handle_rect) {
                                    if is_dragged {
                                        ui.painter().rect_filled(
                                            handle_rect,
                                            4.0,
                                            egui::Color32::from_rgba_premultiplied(80, 60, 20, 60),
                                        );
                                    } else if is_current {
                                        ui.painter().rect_filled(
                                            handle_rect,
                                            4.0,
                                            egui::Color32::from_white_alpha(22),
                                        );
                                    }
                                    if handle_response.hovered() && !is_dragged {
                                        ui.painter().rect_filled(
                                            handle_rect,
                                            4.0,
                                            egui::Color32::from_white_alpha(13),
                                        );
                                    }

                                    let hx = handle_rect.left() + 12.0;
                                    let hy = handle_rect.center().y;
                                    let line_color = if is_dragged {
                                        egui::Color32::from_rgb(255, 200, 80)
                                    } else {
                                        egui::Color32::from_rgb(140, 110, 45)
                                    };
                                    for dy in [-4.0, 0.0, 4.0] {
                                        ui.painter().line_segment(
                                            [
                                                egui::pos2(hx - 5.0, hy + dy),
                                                egui::pos2(hx + 5.0, hy + dy),
                                            ],
                                            egui::Stroke::new(1.5, line_color),
                                        );
                                    }

                                    let color = if is_dragged {
                                        egui::Color32::from_rgb(255, 200, 80)
                                    } else if is_current {
                                        egui::Color32::from_rgb(255, 210, 80)
                                    } else {
                                        ui.visuals().text_color()
                                    };

                                    let font = if is_current {
                                        egui::FontId::new(14.0, egui::FontFamily::Proportional)
                                    } else {
                                        egui::FontId::new(13.0, egui::FontFamily::Proportional)
                                    };

                                    ui.painter().text(
                                        egui::pos2(
                                            handle_rect.left() + drag_handle_width + 8.0,
                                            handle_rect.center().y,
                                        ),
                                        egui::Align2::LEFT_CENTER,
                                        &name,
                                        font,
                                        color,
                                    );

                                    let del_rect = egui::Rect::from_min_size(
                                        egui::pos2(handle_rect.right() - delete_btn_width, handle_rect.top()),
                                        egui::vec2(delete_btn_width, row_height),
                                    );
                                    let del_resp = ui.interact(del_rect, ui.id().with(("del", i)), egui::Sense::click());
                                    if del_resp.clicked() {
                                        remove_index = Some(i);
                                    }
                                    if handle_response.hovered() || del_resp.hovered() {
                                        let del_color = if del_resp.hovered() {
                                            egui::Color32::from_rgb(255, 80, 80)
                                        } else {
                                            egui::Color32::from_gray(100)
                                        };
                                        let dc = del_rect.center();
                                        let ds = 4.0;
                                        ui.painter().line_segment([egui::pos2(dc.x - ds, dc.y - ds), egui::pos2(dc.x + ds, dc.y + ds)], egui::Stroke::new(1.5, del_color));
                                        ui.painter().line_segment([egui::pos2(dc.x + ds, dc.y - ds), egui::pos2(dc.x - ds, dc.y + ds)], egui::Stroke::new(1.5, del_color));
                                    }
                                }

                                if handle_response.drag_started() {
                                    self.drag_index = Some(i);
                                }
                                if handle_response.clicked() {
                                    let clicked_in_del = ui.input(|i| i.pointer.interact_pos())
                                        .map(|p| p.x > handle_rect.right() - delete_btn_width)
                                        .unwrap_or(false);
                                    if !clicked_in_del {
                                        match self.audio.play_song(song) {
                                            Ok(_) => self.error_message = None,
                                            Err(e) => self.error_message = Some(e),
                                        }
                                    }
                                }
                            }

                            if let Some(idx) = remove_index {
                                let path = self.playlist.remove(idx);
                                let is_current = self.audio.current_file() == Some(&path);
                                if is_current {
                                    self.audio.unload();
                                    self.seek_position = 0.0;
                                }
                                let _ = std::fs::remove_file(&path);
                                self.save_playlist();
                            }

                            if let Some(drag_from) = self.drag_index {
                                if !ui.input(|i| i.pointer.any_down()) {
                                    if let Some(pointer) =
                                        ui.input(|i| i.pointer.hover_pos())
                                    {
                                        let drop_to = row_rects
                                            .iter()
                                            .position(|r| r.contains(pointer))
                                            .unwrap_or(drag_from);
                                        if drag_from != drop_to {
                                            let item = self.playlist.remove(drag_from);
                                            self.playlist.insert(drop_to, item);
                                            self.save_playlist();
                                        }
                                    }
                                    self.drag_index = None;
                                }
                            }
                        }
                    });
                }

                if let Some(error) = &self.error_message {
                    ui.add_space(8.0);
                    ui.label(
                        egui::RichText::new(error)
                            .size(12.0)
                            .color(egui::Color32::from_rgb(255, 100, 100)),
                    );
                }
            });
        });
    }
}
