//! Window layout of an emulator app and its keyboard shortcuts.
//!
//! [`Layout`] records where the controls and the live view are shown; [`Action`] is
//! everything a shortcut can do, applied with [`crate::LaserUi::apply`]. Opening and
//! closing the windows stays with the application (see [`crate::show_window`]).

use serde::{Deserialize, Serialize};

/// Where the control panel is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DockState {
    /// In the main window: next to the live view, or filling it while the view is detached.
    #[default]
    Docked,
    /// In a window of its own.
    Detached,
    /// Not shown.
    Hidden,
}

/// Where the live laser view is shown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewPlacement {
    /// Fills the main window (next to docked controls).
    #[default]
    Docked,
    /// In a window of its own, e.g. fullscreen on a projector.
    Detached,
}

/// Placement of the controls and the live view. Serde, to persist it between runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Layout {
    /// Where the controls are.
    pub controls: DockState,
    /// Where the live view is.
    pub view: ViewPlacement,
    /// Where the controls return to when un-hidden.
    pub controls_before_hide: DockState,
    /// The window showing the live view is fullscreen. The host keeps this in sync
    /// with the window; it is not persisted.
    #[serde(skip)]
    pub fullscreen: bool,
}

impl Layout {
    /// The layout to start with after loading a persisted one: hidden controls are shown.
    pub fn restored(self) -> Self {
        let controls = if self.controls == DockState::Hidden { self.controls_before_hide } else { self.controls };
        Self { controls, controls_before_hide: DockState::Docked, fullscreen: false, ..self }
    }

    /// Detaches docked controls; docks detached or hidden ones.
    pub fn toggle_controls_detached(&mut self) {
        self.controls = match self.controls {
            DockState::Docked => DockState::Detached,
            DockState::Detached | DockState::Hidden => DockState::Docked,
        };
    }

    /// Hides the controls, or shows them where they were before.
    pub fn toggle_controls_hidden(&mut self) {
        if self.controls == DockState::Hidden {
            self.controls = self.controls_before_hide;
        } else {
            self.controls_before_hide = self.controls;
            self.controls = DockState::Hidden;
        }
    }

    /// Detaches or docks the live view. Returns true if the host must leave fullscreen
    /// first: the main window keeps only the controls after detaching.
    pub fn toggle_view_detached(&mut self) -> bool {
        match self.view {
            ViewPlacement::Docked => {
                self.view = ViewPlacement::Detached;
                std::mem::take(&mut self.fullscreen)
            }
            ViewPlacement::Detached => {
                self.view = ViewPlacement::Docked;
                false
            }
        }
    }

    /// The live view has its window to itself (no docked controls beside it).
    pub fn view_alone(&self) -> bool {
        self.view == ViewPlacement::Detached || self.controls != DockState::Docked
    }
}

/// Everything a keyboard shortcut can do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    /// Pause or resume the simulation (Space).
    TogglePlay,
    /// Rewind the clock and the scan playback (R).
    ResetTime,
    /// Show the previous scan profile (←).
    StepBack,
    /// Show the next scan profile (→).
    StepForward,
    /// Toggle fullscreen of the live view's window (F11).
    ToggleFullscreen,
    /// Leave fullscreen (Esc).
    ExitFullscreen,
    /// Detach or dock the controls (D).
    ToggleControlsDetached,
    /// Detach or dock the live view (V).
    ToggleViewDetached,
    /// Hide or show the controls (H). Accept it only from the window showing the live
    /// view, so the controls cannot hide their own window.
    ToggleControlsHidden,
    /// Open or close the slicer (S).
    ToggleSlicer,
}

impl Action {
    /// Every action, in the order of the shortcut help.
    pub const ALL: [Action; 10] = [
        Action::ToggleFullscreen,
        Action::ExitFullscreen,
        Action::ToggleControlsDetached,
        Action::ToggleViewDetached,
        Action::ToggleControlsHidden,
        Action::TogglePlay,
        Action::ResetTime,
        Action::StepBack,
        Action::StepForward,
        Action::ToggleSlicer,
    ];

    /// The action bound to `key`; `None` for other keys and with Ctrl, Alt or Cmd held.
    pub fn from_key(key: egui::Key, modifiers: egui::Modifiers) -> Option<Action> {
        use egui::Key;
        if modifiers.ctrl || modifiers.alt || modifiers.command || modifiers.mac_cmd {
            return None;
        }
        Some(match key {
            Key::Space => Action::TogglePlay,
            Key::R => Action::ResetTime,
            Key::ArrowLeft => Action::StepBack,
            Key::ArrowRight => Action::StepForward,
            Key::F11 => Action::ToggleFullscreen,
            Key::Escape => Action::ExitFullscreen,
            Key::D => Action::ToggleControlsDetached,
            Key::V => Action::ToggleViewDetached,
            Key::H => Action::ToggleControlsHidden,
            Key::S => Action::ToggleSlicer,
            _ => return None,
        })
    }

    /// Key label for help texts, e.g. `"F11"`.
    pub fn key_label(self) -> &'static str {
        match self {
            Action::TogglePlay => "Space",
            Action::ResetTime => "R",
            Action::StepBack => "←",
            Action::StepForward => "→",
            Action::ToggleFullscreen => "F11",
            Action::ExitFullscreen => "Esc",
            Action::ToggleControlsDetached => "D",
            Action::ToggleViewDetached => "V",
            Action::ToggleControlsHidden => "H",
            Action::ToggleSlicer => "S",
        }
    }

    /// Short description for help texts, e.g. `"fullscreen"`.
    pub fn label(self) -> &'static str {
        match self {
            Action::TogglePlay => "play/pause",
            Action::ResetTime => "reset time",
            Action::StepBack => "previous profile",
            Action::StepForward => "next profile",
            Action::ToggleFullscreen => "fullscreen",
            Action::ExitFullscreen => "exit fullscreen",
            Action::ToggleControlsDetached => "detach/dock controls",
            Action::ToggleViewDetached => "detach/dock view",
            Action::ToggleControlsHidden => "hide controls",
            Action::ToggleSlicer => "slicer",
        }
    }

    /// One-line help of all shortcuts: `"F11 fullscreen · Esc exit fullscreen · …"`.
    pub fn help() -> String {
        Action::ALL.iter().map(|a| format!("{} {}", a.key_label(), a.label())).collect::<Vec<_>>().join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Key, Modifiers};

    #[test]
    fn hiding_restores_previous_state() {
        let mut l = Layout { controls: DockState::Detached, ..Default::default() };
        l.toggle_controls_hidden();
        assert_eq!(l.controls, DockState::Hidden);
        l.toggle_controls_hidden();
        assert_eq!(l.controls, DockState::Detached);
    }

    #[test]
    fn detaching_view_leaves_fullscreen() {
        let mut l = Layout { fullscreen: true, ..Default::default() };
        assert!(l.toggle_view_detached());
        assert_eq!(l.view, ViewPlacement::Detached);
        assert!(!l.fullscreen);
        assert!(!l.toggle_view_detached());
        assert_eq!(l.view, ViewPlacement::Docked);
    }

    #[test]
    fn restored_never_hides_controls() {
        let mut l = Layout { controls: DockState::Detached, ..Default::default() };
        l.toggle_controls_hidden();
        assert_eq!(l.restored().controls, DockState::Detached);
    }

    #[test]
    fn keys_map_to_actions() {
        for a in Action::ALL {
            let key = Key::from_name(a.key_label()).or(match a {
                Action::StepBack => Some(Key::ArrowLeft),
                Action::StepForward => Some(Key::ArrowRight),
                _ => None,
            });
            assert_eq!(Action::from_key(key.unwrap(), Modifiers::NONE), Some(a), "{a:?}");
        }
        assert_eq!(Action::from_key(Key::S, Modifiers::CTRL), None);
        assert_eq!(Action::from_key(Key::V, Modifiers::SHIFT), Some(Action::ToggleViewDetached));
        assert_eq!(Action::from_key(Key::Q, Modifiers::NONE), None);
    }
}
