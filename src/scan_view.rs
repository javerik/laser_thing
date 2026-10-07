//! Object slicer window: STL import, slice settings, the background slicing job and
//! the previews (3D view, range image, current profile).

use crate::app::LaserApp;
use crate::mesh::{self, Mesh};
use crate::mesh_view::{GpuMesh, MeshViewCallback, OrbitCamera, Overlay};
use crate::scan::{self, Axis, CameraSide, ModelUnit, Progress, Scan, SliceSettings, UpAxis};
use crate::ui::{grid, row};
use eframe::egui::{self, Color32, RichText, Slider, Stroke, Ui, ViewportBuilder, ViewportClass, ViewportId};
use eframe::egui_wgpu;
use glam::Vec3;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, mpsc};
use std::time::{Duration, Instant};

const SLICER_VIEWPORT: &str = "object_slicer";
/// Settings must be stable this long before an automatic re-slice starts.
const DEBOUNCE: Duration = Duration::from_millis(300);
/// Larger range images are subsampled for display.
const RANGE_IMAGE_MAX: usize = 2048;
/// Larger heightmaps are subsampled when shown as a 3D surface.
const SURFACE_MAX: usize = 400;

const VISIBLE: Color32 = Color32::from_rgb(255, 70, 50);
const OCCLUDED: Color32 = Color32::from_rgb(190, 70, 200);

pub struct Model {
    pub mesh: Arc<Mesh>,
    pub path: PathBuf,
    pub name: String,
    /// Raw bounds in model units.
    pub bounds: ([f32; 3], [f32; 3]),
    id: u64,
}

struct Job {
    cancel: Arc<AtomicBool>,
    progress: Arc<Progress>,
    rx: mpsc::Receiver<Result<(Scan, Duration), String>>,
}

/// What the 3D preview mesh was built from.
#[derive(Clone, PartialEq)]
enum PreviewKey {
    /// Model id and the transform part of the settings.
    Model(u64, SliceSettings),
    /// Heightmap surface of a scan loaded without its model.
    Scan(usize),
}

struct Preview {
    key: PreviewKey,
    mesh: Arc<GpuMesh>,
    /// Bounds of the preview geometry in scan-frame mm.
    bounds: (Vec3, Vec3),
}

pub struct Slicer {
    pub open: bool,
    pub settings: SliceSettings,
    /// Re-slice automatically when settings change.
    pub auto: bool,
    pub model: Option<Model>,
    job: Option<Job>,
    /// Settings of the most recently started slice.
    sliced: Option<SliceSettings>,
    /// Settings waiting for the debounce, and since when.
    pending: Option<(SliceSettings, Instant)>,
    pub error: Option<String>,
    pub last_duration: Option<Duration>,
    camera: Option<OrbitCamera>,
    preview: Option<Preview>,
    range_image: Option<(egui::TextureHandle, Arc<Scan>)>,
    next_id: u64,
}

impl Slicer {
    pub fn new(settings: SliceSettings, auto: bool) -> Self {
        Self {
            open: false,
            settings,
            auto,
            model: None,
            job: None,
            sliced: None,
            pending: None,
            error: None,
            last_duration: None,
            camera: None,
            preview: None,
            range_image: None,
            next_id: 1,
        }
    }

    pub fn is_busy(&self) -> bool {
        self.job.is_some()
    }

    /// True when the loaded model has settings that were not sliced yet.
    pub fn is_stale(&self) -> bool {
        self.model.is_some() && self.sliced.as_ref() != Some(&self.settings)
    }

    pub fn set_model(&mut self, mesh: Mesh, path: PathBuf) {
        let name = path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned());
        let bounds = mesh.bounds();
        self.model = Some(Model { mesh: Arc::new(mesh), path, name, bounds, id: self.next_id });
        self.next_id += 1;
        self.sliced = None;
        self.camera = None;
        self.error = None;
        if self.auto {
            self.start();
        }
    }

    /// Starts slicing the model with the current settings, cancelling a running job.
    pub fn start(&mut self) {
        let Some(model) = &self.model else { return };
        if let Some(job) = self.job.take() {
            job.cancel.store(true, Ordering::Relaxed);
        }
        let cancel = Arc::new(AtomicBool::new(false));
        let progress = Arc::new(Progress::new(0));
        let (tx, rx) = mpsc::channel();
        let (mesh, settings, name) = (model.mesh.clone(), self.settings.clone(), model.name.clone());
        let (c, p) = (cancel.clone(), progress.clone());
        std::thread::spawn(move || {
            let t = Instant::now();
            let result = scan::slice(&mesh, &settings, &name, &c, &p).map(|s| (s, t.elapsed()));
            let _ = tx.send(result);
        });
        self.job = Some(Job { cancel, progress, rx });
        self.sliced = Some(self.settings.clone());
        self.pending = None;
        self.error = None;
    }

    /// Collects a finished slice and starts debounced automatic re-slices.
    pub fn poll(&mut self) -> Option<Scan> {
        let mut done = None;
        if let Some(job) = &self.job {
            match job.rx.try_recv() {
                Ok(Ok((scan, took))) => {
                    self.last_duration = Some(took);
                    done = Some(scan);
                    self.job = None;
                }
                Ok(Err(e)) => {
                    if e != "cancelled" {
                        self.error = Some(e);
                    }
                    self.job = None;
                }
                Err(mpsc::TryRecvError::Empty) => {}
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.error = Some("slicing failed unexpectedly".into());
                    self.job = None;
                }
            }
        }

        if !self.is_stale() {
            self.pending = None;
        } else if self.auto {
            if self.pending.as_ref().is_none_or(|(s, _)| *s != self.settings) {
                self.pending = Some((self.settings.clone(), Instant::now()));
            }
            if self.pending.as_ref().is_some_and(|(_, t)| t.elapsed() >= DEBOUNCE) {
                self.start();
            }
        }
        done
    }

    /// Rebuilds the 3D preview mesh when the model, its transform or the scan changed.
    fn update_preview(&mut self, scan: Option<&Arc<Scan>>) {
        let key = match (&self.model, scan) {
            (Some(m), _) => PreviewKey::Model(m.id, transform_part(&self.settings)),
            (None, Some(s)) => PreviewKey::Scan(Arc::as_ptr(s) as usize),
            (None, None) => {
                self.preview = None;
                return;
            }
        };
        if self.preview.as_ref().is_some_and(|p| p.key == key) {
            return;
        }
        let id = self.next_id;
        self.next_id += 1;
        let (positions, indices) = match (&self.model, scan) {
            (Some(m), _) => (mesh::to_scan_frame(&m.mesh, &self.settings), m.mesh.triangles.iter().flatten().copied().collect()),
            (None, Some(s)) => heightmap_surface(s),
            (None, None) => unreachable!(),
        };
        let (lo, hi) = mesh::bounds(&positions);
        self.preview = Some(Preview {
            key,
            mesh: Arc::new(GpuMesh { id, positions, indices }),
            bounds: (lo.into(), hi.into()),
        });
    }
}

fn transform_part(s: &SliceSettings) -> SliceSettings {
    SliceSettings {
        up_axis: s.up_axis,
        travel_axis: s.travel_axis,
        flip_along: s.flip_along,
        yaw_deg: s.yaw_deg,
        unit: s.unit,
        scale: s.scale,
        ..Default::default()
    }
}

/// Subsampled triangle surface of a scan's heightmap, for scans loaded without a model.
fn heightmap_surface(s: &Scan) -> (Vec<[f32; 3]>, Vec<u32>) {
    let m = &s.meta;
    let step = m.cols.max(m.rows).div_ceil(SURFACE_MAX).max(1);
    let (nc, nr) = ((m.cols - 1) / step + 1, (m.rows - 1) / step + 1);
    let mut positions = Vec::with_capacity(nc * nr);
    for j in 0..nr {
        for i in 0..nc {
            let (ci, rj) = (i * step, j * step);
            let h = s.height[rj * m.cols + ci];
            positions.push([m.u0_mm + ci as f32 * m.du_mm, m.v0_mm + rj as f32 * m.dv_mm, h]);
        }
    }
    let mut indices = Vec::new();
    let at = |i: usize, j: usize| (j * nc + i) as u32;
    for j in 0..nr.saturating_sub(1) {
        for i in 0..nc.saturating_sub(1) {
            let q = [at(i, j), at(i + 1, j), at(i + 1, j + 1), at(i, j + 1)];
            if q.iter().all(|&k| positions[k as usize][2].is_finite()) {
                indices.extend([q[0], q[1], q[2], q[0], q[2], q[3]]);
            }
        }
    }
    // The GPU must not see NaN positions, even unreferenced ones.
    for p in &mut positions {
        if !p[2].is_finite() {
            p[2] = 0.0;
        }
    }
    (positions, indices)
}

fn nice_step(span: f32) -> f32 {
    let raw = (span / 10.0).max(1e-3);
    let mag = 10f32.powf(raw.log10().floor());
    [1.0, 2.0, 5.0, 10.0].into_iter().map(|k| k * mag).find(|s| *s >= raw).unwrap_or(10.0 * mag)
}

fn rgba(c: Color32, a: f32) -> [f32; 4] {
    [c.r() as f32 / 255.0, c.g() as f32 / 255.0, c.b() as f32 / 255.0, a]
}

impl LaserApp {
    pub fn import_stl(&mut self) {
        match crate::scan_file::import_stl_dialog() {
            Ok(Some((mesh, path))) => {
                let n = mesh.triangles.len();
                self.scan.slicer.set_model(mesh, path.clone());
                self.scan.slicer.open = true;
                self.set_status(format!("Imported {} ({n} triangles)", path.display()));
            }
            Ok(None) => {}
            Err(e) => self.set_status(format!("Import failed: {e}")),
        }
    }

    pub fn show_slicer(&mut self, ctx: &egui::Context) {
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
                    .show(ctx, |ui| self.slicer_panels(ui));
                self.scan.slicer.open &= open;
                return;
            }
            egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| self.slicer_panels(ui));
            if ctx.input(|i| i.viewport().close_requested()) {
                self.scan.slicer.open = false;
            }
        });
    }

    fn slicer_panels(&mut self, ui: &mut Ui) {
        let scan = self.scan.scan.clone();
        self.scan.slicer.update_preview(scan.as_ref());
        egui::SidePanel::left("slicer_settings")
            .resizable(true)
            .default_width(330.0)
            .min_width(280.0)
            .show_inside(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| self.slicer_settings_ui(ui));
            });
        egui::CentralPanel::default().show_inside(ui, |ui| self.slicer_preview_ui(ui, scan));
    }

    fn slicer_settings_ui(&mut self, ui: &mut Ui) {
        ui.spacing_mut().slider_width = 150.0;
        ui.horizontal(|ui| {
            ui.heading("Object");
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("📂 Import STL…").clicked() {
                    self.import_stl();
                }
            });
        });
        let sl = &mut self.scan.slicer;
        match &sl.model {
            Some(m) => {
                ui.label(RichText::new(&m.name).strong()).on_hover_text(m.path.display().to_string());
                let (lo, hi) = m.bounds;
                ui.label(
                    RichText::new(format!(
                        "{} triangles · {:.4} × {:.4} × {:.4} model units",
                        m.mesh.triangles.len(),
                        hi[0] - lo[0],
                        hi[1] - lo[1],
                        hi[2] - lo[2]
                    ))
                    .weak(),
                );
            }
            None if self.scan.scan.is_some() => {
                ui.label(RichText::new("Playing a loaded scan; its settings are shown below. Import the model to re-slice.").weak());
            }
            None => {
                ui.label(RichText::new("No model loaded. Import an STL file to slice it.").weak());
            }
        }
        ui.separator();

        // Settings of a scan loaded without its model can't be applied, only shown.
        let editable = sl.model.is_some() || self.scan.scan.is_none();
        ui.add_enabled_ui(editable, |ui| self.slice_settings_ui(ui));
        let sl = &mut self.scan.slicer;
        ui.separator();
        ui.horizontal(|ui| {
            ui.checkbox(&mut sl.auto, "Auto re-slice");
            ui.add_enabled_ui(sl.model.is_some(), |ui| {
                if ui.button("⟳ Slice now").clicked() {
                    sl.start();
                }
            });
        });
        if let Some(job) = &sl.job {
            let f = job.progress.load(Ordering::Relaxed) as f32 / 1000.0;
            ui.add(egui::ProgressBar::new(f).text("Slicing…").animate(true));
        } else if sl.is_stale() {
            ui.label(RichText::new("Settings changed since the last slice").weak());
        } else if let Some(d) = sl.last_duration
            && sl.model.is_some()
        {
            ui.label(RichText::new(format!("Sliced in {:.2} s", d.as_secs_f32())).weak());
        }
        if let Some(e) = &sl.error {
            ui.colored_label(ui.visuals().error_fg_color, e);
        }
        if let Some(scan) = &self.scan.scan {
            ui.separator();
            scan_summary(ui, scan);
            ui.horizontal(|ui| {
                if ui.button("💾 Save scan…").clicked() {
                    self.save_scan();
                }
                if ui.button("📂 Load scan…").clicked() {
                    self.load_scan();
                }
            });
        }
    }

    /// Orientation, scale, resolution and sensor settings of the slicer.
    fn slice_settings_ui(&mut self, ui: &mut Ui) {
        let sl = &mut self.scan.slicer;
        let s = &mut sl.settings;
        egui::CollapsingHeader::new(RichText::new("Orientation").strong()).default_open(true).show(ui, |ui| {
            grid(ui, "slice_orient", |ui| {
                row(ui, "Up axis", |ui| {
                    egui::ComboBox::from_id_salt("up_axis").selected_text(up_label(s.up_axis)).show_ui(ui, |ui| {
                        for a in [UpAxis::PosX, UpAxis::NegX, UpAxis::PosY, UpAxis::NegY, UpAxis::PosZ, UpAxis::NegZ] {
                            ui.selectable_value(&mut s.up_axis, a, up_label(a));
                        }
                    });
                    ui.label("").on_hover_text("Model axis pointing towards the sensor");
                });
                let up = s.up_axis.index_sign().0;
                row(ui, "Travel axis", |ui| {
                    ui.horizontal(|ui| {
                        for (i, a) in [Axis::X, Axis::Y, Axis::Z].into_iter().enumerate() {
                            ui.add_enabled_ui(i != up, |ui| ui.selectable_value(&mut s.travel_axis, a, axis_label(i)));
                        }
                    });
                });
                // Keep the travel axis valid after the up axis changed.
                s.travel_axis = [Axis::X, Axis::Y, Axis::Z][s.travel_axis_index()];
                let along = 3 - up - s.travel_axis_index();
                row(ui, "Along line", |ui| {
                    ui.checkbox(&mut s.flip_along, format!("{} (mirrored)", axis_label(along)))
                        .on_hover_text("The remaining axis runs along the laser line; tick to mirror it");
                });
                row(ui, "Rotation", |ui| {
                    ui.horizontal(|ui| {
                        ui.add(Slider::new(&mut s.yaw_deg, -180.0..=180.0).suffix("°"))
                            .on_hover_text("Rotates the object on the conveyor, about the up axis");
                        if ui.small_button("0").clicked() {
                            s.yaw_deg = 0.0;
                        }
                    });
                });
            });
        });

        egui::CollapsingHeader::new(RichText::new("Scale").strong()).default_open(true).show(ui, |ui| {
            grid(ui, "slice_scale", |ui| {
                row(ui, "Model unit", |ui| {
                    egui::ComboBox::from_id_salt("model_unit").selected_text(unit_label(s.unit)).show_ui(ui, |ui| {
                        for u in [ModelUnit::Mm, ModelUnit::Cm, ModelUnit::M, ModelUnit::Inch] {
                            ui.selectable_value(&mut s.unit, u, unit_label(u));
                        }
                    });
                });
                row(ui, "Scale", |ui| {
                    ui.horizontal(|ui| {
                        ui.add(egui::DragValue::new(&mut s.scale).speed(0.01).range(0.001..=1000.0).suffix("×"));
                        if ui.small_button("1×").clicked() {
                            s.scale = 1.0;
                        }
                    });
                });
                if let Some(p) = &sl.preview
                    && sl.model.is_some()
                {
                    let (lo, hi) = p.bounds;
                    let d = hi - lo;
                    row(ui, "Size", |ui| {
                        ui.label(format!("{:.1} × {:.1} × {:.1} mm", d.x, d.y, d.z))
                            .on_hover_text("Along line × travel × height");
                    });
                }
            });
        });

        egui::CollapsingHeader::new(RichText::new("Resolution").strong()).default_open(true).show(ui, |ui| {
            grid(ui, "slice_res", |ui| {
                row(ui, "Along line", |ui| {
                    ui.add(mm_slider(&mut s.lateral_res_mm, 0.005..=10.0).logarithmic(true))
                        .on_hover_text("Sample spacing along the laser line");
                });
                row(ui, "Travel step", |ui| {
                    ui.add(mm_slider(&mut s.travel_step_mm, 0.005..=50.0).logarithmic(true))
                        .on_hover_text("Object movement between two profiles");
                });
                row(ui, "Side margin", |ui| {
                    ui.add(mm_slider(&mut s.margin_lateral_mm, 0.0..=500.0));
                });
                row(ui, "Lead in/out", |ui| {
                    ui.add(mm_slider(&mut s.margin_travel_mm, 0.0..=500.0))
                        .on_hover_text("Empty travel before and after the object");
                });
            });
            if let Some(p) = sl.preview.as_ref().filter(|_| sl.model.is_some()) {
                match scan::grid_size(p.bounds.0.into(), p.bounds.1.into(), s) {
                    Ok((c, r)) => {
                        let mb = (c * r * 5) as f32 / 1e6;
                        ui.label(RichText::new(format!("{c} samples × {r} profiles · {mb:.1} MB")).weak());
                    }
                    Err(e) => {
                        ui.colored_label(ui.visuals().error_fg_color, e);
                    }
                }
            }
        });

        egui::CollapsingHeader::new(RichText::new("Sensor").strong()).default_open(true).show(ui, |ui| {
            grid(ui, "slice_sensor", |ui| {
                row(ui, "Conveyor", |ui| {
                    ui.checkbox(&mut s.base_plane, "Plane at z = 0")
                        .on_hover_text("Keep the line visible next to the object, as on a belt");
                });
                row(ui, "Triangulation", |ui| {
                    ui.add(Slider::new(&mut s.triangulation_deg, 0.0..=80.0).suffix("°"))
                        .on_hover_text("Angle between camera axis and laser plane; also sets the default height gain");
                });
                row(ui, "Occlusion", |ui| {
                    ui.checkbox(&mut s.occlusion, "Hide what the camera can't see");
                });
                row(ui, "Camera side", |ui| {
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut s.camera_side, CameraSide::Ahead, "Ahead (+travel)");
                        ui.selectable_value(&mut s.camera_side, CameraSide::Behind, "Behind");
                    });
                });
            });
        });
        ui.horizontal(|ui| {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.small_button("↺ Reset").on_hover_text("Reset slice settings to defaults").clicked() {
                    *s = SliceSettings::default();
                }
            });
        });
    }

    fn slicer_preview_ui(&mut self, ui: &mut Ui, scan: Option<Arc<Scan>>) {
        let row = self.current_row();
        let avail = ui.available_size();
        let bottom = if scan.is_some() { 300.0 } else { 0.0 };
        let view_h = (avail.y - bottom).max(160.0);
        let (rect, response) = ui.allocate_exact_size(egui::vec2(avail.x, view_h), egui::Sense::click_and_drag());
        self.view_3d(ui, rect, &response, scan.as_deref(), row);

        let Some(scan) = scan else { return };
        let rows = scan.meta.rows;
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Range image").strong());
            ui.label(RichText::new("x: travel · y: along line · magenta: occluded · click to pick a profile").weak().small());
        });
        if let Some(r) = self.range_image_ui(ui, &scan, row, 130.0) {
            self.scan.pos = r as f64;
        }
        ui.add_space(4.0);
        profile_plot(ui, &scan, row, 100.0);
        ui.horizontal(|ui| {
            if ui.button("◀").on_hover_text("Previous profile (←)").clicked() {
                self.step_scan(-1);
            }
            if ui.button("▶").on_hover_text("Next profile (→)").clicked() {
                self.step_scan(1);
            }
            let mut r = row;
            ui.spacing_mut().slider_width = (ui.available_width() - 220.0).max(80.0);
            if ui.add(Slider::new(&mut r, 0..=rows - 1).text("profile")).changed() {
                self.scan.pos = r as f64;
            }
            ui.label(format!("v = {:.2} mm", scan.meta.v0_mm + row as f32 * scan.meta.dv_mm));
        });
    }

    fn view_3d(&mut self, ui: &mut Ui, rect: egui::Rect, response: &egui::Response, scan: Option<&Scan>, row: usize) {
        let painter = ui.painter_at(rect);
        let sl = &mut self.scan.slicer;
        let mesh = sl.preview.as_ref().map(|p| p.mesh.clone());
        let mesh_bounds = sl.preview.as_ref().map(|p| p.bounds);
        let scan_bounds = scan.map(|s| {
            let m = &s.meta;
            let lo = Vec3::new(m.u0_mm, m.v0_mm, 0.0);
            (lo, lo + Vec3::new(s.width_mm(), s.length_mm(), m.z_max_mm))
        });
        let Some((lo, mut hi)) = [mesh_bounds, scan_bounds].into_iter().flatten().reduce(|a, b| (a.0.min(b.0), a.1.max(b.1))) else {
            painter.rect_filled(rect, 0.0, Color32::from_gray(18));
            painter.text(rect.center(), egui::Align2::CENTER_CENTER, "Import an STL model to start", egui::FontId::proportional(18.0), Color32::from_gray(140));
            return;
        };

        let z_max = hi.z;
        // Frame the laser source and camera marker above the object too.
        hi.z = lo.z + (hi.z - lo.z) * 1.6;
        let camera = sl.camera.get_or_insert_with(|| OrbitCamera::framing(lo, hi));
        if response.double_clicked() {
            *camera = OrbitCamera::framing(lo, hi);
        }
        camera.handle_input(ui, response);
        let camera = *camera;

        let settings = scan.map_or(&sl.settings, |s| &s.meta.settings);
        let overlay = build_overlay(scan, row, settings, lo, hi.with_z(z_max));
        let ppp = ui.ctx().pixels_per_point();
        painter.add(egui_wgpu::Callback::new_paint_callback(
            rect,
            MeshViewCallback {
                mesh,
                overlay: Arc::new(overlay.overlay),
                uniforms: camera.uniforms(rect.aspect_ratio(), z_max.max(1e-3)),
                size_px: [(rect.width() * ppp).round() as u32, (rect.height() * ppp).round() as u32],
                srgb_target: self.srgb_target(),
            },
        ));
        for (pos, text, color) in overlay.labels {
            if let Some(p) = camera.project(rect, pos) {
                painter.text(p, egui::Align2::LEFT_BOTTOM, text, egui::FontId::proportional(13.0), color);
            }
        }
        painter.text(
            rect.left_bottom() + egui::vec2(8.0, -6.0),
            egui::Align2::LEFT_BOTTOM,
            "drag: orbit · right/shift-drag: pan · scroll: zoom · double-click: reset",
            egui::FontId::proportional(11.0),
            Color32::from_gray(120),
        );
    }

    /// Draws the range image; returns the profile under the pointer while clicked/dragged.
    fn range_image_ui(&mut self, ui: &mut Ui, scan: &Arc<Scan>, row: usize, height: f32) -> Option<usize> {
        let sl = &mut self.scan.slicer;
        if !sl.range_image.as_ref().is_some_and(|(_, s)| Arc::ptr_eq(s, scan)) {
            let tex = ui.ctx().load_texture("range_image", range_image(scan), egui::TextureOptions::LINEAR);
            sl.range_image = Some((tex, scan.clone()));
        }
        let (tex, _) = sl.range_image.as_ref().unwrap();
        let (rect, response) = ui.allocate_exact_size(egui::vec2(ui.available_width(), height), egui::Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.image(tex.id(), rect, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        let rows = scan.meta.rows;
        let x = rect.left() + (row as f32 + 0.5) / rows as f32 * rect.width();
        painter.line_segment([egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())], Stroke::new(1.5, VISIBLE));
        let pointer = response.interact_pointer_pos()?;
        (response.is_pointer_button_down_on() || response.clicked())
            .then(|| (((pointer.x - rect.left()) / rect.width() * rows as f32) as usize).min(rows - 1))
    }
}

struct OverlayWithLabels {
    overlay: Overlay,
    labels: Vec<(Vec3, &'static str, Color32)>,
}

/// Conveyor grid, travel arrow, laser plane and fan, camera direction and the current profile.
fn build_overlay(scan: Option<&Scan>, row: usize, s: &SliceSettings, lo: Vec3, hi: Vec3) -> OverlayWithLabels {
    let mut o = Overlay::default();
    let mut labels = Vec::new();
    let (u0, u1, v0, v1) = match scan {
        Some(sc) => (sc.meta.u0_mm, sc.meta.u0_mm + sc.width_mm(), sc.meta.v0_mm, sc.meta.v0_mm + sc.length_mm()),
        None => (lo.x, hi.x, lo.y, hi.y),
    };
    let step = nice_step((u1 - u0).max(v1 - v0));
    let grid_c = [0.45, 0.47, 0.5, if s.base_plane { 0.55 } else { 0.25 }];
    let mut u = (u0 / step).ceil() * step;
    while u <= u1 {
        o.line(Vec3::new(u, v0, 0.0), Vec3::new(u, v1, 0.0), grid_c);
        u += step;
    }
    let mut v = (v0 / step).ceil() * step;
    while v <= v1 {
        o.line(Vec3::new(u0, v, 0.0), Vec3::new(u1, v, 0.0), grid_c);
        v += step;
    }
    let edge = [0.7, 0.72, 0.76, 0.9];
    let c = [Vec3::new(u0, v0, 0.0), Vec3::new(u1, v0, 0.0), Vec3::new(u1, v1, 0.0), Vec3::new(u0, v1, 0.0)];
    for k in 0..4 {
        o.line(c[k], c[(k + 1) % 4], edge);
    }

    // Travel arrow beside the scan area.
    let yellow = [0.95, 0.78, 0.2, 1.0];
    let ua = u1 + step;
    let (va, vb) = (v0, v0 + (v1 - v0).max(step));
    o.line(Vec3::new(ua, va, 0.0), Vec3::new(ua, vb, 0.0), yellow);
    o.line(Vec3::new(ua, vb, 0.0), Vec3::new(ua - 0.4 * step, vb - 0.8 * step, 0.0), yellow);
    o.line(Vec3::new(ua, vb, 0.0), Vec3::new(ua + 0.4 * step, vb - 0.8 * step, 0.0), yellow);
    labels.push((Vec3::new(ua + 0.3 * step, vb, 0.0), "travel", Color32::from_rgb(242, 199, 51)));

    let Some(sc) = scan else {
        return OverlayWithLabels { overlay: o, labels };
    };
    let m = &sc.meta;
    let v = m.v0_mm + row as f32 * m.dv_mm;
    let top = hi.z.max(m.z_max_mm).max(step) * 1.2;
    let uc = 0.5 * (u0 + u1);

    // Laser plane and fan from the source above.
    o.quad([Vec3::new(u0, v, 0.0), Vec3::new(u1, v, 0.0), Vec3::new(u1, v, top), Vec3::new(u0, v, top)], [1.0, 0.2, 0.15, 0.16]);
    let src = Vec3::new(uc, v, top * 1.3);
    let fan = [1.0, 0.3, 0.2, 0.55];
    o.line(src, Vec3::new(u0, v, 0.0), fan);
    o.line(src, Vec3::new(u1, v, 0.0), fan);
    labels.push((src, "laser", Color32::from_rgb(255, 110, 90)));

    // Camera viewing direction in the travel/height plane.
    let alpha = m.settings.triangulation_deg.to_radians();
    let side = m.settings.camera_side.sign() as f32;
    let cam = Vec3::new(uc, v + side * alpha.sin() * top * 1.3, alpha.cos() * top * 1.3);
    let cyan = [0.35, 0.85, 1.0, 1.0];
    o.line_on_top(Vec3::new(uc, v, 0.0), cam, cyan);
    labels.push((cam, "camera", Color32::from_rgb(90, 215, 255)));

    // Current profile, always on top: red where seen, magenta where occluded.
    let heights = sc.row_height(row);
    let vis = sc.row_visible(row);
    let lift = 0.002 * top;
    for i in 0..m.cols.saturating_sub(1) {
        let (a, b) = (heights[i], heights[i + 1]);
        if !(a.is_finite() && b.is_finite()) {
            continue;
        }
        let color = if vis[i] != 0 && vis[i + 1] != 0 { rgba(VISIBLE, 1.0) } else { rgba(OCCLUDED, 0.8) };
        let ua = m.u0_mm + i as f32 * m.du_mm;
        o.line_on_top(Vec3::new(ua, v, a + lift), Vec3::new(ua + m.du_mm, v, b + lift), color);
    }
    OverlayWithLabels { overlay: o, labels }
}

/// Range image with travel to the right and the line direction downwards.
fn range_image(scan: &Scan) -> egui::ColorImage {
    let m = &scan.meta;
    let (sx, sy) = (m.rows.div_ceil(RANGE_IMAGE_MAX).max(1), m.cols.div_ceil(RANGE_IMAGE_MAX).max(1));
    let (w, h) = (m.rows.div_ceil(sx), m.cols.div_ceil(sy));
    let z_max = m.z_max_mm.max(1e-6);
    let mut pixels = Vec::with_capacity(w * h);
    for y in 0..h {
        let i = y * sy;
        for x in 0..w {
            let k = x * sx * m.cols + i;
            let z = scan.height[k];
            pixels.push(if !z.is_finite() {
                Color32::BLACK
            } else {
                let t = (z / z_max).clamp(0.0, 1.0);
                if scan.visible[k] != 0 {
                    Color32::from_gray((45.0 + 210.0 * t) as u8)
                } else {
                    Color32::from_rgb((100.0 + 120.0 * t) as u8, 25, (95.0 + 110.0 * t) as u8)
                }
            });
        }
    }
    egui::ColorImage::new([w, h], pixels)
}

/// Height along the line for one profile: red where the camera sees it, magenta where occluded.
pub(crate) fn profile_plot(ui: &mut Ui, scan: &Scan, row: usize, height: f32) {
    let width = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, height), egui::Sense::hover());
    let painter = ui.painter_at(rect);
    let visuals = ui.visuals();
    painter.rect_filled(rect, 3.0, visuals.extreme_bg_color);
    let m = &scan.meta;
    let z_top = (m.z_max_mm * 1.1).max(1e-3);
    let to_screen = |i: usize, z: f32| {
        egui::pos2(
            egui::remap(i as f32, 0.0..=(m.cols.max(2) - 1) as f32, rect.left() + 2.0..=rect.right() - 2.0),
            egui::remap(z, 0.0..=z_top, rect.bottom() - 4.0..=rect.top() + 14.0),
        )
    };
    let heights = scan.row_height(row);
    let vis = scan.row_visible(row);
    // Runs of equally classified samples become one polyline each.
    let mut run: Vec<egui::Pos2> = Vec::new();
    let mut run_class = 0u8;
    let flush = |run: &mut Vec<egui::Pos2>, class: u8| {
        if run.len() >= 2 {
            let color = if class == 1 { VISIBLE } else { OCCLUDED };
            painter.add(egui::Shape::line(std::mem::take(run), Stroke::new(1.5, color)));
        }
        run.clear();
    };
    for i in 0..m.cols {
        let class = if !heights[i].is_finite() { 0 } else if vis[i] != 0 { 1 } else { 2 };
        if class != run_class {
            let last = run.last().copied();
            flush(&mut run, run_class);
            // Connect neighbouring runs so the outline stays continuous.
            if let (Some(p), true) = (last, class != 0) {
                run.push(p);
            }
            run_class = class;
        }
        if class != 0 {
            run.push(to_screen(i, heights[i]));
        }
    }
    flush(&mut run, run_class);
    painter.text(
        rect.left_top() + egui::vec2(4.0, 2.0),
        egui::Align2::LEFT_TOP,
        format!("profile {row} · width {:.1} mm · max {:.2} mm", scan.width_mm(), m.z_max_mm),
        egui::FontId::proportional(10.0),
        visuals.weak_text_color(),
    );
}

pub(crate) fn scan_summary(ui: &mut Ui, scan: &Scan) {
    let m = &scan.meta;
    ui.label(RichText::new(if m.source.is_empty() { "Scan" } else { &m.source }).strong());
    ui.label(
        RichText::new(format!(
            "{} profiles × {} samples · {:.1} × {:.1} mm · max height {:.2} mm",
            m.rows,
            m.cols,
            scan.width_mm(),
            scan.length_mm(),
            m.z_max_mm
        ))
        .weak(),
    );
}

fn mm_slider(v: &mut f32, range: std::ops::RangeInclusive<f32>) -> Slider<'_> {
    Slider::new(v, range).suffix(" mm").clamping(egui::SliderClamping::Never)
}

fn axis_label(i: usize) -> &'static str {
    ["X", "Y", "Z"][i]
}

fn up_label(a: UpAxis) -> &'static str {
    match a {
        UpAxis::PosX => "+X",
        UpAxis::NegX => "−X",
        UpAxis::PosY => "+Y",
        UpAxis::NegY => "−Y",
        UpAxis::PosZ => "+Z",
        UpAxis::NegZ => "−Z",
    }
}

fn unit_label(u: ModelUnit) -> &'static str {
    match u {
        ModelUnit::Mm => "mm",
        ModelUnit::Cm => "cm",
        ModelUnit::M => "m",
        ModelUnit::Inch => "inch",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::tests::box_mesh;

    #[test]
    fn surface_of_loaded_scan() {
        let m = box_mesh([0.0; 3], [10.0, 10.0, 2.0]);
        let s = SliceSettings { lateral_res_mm: 1.0, travel_step_mm: 1.0, base_plane: false, margin_lateral_mm: 2.0, margin_travel_mm: 2.0, ..Default::default() };
        let scan = scan::slice(&m, &s, "", &AtomicBool::new(false), &Progress::new(0)).unwrap();
        let (pos, idx) = heightmap_surface(&scan);
        assert_eq!(pos.len(), 15 * 15);
        assert!(pos.iter().all(|p| p[2].is_finite()));
        // Only the 10 × 10 quads on top of the box have four finite corners.
        assert_eq!(idx.len(), 10 * 10 * 6);
    }

    #[test]
    fn nice_steps() {
        assert_eq!(nice_step(100.0), 10.0);
        assert_eq!(nice_step(130.0), 20.0);
        assert_eq!(nice_step(4.0), 0.5);
    }
}
