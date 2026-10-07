//! Application shell: the main window, the dockable / detachable live view and control
//! panel, the slicer window, keyboard shortcuts and persistence.

use crate::native_files::{self, NativeFiles};
use eframe::egui::{self, Color32, Key, ViewportBuilder, ViewportCommand, ViewportId};
use laserline_egui::{LaserUi, show_window};
use laserline_egui::core::params::LaserParams;
use laserline_egui::core::scan::SliceSettings;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// Where the control panel currently lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum DockState {
    /// In the main window: a side panel next to the live view, or the whole window
    /// while the live view is detached.
    Docked,
    /// Separate OS window.
    Detached,
    /// Not shown at all (pure live view).
    Hidden,
}

/// Where the live laser view currently lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewPlacement {
    /// Fills the main window (next to the docked controls).
    Docked,
    /// Separate OS window, e.g. fullscreen on a projector.
    Detached,
}

pub struct LaserApp {
    pub ui: LaserUi,
    pub dock: DockState,
    pub view: ViewPlacement,
    /// State to return to when un-hiding.
    pub dock_before_hide: DockState,
    pub fullscreen: bool,
    files: NativeFiles,
    last_frame: Instant,
    hidden_at: Option<Instant>,
}

const KEY_PARAMS: &str = "params";
const KEY_DOCK: &str = "dock";
const KEY_VIEW: &str = "view_placement";
const KEY_SLICE: &str = "slice_settings";
const KEY_SLICE_AUTO: &str = "slice_auto";
const CONTROLS_VIEWPORT: &str = "laser_controls";
const SLICER_VIEWPORT: &str = "object_slicer";
const LIVE_VIEWPORT: &str = "laser_live_view";

impl LaserApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, String> {
        let rs = cc
            .wgpu_render_state
            .as_ref()
            .ok_or("wgpu renderer not available")?;
        LaserUi::install_gpu(rs);

        let (params, dock, view, slice, auto) = match cc.storage {
            Some(s) => (
                eframe::get_value(s, KEY_PARAMS).unwrap_or_default(),
                eframe::get_value(s, KEY_DOCK).unwrap_or(DockState::Docked),
                eframe::get_value(s, KEY_VIEW).unwrap_or(ViewPlacement::Docked),
                eframe::get_value(s, KEY_SLICE).unwrap_or_default(),
                eframe::get_value(s, KEY_SLICE_AUTO).unwrap_or(true),
            ),
            None => (LaserParams::default(), DockState::Docked, ViewPlacement::Docked, SliceSettings::default(), true),
        };
        // Never start with invisible controls.
        let dock = if dock == DockState::Hidden { DockState::Docked } else { dock };

        Ok(Self {
            ui: LaserUi::new(params, slice, auto, rs.target_format.is_srgb()),
            dock,
            view,
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

    pub fn toggle_view_detached(&mut self, ctx: &egui::Context) {
        self.view = match self.view {
            ViewPlacement::Docked => {
                // The main window keeps only the controls: leave fullscreen there.
                if self.fullscreen {
                    self.set_fullscreen(ctx, false);
                    self.fullscreen = false;
                }
                ViewPlacement::Detached
            }
            ViewPlacement::Detached => ViewPlacement::Docked,
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

    /// The window showing the live view.
    fn live_viewport(&self) -> ViewportId {
        match self.view {
            ViewPlacement::Docked => ViewportId::ROOT,
            ViewPlacement::Detached => ViewportId::from_hash_of(LIVE_VIEWPORT),
        }
    }

    /// Works from any viewport; fullscreen always applies to the live view window.
    pub fn set_fullscreen(&self, ctx: &egui::Context, on: bool) {
        ctx.send_viewport_cmd_to(self.live_viewport(), ViewportCommand::Fullscreen(on));
    }

    /// `in_live_window`: the keys were pressed in the window showing the live view.
    fn handle_shortcuts(&mut self, ctx: &egui::Context, in_live_window: bool) {
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
                Key::V => self.toggle_view_detached(ctx),
                // Only from the live window, so the panel can't hide itself away from its own window.
                Key::H if in_live_window => self.toggle_hidden(),
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
        let id = ViewportId::from_hash_of(CONTROLS_VIEWPORT);
        let frame = egui::Frame::central_panel(&ctx.style());
        let open = show_window(ctx, id, builder, frame, |ui| {
            self.handle_shortcuts(ui.ctx(), false);
            self.controls_scroll(ui);
        });
        if !open {
            // Closing the controls window docks the panel instead of hiding it for good.
            self.dock = DockState::Docked;
        }
    }

    fn show_live_window(&mut self, ctx: &egui::Context) {
        let builder = ViewportBuilder::default()
            .with_title("Laser Line — Live View")
            .with_app_id(format!("{}-live", crate::APP_ID))
            .with_inner_size([1280.0, 720.0])
            .with_min_inner_size([320.0, 240.0]);
        let id = ViewportId::from_hash_of(LIVE_VIEWPORT);
        let open = show_window(ctx, id, builder, egui::Frame::NONE.fill(Color32::BLACK), |ui| {
            self.handle_shortcuts(ui.ctx(), true);
            self.live_view(ui);
        });
        if !open {
            // Closing the live view window docks the view into the main window.
            self.view = ViewPlacement::Docked;
        }
    }

    fn show_slicer(&mut self, ctx: &egui::Context) {
        let builder = ViewportBuilder::default()
            .with_title("Object Slicer")
            .with_app_id(format!("{}-slicer", crate::APP_ID))
            .with_inner_size([1200.0, 820.0])
            .with_min_inner_size([640.0, 480.0]);
        let id = ViewportId::from_hash_of(SLICER_VIEWPORT);
        let open = show_window(ctx, id, builder, egui::Frame::NONE, |ui| self.ui.slicer_panels(ui, &mut self.files));
        self.ui.slicer_open &= open;
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
                    egui::RichText::new("Keys: F11 fullscreen · Esc exit · D dock/detach controls · V dock/detach view · H hide · Space play/pause · R reset time · ←/→ step scan profile · double-click view = fullscreen")
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
            });
        });
        ui.horizontal(|ui| {
            let (label, tip) = match self.view {
                ViewPlacement::Docked => ("⏏ Detach view", "Move the live view into its own window (V)"),
                ViewPlacement::Detached => ("Dock view", "Dock the live view into the main window (V)"),
            };
            if ui.button(label).on_hover_text(tip).clicked() {
                self.toggle_view_detached(ui.ctx());
            }
            let (label, tip) = match self.dock {
                DockState::Docked => ("⏏ Detach controls", "Move the controls into their own window (D)"),
                _ => ("Dock controls", "Dock the controls into the main window (D)"),
            };
            if ui.button(label).on_hover_text(tip).clicked() {
                self.toggle_detached();
            }
        });
    }

    /// Main window content when both the live view and the controls are elsewhere.
    fn placeholder(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() / 2.0 - 40.0).max(0.0));
            ui.label("The live view and the controls are in their own windows.");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Dock view").clicked() {
                    self.toggle_view_detached(ui.ctx());
                }
                let controls = if self.dock == DockState::Hidden { "Show controls" } else { "Dock controls" };
                if ui.button(controls).clicked() {
                    self.dock = DockState::Docked;
                    self.hidden_at = None;
                }
            });
        });
    }

    fn live_view(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let response = ui.allocate_rect(rect, egui::Sense::click());
        self.ui.live_view(ui, rect);
        // The state of whichever window shows the view.
        self.fullscreen = ui.ctx().input(|i| i.viewport().fullscreen.unwrap_or(false));

        if response.double_clicked() {
            self.set_fullscreen(ui.ctx(), !self.fullscreen);
        }
        let alone = self.view == ViewPlacement::Detached || self.dock != DockState::Docked;
        if self.fullscreen && alone && response.hovered() {
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

        self.handle_shortcuts(ctx, self.view == ViewPlacement::Docked);
        self.ui.begin_frame(ctx, dt);
        if self.ui.slicer_open {
            self.show_slicer(ctx);
        }
        if self.view == ViewPlacement::Detached {
            self.show_live_window(ctx);
        }

        let view_docked = self.view == ViewPlacement::Docked;
        match self.dock {
            DockState::Docked if view_docked => {
                egui::SidePanel::right("controls_panel")
                    .resizable(true)
                    .default_width(360.0)
                    .min_width(280.0)
                    .show(ctx, |ui| self.controls_scroll(ui));
            }
            // The live view is elsewhere: the controls get the whole main window.
            DockState::Docked => {
                egui::CentralPanel::default().show(ctx, |ui| self.controls_scroll(ui));
            }
            DockState::Detached => self.show_detached(ctx),
            DockState::Hidden => {}
        }

        if view_docked {
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE.fill(Color32::BLACK))
                .show(ctx, |ui| self.live_view(ui));
        } else if self.dock != DockState::Docked {
            egui::CentralPanel::default().show(ctx, |ui| self.placeholder(ui));
        }

        // Live emulator: animate continuously (vsync-limited).
        ctx.request_repaint();
    }

    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, KEY_PARAMS, &self.ui.emu.params);
        eframe::set_value(storage, KEY_DOCK, &self.dock);
        eframe::set_value(storage, KEY_VIEW, &self.view);
        eframe::set_value(storage, KEY_SLICE, &self.ui.slicer.settings);
        eframe::set_value(storage, KEY_SLICE_AUTO, &self.ui.slicer.auto);
    }
}
