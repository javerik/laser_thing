//! Control panel widgets. The same UI is used docked and detached.

use crate::app::{DockState, LaserApp};
use crate::model;
use crate::params::{
    ColorMode, ColorParams, DeformKind, DeformParams, EndMode, LaserParams, LengthUnit, LineParams,
    MotionKind, MotionParams, NoiseParams, Orientation, ProfileKind, ProfileParams, RateMode,
    HeightGain, ScanParams, ScanScale, SweepMode,
};
use crate::presets;
use crate::scan_view::{profile_plot, scan_summary};
use eframe::egui::{self, Color32, RichText, Slider, Stroke, Ui};
use std::time::Duration;

const WAVELENGTH_PRESETS: [f32; 5] = [405.0, 450.0, 520.0, 635.0, 660.0];

impl LaserApp {
    pub fn controls_ui(&mut self, ui: &mut Ui) {
        ui.spacing_mut().slider_width = 170.0;
        self.header_ui(ui);
        ui.separator();
        self.transport_ui(ui);
        ui.separator();

        let extent = self.live.extent_along_px;
        let length_px = self.live.length_px;
        let p = &mut self.params;
        section(ui, "Line", true, &mut p.line, |ui, l| line_ui(ui, l, extent));
        let width = p.line.width_px;
        section(ui, "Profile", true, &mut p.profile, |ui, pr| profile_ui(ui, pr, width));
        section(ui, "Color", false, &mut p.color, color_ui);
        let offset = self.live.offset_px;
        section(ui, "Movement", true, &mut p.motion, |ui, m| motion_ui(ui, m, offset));
        let t = self.sim_time;
        let scan_drives = self.params.scan.enabled && self.scan.scan.is_some();
        let p = &mut self.params;
        section(ui, "Deformation", false, &mut p.deform, |ui, d| {
            if scan_drives {
                ui.label(RichText::new("The object scan shapes the line; this deformation is inactive.").weak());
            }
            deform_ui(ui, d, length_px, t)
        });
        section(ui, "Background & noise", false, &mut p.noise, noise_ui);

        egui::CollapsingHeader::new(RichText::new("Object scan").strong())
            .default_open(true)
            .show(ui, |ui| self.object_scan_ui(ui));

        egui::CollapsingHeader::new(RichText::new("Presets").strong())
            .default_open(true)
            .show(ui, |ui| self.presets_ui(ui));

        ui.separator();
        self.readout_ui(ui);
    }

    fn header_ui(&mut self, ui: &mut Ui) {
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

    fn transport_ui(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let play = if self.playing { "⏸ Pause" } else { "▶ Play" };
            if ui.add_sized([80.0, 22.0], egui::Button::new(play)).on_hover_text("Space").clicked() {
                self.playing = !self.playing;
            }
            if ui.button("⟲ Reset time").on_hover_text("R (also rewinds the object scan)").clicked() {
                self.reset_time();
            }
            ui.label(format!("t = {:.2} s", self.sim_time));
        });
        ui.horizontal(|ui| {
            ui.label("Speed");
            ui.add(
                Slider::new(&mut self.params.time_scale, 0.05..=10.0)
                    .logarithmic(true)
                    .suffix("×"),
            );
            if ui.small_button("1×").clicked() {
                self.params.time_scale = 1.0;
            }
        });
    }

    fn presets_ui(&mut self, ui: &mut Ui) {
        let builtin = presets::builtin();
        ui.horizontal(|ui| {
            self.preset_index = self.preset_index.min(builtin.len() - 1);
            egui::ComboBox::from_id_salt("builtin_presets")
                .width(190.0)
                .selected_text(builtin[self.preset_index].0)
                .show_ui(ui, |ui| {
                    for (i, (name, _)) in builtin.iter().enumerate() {
                        ui.selectable_value(&mut self.preset_index, i, *name);
                    }
                });
            if ui.button("Apply").clicked() {
                let (name, p) = &builtin[self.preset_index];
                self.params = p.clone();
                self.sim_time = 0.0;
                self.set_status(format!("Applied “{name}”"));
            }
        });
        ui.horizontal(|ui| {
            if ui.button("💾 Save…").clicked() {
                match presets::save_dialog(&self.params) {
                    Ok(Some(path)) => self.set_status(format!("Saved {}", path.display())),
                    Ok(None) => {}
                    Err(e) => self.set_status(format!("Save failed: {e}")),
                }
            }
            if ui.button("📂 Load…").clicked() {
                match presets::load_dialog() {
                    Ok(Some((p, path))) => {
                        self.params = p;
                        self.set_status(format!("Loaded {}", path.display()));
                    }
                    Ok(None) => {}
                    Err(e) => self.set_status(format!("Load failed: {e}")),
                }
            }
            if ui.button("Reset all").clicked() {
                self.params = LaserParams::default();
                self.set_status("All parameters reset");
            }
        });
        if let Some((msg, at)) = &self.status
            && at.elapsed() < Duration::from_secs(5)
        {
            ui.label(RichText::new(msg).weak());
        }
    }

    fn object_scan_ui(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            if ui.button("📂 Import STL…").clicked() {
                self.import_stl();
            }
            let open = self.scan.slicer.open;
            if ui.selectable_label(open, "🔧 Slicer").on_hover_text("Slice settings and 3D preview").clicked() {
                self.scan.slicer.open = !open;
            }
            if ui.button("Load scan…").clicked() {
                self.load_scan();
            }
            if ui.add_enabled(self.scan.scan.is_some(), egui::Button::new("Save scan…")).clicked() {
                self.save_scan();
            }
        });
        if self.scan.slicer.is_busy() {
            ui.add(egui::Spinner::new());
        }
        let Some(scan) = self.scan.scan.clone() else {
            ui.label(RichText::new("No scan loaded. Import an STL model or load a .lscan file.").weak());
            return;
        };
        scan_summary(ui, &scan);
        ui.checkbox(&mut self.params.scan.enabled, "Drive the line from the scan");

        let row = self.current_row();
        let live = self.live.scan;
        let angle = scan.meta.settings.triangulation_deg;
        scan_params_ui(ui, &mut self.params.scan, live, angle);

        ui.add_space(4.0);
        let rows = scan.meta.rows;
        ui.horizontal(|ui| {
            if ui.small_button("◀").on_hover_text("Previous profile (←)").clicked() {
                self.step_scan(-1);
            }
            if ui.small_button("▶").on_hover_text("Next profile (→)").clicked() {
                self.step_scan(1);
            }
            let mut r = row;
            if ui.add(Slider::new(&mut r, 0..=rows - 1).text("profile")).changed() {
                self.scan.pos = r as f64;
            }
        });
        profile_plot(ui, &scan, row, 56.0);
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("↺ Reset").on_hover_text("Reset scan display & playback to defaults").clicked() {
                    self.params.scan = ScanParams::default();
                }
            });
        });
    }

    fn readout_ui(&self, ui: &mut Ui) {
        let l = &self.live;
        egui::Grid::new("readout").num_columns(2).show(ui, |ui| {
            ui.label("View");
            ui.label(format!("{:.0} × {:.0} px", l.view_px[0], l.view_px[1]));
            ui.end_row();
            ui.label("Center");
            ui.label(format!("{:.1}, {:.1} px", l.center_px[0], l.center_px[1]));
            ui.end_row();
            ui.label("Length");
            ui.label(format!("{:.0} px", l.length_px));
            ui.end_row();
            ui.label("Frame rate");
            ui.label(format!("{:.0} fps", self.fps));
            ui.end_row();
        });
        ui.add_space(4.0);
        ui.label(
            RichText::new("Keys: F11 fullscreen · Esc exit · D dock/detach · H hide · Space play/pause · R reset time · ←/→ step scan profile · double-click view = fullscreen")
                .small()
                .weak(),
        );
    }
}

/// Collapsible section with a "Reset" button restoring the section defaults.
pub(crate) fn section<T: Default>(ui: &mut Ui, title: &str, open: bool, value: &mut T, body: impl FnOnce(&mut Ui, &mut T)) {
    egui::CollapsingHeader::new(RichText::new(title).strong())
        .default_open(open)
        .show(ui, |ui| {
            body(ui, value);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("↺ Reset").on_hover_text(format!("Reset {title} to defaults")).clicked() {
                        *value = T::default();
                    }
                });
            });
        });
}

pub(crate) fn grid(ui: &mut Ui, id: &str, body: impl FnOnce(&mut Ui)) {
    egui::Grid::new(id).num_columns(2).spacing([10.0, 6.0]).show(ui, body);
}

pub(crate) fn row(ui: &mut Ui, label: &str, widget: impl FnOnce(&mut Ui)) {
    ui.label(label);
    widget(ui);
    ui.end_row();
}

fn line_ui(ui: &mut Ui, l: &mut LineParams, extent_along: f32) {
    grid(ui, "line_grid", |ui| {
        row(ui, "Intensity", |ui| {
            ui.add(Slider::new(&mut l.intensity_pct, 0.0..=100.0).suffix(" %"))
                .on_hover_text(format!("Peak pixel value ≈ {:.0} / 255", l.intensity_pct / 100.0 * 255.0));
        });
        row(ui, "Width", |ui| {
            ui.add(
                Slider::new(&mut l.width_px, 0.2..=200.0)
                    .logarithmic(true)
                    .suffix(" px")
                    .max_decimals(2),
            )
            .on_hover_text("Full width at half maximum for Gaussian");
        });
        row(ui, "Length", |ui| {
            ui.vertical(|ui| {
                let range = match l.length_unit {
                    LengthUnit::Pixels => 0.0..=8000.0,
                    LengthUnit::Percent => 0.0..=150.0,
                };
                let suffix = if l.length_unit == LengthUnit::Pixels { " px" } else { " %" };
                ui.add(Slider::new(&mut l.length, range).suffix(suffix));
                ui.horizontal(|ui| {
                    let before = l.length_unit;
                    ui.selectable_value(&mut l.length_unit, LengthUnit::Percent, "% of view");
                    ui.selectable_value(&mut l.length_unit, LengthUnit::Pixels, "pixels");
                    // Keep the visible length when switching units.
                    if before != l.length_unit && extent_along > 0.0 {
                        l.length = match l.length_unit {
                            LengthUnit::Pixels => (l.length / 100.0 * extent_along).round(),
                            LengthUnit::Percent => l.length / extent_along * 100.0,
                        };
                    }
                });
            });
        });
        row(ui, "Position X", |ui| {
            ui.add(Slider::new(&mut l.position_pct[0], 0.0..=100.0).suffix(" %"));
        });
        row(ui, "Position Y", |ui| {
            ui.add(Slider::new(&mut l.position_pct[1], 0.0..=100.0).suffix(" %"));
        });
        row(ui, "", |ui| {
            if ui.small_button("Center").clicked() {
                l.position_pct = [50.0, 50.0];
            }
        });
        row(ui, "Orientation", |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut l.orientation, Orientation::Horizontal, "— Horizontal");
                ui.selectable_value(&mut l.orientation, Orientation::Vertical, "| Vertical");
            });
        });
        row(ui, "Tilt", |ui| {
            ui.add(
                Slider::new(&mut l.tilt_deg, -45.0..=45.0)
                    .suffix("°")
                    .clamping(egui::SliderClamping::Never),
            );
        });
    });
}

fn profile_ui(ui: &mut Ui, p: &mut ProfileParams, width: f32) {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut p.kind, ProfileKind::Gaussian, "Gaussian");
        ui.selectable_value(&mut p.kind, ProfileKind::TopHat, "Top-hat");
        ui.selectable_value(&mut p.kind, ProfileKind::Hard, "Hard edge");
    });
    grid(ui, "profile_grid", |ui| {
        if p.kind == ProfileKind::TopHat {
            row(ui, "Edge softness", |ui| {
                ui.add(Slider::new(&mut p.edge_softness_px, 0.0..=50.0).suffix(" px"));
            });
        }
        row(ui, "End fade", |ui| {
            ui.add(Slider::new(&mut p.end_softness_px, 0.0..=300.0).suffix(" px"))
                .on_hover_text("Fade-out length at both line ends (0 = hard ends)");
        });
    });
    // Cross-section preview over ±1.5 × the visible width.
    let half = 1.5 * (width + p.edge_softness_px * (p.kind == ProfileKind::TopHat) as u8 as f32).max(1.0);
    plot(ui, -half, half, 0.0, 1.0, |v| model::profile_intensity(p, width, v), "cross-section");
}

fn color_ui(ui: &mut Ui, c: &mut ColorParams) {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut c.mode, ColorMode::Wavelength, "Wavelength");
        ui.selectable_value(&mut c.mode, ColorMode::Custom, "Custom RGB");
        let [r, g, b] = model::laser_rgb(c);
        let (rect, _) = ui.allocate_exact_size(egui::vec2(36.0, 18.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 3.0, egui::Rgba::from_rgb(r, g, b));
    });
    match c.mode {
        ColorMode::Wavelength => {
            ui.add(Slider::new(&mut c.wavelength_nm, 380.0..=780.0).suffix(" nm"));
            ui.horizontal(|ui| {
                for nm in WAVELENGTH_PRESETS {
                    let [r, g, b] = model::wavelength_to_rgb(nm);
                    let text = RichText::new(format!("{nm:.0}")).color(egui::Rgba::from_rgb(r, g, b));
                    if ui.selectable_label(c.wavelength_nm == nm, text).clicked() {
                        c.wavelength_nm = nm;
                    }
                }
            });
        }
        ColorMode::Custom => {
            ui.horizontal(|ui| {
                ui.color_edit_button_rgb(&mut c.custom_rgb);
                for (name, rgb) in [("White", [1.0; 3]), ("Red", [1.0, 0.0, 0.0]), ("Green", [0.0, 1.0, 0.0]), ("Blue", [0.0, 0.0, 1.0])] {
                    if ui.small_button(name).clicked() {
                        c.custom_rgb = rgb;
                    }
                }
            });
        }
    }
}

fn motion_ui(ui: &mut Ui, m: &mut MotionParams, offset: f32) {
    ui.horizontal(|ui| {
        ui.selectable_value(&mut m.kind, MotionKind::Static, "Static");
        ui.selectable_value(&mut m.kind, MotionKind::Sweep, "Sweep");
        ui.selectable_value(&mut m.kind, MotionKind::Sine, "Oscillate");
    });
    grid(ui, "motion_grid", |ui| match m.kind {
        MotionKind::Static => {}
        MotionKind::Sweep => {
            row(ui, "Speed", |ui| {
                ui.add(Slider::new(&mut m.speed_px_s, -3000.0..=3000.0).suffix(" px/s"))
                    .on_hover_text("Negative values reverse the direction");
            });
            row(ui, "Span", |ui| {
                ui.add(Slider::new(&mut m.span_pct, 0.0..=100.0).suffix(" %"))
                    .on_hover_text("Travel range, percent of the view across the line, centered on the position");
            });
            row(ui, "At end", |ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut m.sweep_mode, SweepMode::Bounce, "Bounce");
                    ui.selectable_value(&mut m.sweep_mode, SweepMode::Wrap, "Wrap");
                });
            });
        }
        MotionKind::Sine => {
            row(ui, "Amplitude", |ui| {
                ui.add(Slider::new(&mut m.amplitude_px, 0.0..=1000.0).suffix(" px"));
            });
            row(ui, "Frequency", |ui| {
                ui.add(Slider::new(&mut m.frequency_hz, 0.01..=20.0).logarithmic(true).suffix(" Hz"));
            });
            row(ui, "Phase", |ui| {
                ui.add(Slider::new(&mut m.phase_deg, -180.0..=180.0).suffix("°"));
            });
        }
    });
    if m.kind != MotionKind::Static {
        ui.label(RichText::new(format!("Current offset: {offset:+.1} px")).weak());
    }
}

fn deform_ui(ui: &mut Ui, d: &mut DeformParams, length_px: f32, t: f64) {
    ui.horizontal_wrapped(|ui| {
        ui.selectable_value(&mut d.kind, DeformKind::None, "None");
        ui.selectable_value(&mut d.kind, DeformKind::Step, "Step");
        ui.selectable_value(&mut d.kind, DeformKind::Bump, "Bump");
        ui.selectable_value(&mut d.kind, DeformKind::Box, "Box");
        ui.selectable_value(&mut d.kind, DeformKind::Sine, "Wave");
    });
    if d.kind == DeformKind::None {
        return;
    }
    grid(ui, "deform_grid", |ui| {
        match d.kind {
            DeformKind::Sine => {
                row(ui, "Amplitude", |ui| {
                    ui.add(px(&mut d.amplitude_px, 0.0..=300.0));
                });
                row(ui, "Wavelength", |ui| {
                    ui.add(px(&mut d.wavelength_px, 4.0..=4000.0).logarithmic(true));
                });
            }
            _ => {
                row(ui, "Position", |ui| {
                    ui.add(px(&mut d.position_px, -2000.0..=2000.0))
                        .on_hover_text("Along the line, relative to its center");
                });
                row(ui, "Height", |ui| {
                    ui.add(px(&mut d.height_px, -500.0..=500.0))
                        .on_hover_text("Displacement across the line; sign sets the direction");
                });
                if matches!(d.kind, DeformKind::Bump | DeformKind::Box) {
                    row(ui, "Width", |ui| {
                        ui.add(px(&mut d.width_px, 1.0..=3000.0).logarithmic(true));
                    });
                }
                if d.kind == DeformKind::Box {
                    row(ui, "Flank", |ui| {
                        ui.add(px(&mut d.slope_px, 0.0..=1000.0));
                    });
                }
            }
        }
        row(ui, "Travel", |ui| {
            ui.add(Slider::new(&mut d.scroll_px_s, -2000.0..=2000.0).suffix(" px/s"))
                .on_hover_text("Moves the feature along the line, like an object passing through");
        });
    });
    let half = 0.5 * length_px.max(10.0);
    let (lo, hi) = (0..=200)
        .map(|i| model::deform_offset(d, -half + i as f32 * half / 100.0, t))
        .fold((0.0f32, 0.0f32), |(lo, hi), v| (lo.min(v), hi.max(v)));
    let pad = ((hi - lo) * 0.1).max(1.0);
    plot(ui, -half, half, lo - pad, hi + pad, |u| model::deform_offset(d, u, t), "shape along the line");
}

/// Pixel slider that still accepts typed values outside its range.
fn px(v: &mut f32, range: std::ops::RangeInclusive<f32>) -> Slider<'_> {
    Slider::new(v, range).suffix(" px").clamping(egui::SliderClamping::Never)
}

/// Scale, height gain and playback of the scan. `live` is the mapping used in the last frame.
fn scan_params_ui(ui: &mut Ui, s: &mut ScanParams, live: Option<crate::scan::DisplayMapping>, angle_deg: f32) {
    grid(ui, "scan_grid", |ui| {
        row(ui, "Scale", |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut s.scale, ScanScale::FitLine, "Fit line length")
                    .on_hover_text("Stretch the scan width over the line length set above");
                ui.selectable_value(&mut s.scale, ScanScale::Fixed, "Fixed");
            });
        });
        if s.scale == ScanScale::Fixed {
            row(ui, "", |ui| {
                ui.add(Slider::new(&mut s.px_per_mm, 0.05..=200.0).logarithmic(true).suffix(" px/mm"))
                    .on_hover_text("Screen pixels per mm along the line; sets the line length");
            });
        }
        row(ui, "Height gain", |ui| {
            ui.vertical(|ui| {
                ui.horizontal(|ui| {
                    ui.selectable_value(&mut s.height_gain, HeightGain::MatchScale, "1:1")
                        .on_hover_text("Same px/mm as along the line: the line shows the true-scale profile");
                    ui.selectable_value(&mut s.height_gain, HeightGain::FromAngle, format!("sin {angle_deg:.0}°"))
                        .on_hover_text("px/mm · sin(triangulation angle of the scan): what a camera at that angle sees");
                    ui.selectable_value(&mut s.height_gain, HeightGain::Manual, "Manual");
                });
                match s.height_gain {
                    HeightGain::Manual => {
                        ui.add(Slider::new(&mut s.height_px_per_mm, 0.0..=200.0).logarithmic(true).suffix(" px/mm"))
                            .on_hover_text("Line displacement per mm of object height");
                    }
                    HeightGain::FromAngle if angle_deg < 1.0 => {
                        ui.colored_label(
                            ui.visuals().warn_fg_color,
                            "The scan was sliced at a triangulation angle of 0°, so the height gain is zero and the line stays flat.",
                        );
                    }
                    _ => {}
                }
                ui.checkbox(&mut s.invert_height, "Invert (higher = down)");
            });
        });
        if let Some(m) = live {
            row(ui, "", |ui| {
                ui.label(
                    RichText::new(format!(
                        "{:.3} px/mm ({:.4} mm/px) · height {:.3} px/mm",
                        m.px_per_mm,
                        1.0 / m.px_per_mm.max(1e-9),
                        m.height_px_per_mm
                    ))
                    .weak(),
                );
            });
        }
        row(ui, "Rate", |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut s.rate, RateMode::PerFrame, "1 / frame")
                    .on_hover_text("One profile per displayed frame, for a camera synchronised to the display");
                ui.selectable_value(&mut s.rate, RateMode::PerSecond, "Fixed rate");
            });
        });
        if s.rate == RateMode::PerSecond {
            row(ui, "", |ui| {
                ui.add(Slider::new(&mut s.profiles_per_s, 0.1..=1000.0).logarithmic(true).suffix(" profiles/s"))
                    .on_hover_text("Scaled by the simulation speed");
            });
        }
        row(ui, "At end", |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut s.end, EndMode::Loop, "Loop");
                ui.selectable_value(&mut s.end, EndMode::Bounce, "Bounce");
                ui.selectable_value(&mut s.end, EndMode::Once, "Once");
            });
        });
    });
}

fn noise_ui(ui: &mut Ui, n: &mut NoiseParams) {
    grid(ui, "noise_grid", |ui| {
        row(ui, "Ambient light", |ui| {
            ui.add(Slider::new(&mut n.ambient_pct, 0.0..=50.0).suffix(" %"));
        });
        row(ui, "Speckle", |ui| {
            ui.add(Slider::new(&mut n.speckle_pct, 0.0..=100.0).suffix(" %"))
                .on_hover_text("Speckle contrast on the laser line");
        });
        if n.speckle_pct > 0.0 {
            row(ui, "Grain size", |ui| {
                ui.add(Slider::new(&mut n.grain_px, 0.5..=20.0).logarithmic(true).suffix(" px"));
            });
        }
        row(ui, "Sensor noise σ", |ui| {
            ui.add(Slider::new(&mut n.noise_pct, 0.0..=20.0).suffix(" %"));
        });
        row(ui, "", |ui| {
            ui.checkbox(&mut n.animated, "Animated").on_hover_text("Off = frozen pattern");
        });
    });
}

/// Small line plot of `f` over [x0, x1] with y range [y0, y1].
fn plot(ui: &mut Ui, x0: f32, x1: f32, y0: f32, y1: f32, f: impl Fn(f32) -> f32, caption: &str) {
    let width = ui.available_width().min(340.0);
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, 56.0), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.rect_filled(rect, 3.0, visuals.extreme_bg_color);

    let to_screen = |x: f32, y: f32| {
        egui::pos2(
            egui::remap(x, x0..=x1, rect.left()..=rect.right()),
            egui::remap(y, y0..=y1, rect.bottom() - 3.0..=rect.top() + 3.0),
        )
    };
    if y0 < 0.0 && y1 > 0.0 {
        painter.line_segment([to_screen(x0, 0.0), to_screen(x1, 0.0)], Stroke::new(1.0, visuals.weak_text_color()));
    }
    let n = (rect.width() as usize).max(2);
    let points = (0..=n)
        .map(|i| {
            let x = x0 + (x1 - x0) * i as f32 / n as f32;
            to_screen(x, f(x))
        })
        .collect();
    painter.add(egui::Shape::line(points, Stroke::new(1.5, Color32::from_rgb(255, 80, 60))));
    painter.text(rect.left_top() + egui::vec2(4.0, 2.0), egui::Align2::LEFT_TOP, caption, egui::FontId::proportional(10.0), visuals.weak_text_color());
}
