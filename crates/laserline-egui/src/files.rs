//! File access the UI needs, provided by the host application, so the UI itself never
//! touches the file system or a dialog library.

/// The kinds of files the UI opens and saves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileKind {
    /// STL model (`.stl`), open only.
    Stl,
    /// Sliced scan (`.lscan`).
    Scan,
    /// Laser parameters as JSON (`.json`).
    Preset,
}

impl FileKind {
    /// Filter name and extensions for file dialogs.
    pub fn filter(self) -> (&'static str, &'static [&'static str]) {
        match self {
            FileKind::Stl => ("STL model", &["stl", "STL"]),
            FileKind::Scan => ("Laser scan", &["lscan"]),
            FileKind::Preset => ("Laser preset", &["json"]),
        }
    }
}

/// A file the user picked.
#[derive(Clone, Debug)]
pub struct OpenedFile {
    /// File name, e.g. `part.stl`.
    pub name: String,
    /// Full path or URL, for messages.
    pub location: String,
    pub bytes: Vec<u8>,
}

pub trait FileHost {
    /// Asks the user for a file of `kind`; `Ok(None)` if they cancelled.
    fn open(&mut self, kind: FileKind) -> Result<Option<OpenedFile>, String>;

    /// Asks the user where to save `bytes`, suggesting `file_name`. Returns where the
    /// file went, or `Ok(None)` if they cancelled.
    fn save(&mut self, kind: FileKind, file_name: &str, bytes: &[u8]) -> Result<Option<String>, String>;
}
