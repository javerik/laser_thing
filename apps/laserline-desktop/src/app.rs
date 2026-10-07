//! Application shell: live view window, the dockable / detachable control panel, the
//! slicer window, keyboard shortcuts and persistence.

use crate::native_files::{self, NativeFiles};
use eframe::egui::{self, Color32, Key, ViewportBuilder, ViewportClass, ViewportCommand, ViewportId};
use laserline_egui::LaserUi;
use laserline_egui::core::params::LaserParams;
use laserline_egui::core::scan::SliceSettings;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Where the control panel currently lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DockState {
    /// Side panel inside the live view window.
    Docked,
    /// Separate OS window.
    Detached,
    /// Not shown at all (pure live view).
    Hidden,
}

pub struct LaserApp {
    pub ui: LaserUi,
    pub dock: DockState,
    /// State to return to when un-hiding.
    pub dock_before_hide: DockState,
    pub fullscreen: bool,
    files: NativeFiles,
    last_frame: Instant,
    hidden_at: Option<Instant>,
}

const KEY_PARAMS: &str = "params";
const KEY_DOCK: &str = "dock";
const KEY_SLICE: &str = "slice_settings";
const KEY_SLICE_AUTO: &str = "slice_auto";
const CONTROLS_VIEWPORT: &str = "laser_controls";
const SLICER_VIEWPORT: &str = "object_slicer";

impl LaserApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, String> {
        let rs = cc
            .wgpu_render_state
            .as_ref()
            .ok_or("wgpu renderer not available")?;
        LaserUi::install_gpu(rs);

        let (params, dock, slice, auto) = match cc.storage {
            Some(s) => (
                eframe::get_value(s, KEY_PARAMS).unwrap_or_default(),
                eframe::get_value(s, KEY_DOCK).unwrap_or(DockState::Docked),
                eframe::get_value(s, KEY_SLICE).unwrap_or_default(),
                eframe::get_value(s, KEY_SLICE_AUTO).unwrap_or(true),
            ),
            None => (LaserParams::default(), DockState::Docked, SliceSettings::default(), true),
        };
        // Never start with invisible controls.
        let dock = if dock == DockState::Hidden { DockState::Docked } else { dock };

        Ok(Self {
            ui: LaserUi::new(params, slice, auto, rs.target_format.is_srgb()),
            dock,
            dock_before_hide: DockState::Docked,
            fullscreen: false,
            files: NativeFiles,
            last_frame: Instant::now(),
            hidden_at: None,
        })
    }

    /// Opens an `.stl` model (into the slicer) or a `.lscan` scan given on the command line.
    pub fn open_path(&mut self, path: &std::path::Path) {
        match native_files::read(path).and_then(|f| self.ui.open_file(f)) {
            Ok(()) => self.ui.set_status(format!("Opened {}", path.display())),
            Err(e) => self.ui.set_status(e),
        }
    }

    pub fn toggle_detached(&mut self) {
        self.dock = match self.dock {
            DockState::Docked => DockState::Detached,
            DockState::Detached | DockState::Hidden => DockState::Docked,
        };
    }

    pub fn toggle_hidden(&mut self) {
        if self.dock == DockState::Hidden {
            self.dock = self.dock_before_hide;
            self.hidden_at = None;
        } else {
            self.dock_before_hide = self.dock;
            self.dock = DockState::Hidden;
            self.hidden_at = Some(Instant::now());
        }
    }

    /// Works from any viewport; fullscreen always applies to the live view window.
    pub fn set_fullscreen(&self, ctx: &egui::Context, on: bool) {
        ctx.send_viewport_cmd_to(ViewportId::ROOT, ViewportCommand::Fullscreen(on));
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context, is_root: bool) {
        if ctx.wants_keyboard_input() {
            return;
        }
        let pressed: Vec<Key> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Key { key, pressed: true, repeat: false, .. } => Some(*key),
                    _ => None,
                })
                .collect()
        });
        for key in pressed {
            match key {
                Key::F11 => self.set_fullscreen(ctx, !self.fullscreen),
                Key::Escape if self.fullscreen => self.set_fullscreen(ctx, false),
                Key::D => self.toggle_detached(),
                // Only from the live window, so the panel can't hide itself away from its own window.
                Key::H if is_root => self.toggle_hidden(),
                key => {
                    self.ui.handle_key(key);
                }
            }
        }
    }

    fn show_detached(&mut self, ctx: &egui::Context) {
        let builder = ViewportBuilder::default()
            .with_title("Laser Controls")
            .with_app_id(format!("{}-controls", crate::APP_ID))
            .with_inner_size([400.0, 760.0])
            .with_min_inner_size([300.0, 300.0]);
        ctx.show_viewport_immediate(ViewportId::from_hash_of(CONTROLS_VIEWPORT), builder, |ctx, class| {
            if class == ViewportClass::Embedded {
                // Backend can't open more windows: fall back to a floating egui window.
                egui::Window::new("Laser Controls")
                    .default_width(380.0)
                    .show(ctx, |ui| self.controls_scroll(ui));
                return;
            }
            self.handle_shortcuts(ctx, false);
            egui::CentralPanel::default().show(ctx, |ui| self.controls_scroll(ui));
            if ctx.input(|i| i.viewport().close_requested()) {
                // Closing the controls window docks the panel instead of hiding it for good.
                self.dock = DockState::Docked;
            }
        });
    }

    fn show_slicer(&mut self, ctx: &egui::Context) {
        let builder = ViewportBuilder::default()
            .with_title("Object Slicer")
            .with_app_id(format!("{}-slicer", crate::APP_ID))
            .with_inner_size([1200.0, 820.0])
            .with_min_inner_size([640.0, 480.0]);
        ctx.show_viewport_immediate(ViewportId::from_hash_of(SLICER_VIEWPORT), builder, |ctx, class| {
            if class == ViewportClass::Embedded {
                let mut open = true;
                egui::Window::new("Object Slicer")
                    .open(&mut open)
                    .default_size([1000.0, 700.0])
                    .show(ctx, |ui| self.ui.slicer_panels(ui, &mut self.files));
                self.ui.slicer_open &= open;
                return;
            }
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ctx, |ui| self.ui.slicer_panels(ui, &mut self.files));
            if ctx.input(|i| i.viewport().close_requested()) {
                self.ui.slicer_open = false;
            }
        });
    }

    fn controls_scroll(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                self.header_ui(ui);
                ui.separator();
                self.ui.controls_ui(ui, &mut self.files);
                ui.add_space(4.0);
                ui.label(
                    egui::RichText::new("Keys: F11 fullscreen · Esc exit · D dock/detach · H hide · Space play/pause · R reset time · ←/→ step scan profile · double-click view = fullscreen")
                        .small()
                        .weak(),
                );
            });
    }

    fn header_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.heading("Laser Line");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Hide").on_hover_text("Hide controls (H in live view)").clicked() {
                    self.toggle_hidden();
                }
                let fs = if self.fullscreen { "Windowed" } else { "Fullscreen" };
                if ui.button(fs).on_hover_text("Toggle live view fullscreen (F11)").clicked() {
                    self.set_fullscreen(ui.ctx(), !self.fullscreen);
                }
                let (label, tip) = match self.dock {
                    DockState::Docked => ("⏏ Detach", "Move controls into their own window (D)"),
                    _ => ("Dock", "Dock controls into the live view (D)"),
                };
                if ui.button(label).on_hover_text(tip).clicked() {
                    self.toggle_detached();
                }
            });
        });
    }

    fn live_view(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let response = ui.allocate_rect(rect, egui::Sense::click());
        self.ui.live_view(ui, rect);

        if response.double_clicked() {
            self.set_fullscreen(ui.ctx(), !self.fullscreen);
        }
        if self.fullscreen && self.dock != DockState::Docked && response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::None);
        }

        // Brief hint after hiding the controls, then a clean picture.
        if let Some(t) = self.hidden_at {
            let age = t.elapsed();
            if age < Duration::from_secs(3) {
                ui.painter().text(
                    rect.center_bottom() - egui::vec2(0.0, 24.0),
                    egui::Align2::CENTER_BOTTOM,
                    "Controls hidden — press H to show",
                    egui::FontId::proportional(16.0),
                    Color32::from_gray(160),
                );
            } else {
                self.hidden_at = None;
            }
        }
    }
}

impl eframe::App for LaserApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f64().min(0.25);
        self.last_frame = now;
        self.fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));

        self.handle_shortcuts(ctx, true);
        self.ui.begin_frame(ctx, dt);
        if self.ui.slicer_open {
            self.show_slicer(ctx);
        }

        match self.dock {
            DockState::Docked => {
                egui::SidePanel::right("controls_panel")
                    .resizable(true)
                    .default_width(360.0)
                    .min_width(280.0)
                    .show(ctx, |ui| self.controls_scroll(ui));
            }
            DockState::Detached => self.show_detached(ctx),
            DockState::Hidden => {}
        }

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE.fill(Color32::BLACK))
            .show(ctx, |ui| self.live_view(ui));

        // Live emulator: animate continuously (vsync-limited).
        ctx.request_repaint();
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, KEY_PARAMS, &self.ui.emu.params);
        eframe::set_value(storage, KEY_DOCK, &self.dock);
        eframe::set_value(storage, KEY_SLICE, &self.ui.slicer.settings);
        eframe::set_value(storage, KEY_SLICE_AUTO, &self.ui.slicer.auto);
    }
}
