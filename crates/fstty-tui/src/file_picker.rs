//! Simple file picker with extension filtering

use std::path::{Path, PathBuf};
use std::{fs, io};

use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Padding};

/// Valid waveform file extensions
const VALID_EXTENSIONS: &[&str] = &["fst", "vcd", "ghw"];

/// Entry in the file picker
#[derive(Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub is_dir: bool,
    pub name: String,
}

impl FileEntry {
    fn display_name(&self) -> String {
        if self.is_dir {
            format!("{}/", self.name)
        } else {
            self.name.clone()
        }
    }
}

/// File picker state
pub struct FilePicker {
    /// Current directory
    cwd: PathBuf,
    /// Entries in current directory
    entries: Vec<FileEntry>,
    /// Selection state
    list_state: ListState,
    /// Whether picker is active
    pub active: bool,
}

impl FilePicker {
    /// Create a new file picker starting at the given directory
    pub fn new(start_dir: impl AsRef<Path>) -> io::Result<Self> {
        let cwd = start_dir.as_ref().canonicalize()?;
        let mut picker = Self {
            cwd: cwd.clone(),
            entries: Vec::new(),
            list_state: ListState::default(),
            active: false,
        };
        picker.refresh()?;
        Ok(picker)
    }

    /// Open the picker
    pub fn open(&mut self) {
        self.active = true;
        self.list_state.select(Some(0));
    }

    /// Close the picker
    pub fn close(&mut self) {
        self.active = false;
    }

    /// Refresh the directory listing
    fn refresh(&mut self) -> io::Result<()> {
        let mut entries = Vec::new();

        // Add parent directory entry if not at root
        if let Some(parent) = self.cwd.parent() {
            entries.push(FileEntry {
                path: parent.to_path_buf(),
                is_dir: true,
                name: "..".to_string(),
            });
        }

        // Read directory contents
        let mut dir_entries: Vec<_> = fs::read_dir(&self.cwd)?
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let path = e.path();
                let is_dir = path.is_dir();
                let name = path.file_name()?.to_string_lossy().to_string();

                // Skip hidden files
                if name.starts_with('.') {
                    return None;
                }

                // For files, only show valid extensions
                if !is_dir {
                    let ext = path.extension()?.to_str()?.to_lowercase();
                    if !VALID_EXTENSIONS.contains(&ext.as_str()) {
                        return None;
                    }
                }

                Some(FileEntry { path, is_dir, name })
            })
            .collect();

        // Sort: directories first, then alphabetically
        dir_entries.sort_by(|a, b| {
            match (a.is_dir, b.is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
            }
        });

        entries.extend(dir_entries);
        self.entries = entries;

        // Reset selection if out of bounds
        if let Some(idx) = self.list_state.selected() {
            if idx >= self.entries.len() {
                self.list_state.select(Some(self.entries.len().saturating_sub(1)));
            }
        }

        Ok(())
    }

    /// Move selection up
    pub fn up(&mut self) {
        if let Some(idx) = self.list_state.selected() {
            if idx > 0 {
                self.list_state.select(Some(idx - 1));
            }
        }
    }

    /// Move selection down
    pub fn down(&mut self) {
        if let Some(idx) = self.list_state.selected() {
            if idx + 1 < self.entries.len() {
                self.list_state.select(Some(idx + 1));
            }
        }
    }

    /// Select current entry - returns Some(path) if a file was selected
    pub fn select(&mut self) -> io::Result<Option<PathBuf>> {
        let idx = self.list_state.selected().unwrap_or(0);
        if let Some(entry) = self.entries.get(idx) {
            if entry.is_dir {
                // Navigate into directory
                self.cwd = entry.path.clone();
                self.refresh()?;
                self.list_state.select(Some(0));
                Ok(None)
            } else {
                // File selected
                Ok(Some(entry.path.clone()))
            }
        } else {
            Ok(None)
        }
    }

    /// Get current working directory
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// Render the file picker
    pub fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        // Calculate picker size (70% width, 80% height)
        let width = (area.width * 70 / 100).max(40).min(area.width - 4);
        let height = (area.height * 80 / 100).max(10).min(area.height - 4);

        let picker_area = Rect {
            x: (area.width - width) / 2,
            y: (area.height - height) / 2,
            width,
            height,
        };

        // Clear background
        frame.render_widget(Clear, picker_area);

        // Build list items
        let items: Vec<ListItem> = self.entries
            .iter()
            .map(|e| {
                let style = if e.is_dir {
                    Style::default().fg(Color::Blue).bold()
                } else {
                    Style::default()
                };
                ListItem::new(e.display_name()).style(style)
            })
            .collect();

        let list = List::new(items)
            .block(
                Block::default()
                    .title(format!(" {} ", self.cwd.display()))
                    .borders(Borders::ALL)
                    .padding(Padding::horizontal(1))
            )
            .highlight_style(Style::default().reversed())
            .highlight_symbol("> ");

        frame.render_stateful_widget(list, picker_area, &mut self.list_state);
    }
}
