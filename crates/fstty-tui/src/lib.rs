//! fstty-tui - TUI application for waveform analysis

pub mod app;
pub mod file_picker;

// Temporarily disabled for minimal TUI
// pub mod components;
// pub mod event;

pub use app::App;
pub use file_picker::FilePicker;
