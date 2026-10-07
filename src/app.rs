//! Application state, live view and the dockable / detachable control panel.

use crate::mesh_view::MeshViewResources;
use crate::model;
use crate::params::{DeformKind, LaserParams, LengthUnit, RateMode};
use crate::renderer::{DEFORM_SAMPLES, LaserCallback, LaserResources, Uniforms};
use crate::scan::{self, DisplayMapping, Scan, SliceSettings};
use crate::scan_view::Slicer;
use eframe::egui::{self, Color32, Key, ViewportBuilder, ViewportClass, ViewportCommand, ViewportId};
use eframe::egui_wgpu;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
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

/// Values measured while drawing the live view, shown as readouts in the UI.
#[derive(Clone, Copy, Debug, Default)]
pub struct LiveInfo {
    pub view_px: [f32; 2],
    pub center_px: [f32; 2],
    pub offset_px: f32,
    pub length_px: f32,
    /// View extent along the line direction (basis for % lengths).
    pub extent_along_px: f32,
    /// Effective scan mapping while a scan drives the line.
    pub scan: Option<DisplayMapping>,
}

/// Loaded object scan, its playback position and the slicer that produces it.
pub struct ScanState {
    pub scan: Option<Arc<Scan>>,
    /// Profiles played since the start; mapped to a row by the end mode.
    pub pos: f64,
    pub slicer: Slicer,
}

pub struct LaserApp {
    pub params: LaserParams,
    pub dock: DockState,
    /// State to return to when un-hiding.
    pub dock_before_hide: DockState,
    pub playing: bool,
    pub sim_time: f64,
    pub fps: f32,
    pub fullscreen: bool,
    pub live: LiveInfo,
    pub status: Option<(String, Instant)>,
    pub preset_index: usize,
    pub scan: ScanState,
    last_frame: Instant,
    frame_counter: u32,
    hidden_at: Option<Instant>,
    srgb_target: bool,
}

const KEY_PARAMS: &str = "params";
const KEY_DOCK: &str = "dock";
const KEY_SLICE: &str = "slice_settings";
const KEY_SLICE_AUTO: &str = "slice_auto";
const CONTROLS_VIEWPORT: &str = "laser_controls";

impl LaserApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Result<Self, String> {
        let rs = cc
            .wgpu_render_state
            .as_ref()
            .ok_or("wgpu renderer not available")?;
        rs.renderer
            .write()
            .callback_resources
            .insert(LaserResources::new(&rs.device, rs.target_format));
        rs.renderer
            .write()
            .callback_resources
            .insert(MeshViewResources::new(&rs.device, rs.target_format));

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
            params,
            dock,
            dock_before_hide: DockState::Docked,
            playing: true,
            sim_time: 0.0,
            fps: 0.0,
            fullscreen: false,
            live: LiveInfo::default(),
            status: None,
            preset_index: 0,
            scan: ScanState { scan: None, pos: 0.0, slicer: Slicer::new(slice, auto) },
            last_frame: Instant::now(),
            frame_counter: 0,
            hidden_at: None,
            srgb_target: rs.target_format.is_srgb(),
        })
    }

    /// Opens an `.stl` model (into the slicer) or a `.lscan` scan given on the command line.
    pub fn open_path(&mut self, path: &std::path::Path) {
        let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase();
        let result = match ext.as_str() {
            "stl" => crate::mesh::load_stl(path).map(|mesh| {
                self.scan.slicer.set_model(mesh, path.to_owned());
                self.scan.slicer.open = true;
            }),
            "lscan" => std::fs::read(path)
                .map_err(|e| e.to_string())
                .and_then(|b| crate::scan_file::from_bytes(&b))
                .map(|scan| self.adopt_scan(scan)),
            _ => Err("expected an .stl or .lscan file".into()),
        };
        match result {
            Ok(()) => self.set_status(format!("Opened {}", path.display())),
            Err(e) => self.set_status(format!("{}: {e}", path.display())),
        }
    }

    pub fn set_status(&mut self, msg: impl Into<String>) {
        self.status = Some((msg.into(), Instant::now()));
    }

    pub fn srgb_target(&self) -> bool {
        self.srgb_target
    }

    /// True when the loaded scan shapes the live line.
    pub fn scan_drives_line(&self) -> bool {
        self.params.scan.enabled && self.scan.scan.is_some()
    }

    /// Profile currently shown (0 without a scan).
    pub fn current_row(&self) -> usize {
        self.scan.scan.as_ref().map_or(0, |s| scan::row_index(self.scan.pos, s.meta.rows, self.params.scan.end))
    }

    pub fn step_scan(&mut self, delta: isize) {
        if let Some(s) = &self.scan.scan {
            self.scan.pos = scan::step(self.scan.pos, s.meta.rows, self.params.scan.end, delta);
        }
    }

    pub fn reset_time(&mut self) {
        self.sim_time = 0.0;
        self.scan.pos = 0.0;
    }

    pub fn set_scan(&mut self, scan: Scan) {
        self.scan.scan = Some(Arc::new(scan));
    }

    pub fn save_scan(&mut self) {
        let Some(scan) = &self.scan.scan else { return };
        match crate::scan_file::save_dialog(scan) {
            Ok(Some(path)) => self.set_status(format!("Saved {}", path.display())),
            Ok(None) => {}
            Err(e) => self.set_status(format!("Save failed: {e}")),
        }
    }

    pub fn load_scan(&mut self) {
        match crate::scan_file::load_dialog() {
            Ok(Some((scan, path))) => {
                self.adopt_scan(scan);
                self.set_status(format!("Loaded {}", path.display()));
            }
            Ok(None) => {}
            Err(e) => self.set_status(format!("Load failed: {e}")),
        }
    }

    /// Plays a scan loaded from a file. It replaces the model so the previews match what
    /// plays back, and its settings are shown (and reused if the model is imported again).
    fn adopt_scan(&mut self, scan: Scan) {
        self.scan.slicer.model = None;
        self.scan.slicer.settings = scan.meta.settings.clone();
        self.scan.pos = 0.0;
        self.set_scan(scan);
    }

    /// Advances scan playback by one rendered frame of `dt` seconds.
    fn advance_scan(&mut self, dt: f64) {
        if !self.playing || !self.scan_drives_line() {
            return;
        }
        let p = &self.params;
        self.scan.pos += match p.scan.rate {
            RateMode::PerFrame => 1.0,
            RateMode::PerSecond => dt * p.time_scale as f64 * p.scan.profiles_per_s.max(0.0) as f64,
        };
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
                Key::Space => self.playing = !self.playing,
                Key::R => self.reset_time(),
                Key::ArrowLeft => self.step_scan(-1),
                Key::ArrowRight => self.step_scan(1),
                _ => {}
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

    fn controls_scroll(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| self.controls_ui(ui));
    }

    fn live_view(&mut self, ui: &mut egui::Ui) {
        let rect = ui.max_rect();
        let response = ui.allocate_rect(rect, egui::Sense::click());
        let ppp = ui.ctx().pixels_per_point();
        let p = &self.params;

        let origin = [rect.min.x * ppp, rect.min.y * ppp];
        let size = [rect.width() * ppp, rect.height() * ppp];
        let angle = model::line_angle(&p.line);
        let dir = [angle.cos(), angle.sin()];
        let normal = [-dir[1], dir[0]];

        let extent_along = model::extent_along(size, dir);
        let mut length_px = match p.line.length_unit {
            LengthUnit::Pixels => p.line.length,
            LengthUnit::Percent => p.line.length / 100.0 * extent_along,
        }
        .max(0.0);

        // A driving scan sets the scale along the line; in fixed mode also the length.
        let active_scan = self.scan.scan.as_ref().filter(|_| p.scan.enabled);
        let mapping = active_scan.map(|s| {
            let (mapping, length) = scan::display_mapping(s, &p.scan, length_px);
            length_px = length;
            mapping
        });
        let offset = model::motion_offset(&p.motion, self.sim_time, model::extent_along(size, normal));
        let center = [
            origin[0] + p.line.position_pct[0] / 100.0 * size[0] + normal[0] * offset,
            origin[1] + p.line.position_pct[1] / 100.0 * size[1] + normal[1] * offset,
        ];

        // Sample the deformation over the visible line plus the soft end caps.
        let span = length_px + p.profile.end_softness_px + 4.0;
        let start = -0.5 * span;
        let step = (span / (DEFORM_SAMPLES - 1) as f32).max(1e-3);
        let (deform, deform_start, deform_step) = if let (Some(s), Some(map)) = (active_scan, &mapping) {
            let mut samples = vec![[0.0, 1.0]; DEFORM_SAMPLES];
            let row = scan::row_index(self.scan.pos, s.meta.rows, p.scan.end);
            scan::fill_deform(s, row, map, start, step, &mut samples);
            (Some(samples), start, step)
        } else if p.deform.kind == DeformKind::None {
            (None, 0.0, 1.0)
        } else {
            let mut samples = vec![0.0; DEFORM_SAMPLES];
            model::sample_deformation(&p.deform, self.sim_time, start, step, &mut samples);
            (Some(samples.into_iter().map(|d| [d, 1.0]).collect()), start, step)
        };

        let rgb = model::laser_rgb(&p.color);
        let seed = if p.noise.animated { self.frame_counter } else { 1 };
        let uniforms = Uniforms {
            center,
            dir,
            length: length_px,
            width: p.line.width_px.max(0.0),
            intensity: p.line.intensity_pct / 100.0,
            end_soft: p.profile.end_softness_px.max(0.0),
            color: [rgb[0], rgb[1], rgb[2], 1.0],
            profile: p.profile.kind as u32,
            edge_soft: p.profile.edge_softness_px,
            deform_start,
            deform_step,
            ambient: p.noise.ambient_pct / 100.0,
            speckle: p.noise.speckle_pct / 100.0,
            grain: p.noise.grain_px,
            noise: p.noise.noise_pct / 100.0,
            seed,
            deform_count: if deform.is_some() { DEFORM_SAMPLES as u32 } else { 0 },
            srgb_out: self.srgb_target as u32,
            _pad: 0,
        };
        ui.painter().add(egui_wgpu::Callback::new_paint_callback(
            rect,
            LaserCallback { uniforms, deform },
        ));

        self.live = LiveInfo {
            view_px: size,
            center_px: [center[0] - origin[0], center[1] - origin[1]],
            offset_px: offset,
            length_px,
            extent_along_px: extent_along,
            scan: mapping,
        };

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
        if self.playing {
            self.sim_time += dt * self.params.time_scale as f64;
        }
        if dt > 0.0 {
            self.fps = 0.9 * self.fps + 0.1 * (1.0 / dt) as f32;
        }
        self.frame_counter = self.frame_counter.wrapping_add(1);
        self.fullscreen = ctx.input(|i| i.viewport().fullscreen.unwrap_or(false));

        self.handle_shortcuts(ctx, true);
        if let Some(scan) = self.scan.slicer.poll() {
            self.set_scan(scan);
        }
        self.advance_scan(dt);
        if self.scan.slicer.open {
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
        eframe::set_value(storage, KEY_PARAMS, &self.params);
        eframe::set_value(storage, KEY_DOCK, &self.dock);
        eframe::set_value(storage, KEY_SLICE, &self.scan.slicer.settings);
        eframe::set_value(storage, KEY_SLICE_AUTO, &self.scan.slicer.auto);
    }
}
