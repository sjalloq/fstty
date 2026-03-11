//! Application state and lifecycle

use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{Event, EventStream, KeyCode, KeyEventKind};
use futures::StreamExt;
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph};
use tokio::sync::mpsc;

use fstty_core::fst::ExportConfig;
use fstty_core::{FstSource, WaveformSource};

use crate::export_state::ExportState;
use crate::file_picker::FilePicker;
use crate::hierarchy_browser::{
    HierarchyBrowser, NodeId, SelectionMode, ToggleResult, ALL_SCOPE_TYPES,
};

/// Result of an async waveform load
type LoadResult = std::result::Result<FstSource, String>;

/// Available tabs/tools
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Browse,
    Export,
}

impl Tab {
    pub const ALL: &'static [Tab] = &[Tab::Browse, Tab::Export];

    pub fn label(&self) -> &'static str {
        match self {
            Tab::Browse => "Browse",
            Tab::Export => "Export",
        }
    }

    pub fn index(&self) -> usize {
        match self {
            Tab::Browse => 0,
            Tab::Export => 1,
        }
    }

    pub fn from_index(idx: usize) -> Self {
        match idx {
            0 => Tab::Browse,
            1 => Tab::Export,
            _ => Tab::Browse,
        }
    }
}

/// Popup message level
#[derive(Clone)]
pub enum PopupLevel {
    Info,
    Warning,
    Error,
}

/// Spinner for busy indication
pub struct Spinner {
    frames: &'static [&'static str],
    current: usize,
    last_update: Instant,
    interval: Duration,
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

impl Spinner {
    pub fn new() -> Self {
        Self {
            // Braille spinner - smooth and subtle
            frames: &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"],
            current: 0,
            last_update: Instant::now(),
            interval: Duration::from_millis(80),
        }
    }

    /// Advance the spinner if enough time has passed
    pub fn tick(&mut self) {
        if self.last_update.elapsed() >= self.interval {
            self.current = (self.current + 1) % self.frames.len();
            self.last_update = Instant::now();
        }
    }

    /// Get current spinner frame
    pub fn frame(&self) -> &'static str {
        self.frames[self.current]
    }
}

/// Popup message to display
#[derive(Clone)]
pub struct Popup {
    pub title: String,
    pub message: String,
    pub level: PopupLevel,
    pub expires_at: Option<Instant>,
}

/// Filter popup state
pub struct FilterPopup {
    /// Is the popup active
    pub active: bool,
    /// Currently selected item index
    pub selected: usize,
}

impl Default for FilterPopup {
    fn default() -> Self {
        Self::new()
    }
}

impl FilterPopup {
    pub fn new() -> Self {
        Self {
            active: false,
            selected: 0,
        }
    }

    pub fn open(&mut self) {
        self.active = true;
        self.selected = 0;
    }

    pub fn close(&mut self) {
        self.active = false;
    }

    pub fn up(&mut self) {
        if self.selected > 0 {
            self.selected -= 1;
        }
    }

    pub fn down(&mut self) {
        // +3 for "All", "None", "Default" options at the end
        if self.selected < ALL_SCOPE_TYPES.len() + 2 {
            self.selected += 1;
        }
    }
}

/// Main application state
pub struct App {
    /// Should quit
    exit: bool,
    /// Popup message (dismisses on any key)
    popup: Option<Popup>,
    /// File picker
    file_picker: FilePicker,
    /// Filter configuration popup
    filter_popup: FilterPopup,
    /// Hierarchy browser for Browse tab
    hierarchy_browser: HierarchyBrowser,
    /// Currently loaded file path
    loaded_file: Option<PathBuf>,
    /// Loaded waveform source
    waveform: Option<FstSource>,
    /// Export tab state (VC block selection)
    export_state: Option<ExportState>,
    /// Channel sender to trigger async loads
    load_tx: mpsc::Sender<PathBuf>,
    /// Channel receiver for completed loads
    load_rx: mpsc::Receiver<LoadResult>,
    /// Busy spinner
    spinner: Spinner,
    /// Current busy status message (None = not busy)
    busy_status: Option<String>,
    /// Active tab
    active_tab: Tab,
}

impl App {
    /// Create a new application
    pub fn new() -> Result<Self> {
        let file_picker = FilePicker::new(".")?;

        // Channel for requesting loads (UI -> loader task)
        let (request_tx, mut request_rx) = mpsc::channel::<PathBuf>(1);
        // Channel for receiving results (loader task -> UI)
        let (result_tx, result_rx) = mpsc::channel::<LoadResult>(1);

        // Spawn the loader task
        tokio::spawn(async move {
            while let Some(path) = request_rx.recv().await {
                // Run blocking waveform load on the blocking thread pool
                let result = tokio::task::spawn_blocking(move || {
                    FstSource::open(&path)
                        .map_err(|e| e.to_string())
                }).await;

                // Handle join error and send result
                let load_result = match result {
                    Ok(r) => r,
                    Err(e) => Err(format!("Load task panicked: {}", e)),
                };

                // Send result back (ignore error if receiver dropped)
                let _ = result_tx.send(load_result).await;
            }
        });

        Ok(Self {
            exit: false,
            popup: None,
            file_picker,
            filter_popup: FilterPopup::new(),
            hierarchy_browser: HierarchyBrowser::new(),
            loaded_file: None,
            waveform: None,
            export_state: None,
            load_tx: request_tx,
            load_rx: result_rx,
            spinner: Spinner::new(),
            busy_status: None,
            active_tab: Tab::default(),
        })
    }

    /// Switch to next tab
    fn next_tab(&mut self) {
        let idx = (self.active_tab.index() + 1) % Tab::ALL.len();
        self.active_tab = Tab::from_index(idx);
    }

    /// Switch to previous tab
    fn prev_tab(&mut self) {
        let idx = if self.active_tab.index() == 0 {
            Tab::ALL.len() - 1
        } else {
            self.active_tab.index() - 1
        };
        self.active_tab = Tab::from_index(idx);
    }

    /// Set busy status (shows spinner)
    pub fn set_busy(&mut self, status: impl Into<String>) {
        self.busy_status = Some(status.into());
    }

    /// Clear busy status
    pub fn clear_busy(&mut self) {
        self.busy_status = None;
    }

    /// Set loaded file (for testing/screenshots)
    pub fn set_loaded_file(&mut self, path: PathBuf) {
        self.loaded_file = Some(path);
    }

    /// Load a waveform file (public entry point for CLI usage)
    pub fn load_file(&mut self, path: PathBuf) {
        self.start_load(path);
    }

    /// Start async loading of a waveform file
    fn start_load(&mut self, path: PathBuf) {
        self.loaded_file = Some(path.clone());
        self.set_busy("Loading hierarchy...");
        // Send to loader task (non-blocking, will fail if channel full)
        let _ = self.load_tx.try_send(path);
    }

    /// Handle a completed load result
    fn handle_load_result(&mut self, result: LoadResult) {
        self.clear_busy();
        match result {
            Ok(waveform) => {
                let blocks = waveform.block_infos();
                self.export_state = Some(ExportState::new(blocks));
                self.waveform = Some(waveform);
                self.hierarchy_browser.reset();
                // No toast - the loaded file in title bar is sufficient feedback
            }
            Err(e) => {
                self.loaded_file = None;
                self.waveform = None;
                self.export_state = None;
                self.hierarchy_browser.reset();
                self.show_error("Load Error", e);
            }
        }
    }

    /// Set active tab by name or number (for testing/screenshots)
    pub fn set_tab(&mut self, tab: &str) {
        self.active_tab = match tab.to_lowercase().as_str() {
            "1" | "browse" => Tab::Browse,
            "2" | "export" => Tab::Export,
            _ => Tab::Browse,
        };
    }

    /// Show an info popup
    pub fn show_info(&mut self, title: impl Into<String>, message: impl Into<String>) {
        self.popup = Some(Popup {
            title: title.into(),
            message: message.into(),
            level: PopupLevel::Info,
            expires_at: None,
        });
    }

    /// Show a warning popup
    pub fn show_warning(&mut self, title: impl Into<String>, message: impl Into<String>) {
        self.popup = Some(Popup {
            title: title.into(),
            message: message.into(),
            level: PopupLevel::Warning,
            expires_at: None,
        });
    }

    /// Show an error popup
    pub fn show_error(&mut self, title: impl Into<String>, message: impl Into<String>) {
        self.popup = Some(Popup {
            title: title.into(),
            message: message.into(),
            level: PopupLevel::Error,
            expires_at: None,
        });
    }

    /// Show a popup that auto-dismisses after a duration
    pub fn show_toast(&mut self, title: impl Into<String>, message: impl Into<String>, duration: Duration) {
        self.popup = Some(Popup {
            title: title.into(),
            message: message.into(),
            level: PopupLevel::Info,
            expires_at: Some(Instant::now() + duration),
        });
    }

    /// Run the application main loop
    pub async fn run(&mut self) -> Result<()> {
        let mut terminal = ratatui::init();
        let mut event_stream = EventStream::new();

        // Tick interval for spinner animation
        let mut tick_interval = tokio::time::interval(Duration::from_millis(80));

        while !self.exit {
            // Check for expired popups
            if let Some(ref popup) = self.popup {
                if let Some(expires_at) = popup.expires_at {
                    if Instant::now() >= expires_at {
                        self.popup = None;
                    }
                }
            }

            // Draw current state
            terminal.draw(|frame| self.render(frame))?;

            // Wait for next event using select!
            tokio::select! {
                // Keyboard/terminal events
                maybe_event = event_stream.next() => {
                    if let Some(Ok(Event::Key(key))) = maybe_event {
                        if key.kind == KeyEventKind::Press {
                            self.handle_key(key.code);
                        }
                    }
                }

                // Tick for spinner animation
                _ = tick_interval.tick() => {
                    if self.busy_status.is_some() {
                        self.spinner.tick();
                    }
                }

                // Load results from background task
                Some(result) = self.load_rx.recv() => {
                    self.handle_load_result(result);
                }
            }
        }

        ratatui::restore();
        Ok(())
    }

    /// Render a single frame to string (for screenshots/testing)
    pub fn screenshot(&mut self, width: u16, height: u16) -> String {
        use ratatui::backend::TestBackend;
        use ratatui::Terminal;

        let backend = TestBackend::new(width, height);
        let mut terminal = Terminal::new(backend).unwrap();

        terminal.draw(|frame| self.render(frame)).unwrap();

        // Convert buffer to string
        let buffer = terminal.backend().buffer().clone();
        let mut output = String::new();

        for y in 0..height {
            for x in 0..width {
                let cell = buffer.cell((x, y)).unwrap();
                output.push_str(cell.symbol());
            }
            output.push('\n');
        }

        output
    }

    /// Save screenshot to file with timestamp
    fn save_screenshot(&mut self) {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        // Get terminal size or use default
        let (width, height) = crossterm::terminal::size().unwrap_or((80, 24));
        let content = self.screenshot(width, height);

        // Generate filename with timestamp
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let filename = format!("fstty-screenshot-{}.txt", timestamp);

        match fs::write(&filename, &content) {
            Ok(_) => self.show_toast("Screenshot", format!("Saved to {}", filename), Duration::from_secs(2)),
            Err(e) => self.show_error("Screenshot", format!("Failed to save: {}", e)),
        }
    }

    /// Handle a key press
    fn handle_key(&mut self, code: KeyCode) {
        // Screenshot with Shift-S (uppercase S only)
        if matches!(code, KeyCode::Char('S')) {
            self.save_screenshot();
            return;
        }

        // File picker has priority when active
        if self.file_picker.active {
            match code {
                KeyCode::Esc => {
                    self.file_picker.close();
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.file_picker.up();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.file_picker.down();
                }
                KeyCode::Enter | KeyCode::Char('l') => {
                    match self.file_picker.select() {
                        Ok(Some(path)) => {
                            self.file_picker.close();
                            self.start_load(path);
                        }
                        Ok(None) => {} // Navigated into directory
                        Err(e) => {
                            self.show_error("Error", format!("Failed to open: {}", e));
                        }
                    }
                }
                KeyCode::Backspace | KeyCode::Char('h') => {
                    // Go to parent directory
                    if let Err(e) = self.file_picker.select() {
                        self.show_error("Error", format!("{}", e));
                    }
                }
                _ => {}
            }
            return;
        }

        // Filter popup has priority when active
        if self.filter_popup.active {
            match code {
                KeyCode::Esc => {
                    self.filter_popup.close();
                }
                KeyCode::Up | KeyCode::Char('k') => {
                    self.filter_popup.up();
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.filter_popup.down();
                }
                KeyCode::Enter | KeyCode::Char(' ') => {
                    // Toggle selected item
                    let idx = self.filter_popup.selected;
                    if idx < ALL_SCOPE_TYPES.len() {
                        // Toggle a scope type
                        let (scope_type, _, _) = ALL_SCOPE_TYPES[idx];
                        self.hierarchy_browser.filter_mut().toggle_scope_type(scope_type);
                    } else {
                        // Special actions: All, None, Default
                        match idx - ALL_SCOPE_TYPES.len() {
                            0 => self.hierarchy_browser.filter_mut().enable_all_scopes(),
                            1 => self.hierarchy_browser.filter_mut().disable_all_scopes(),
                            2 => self.hierarchy_browser.filter_mut().reset_to_default(),
                            _ => {}
                        }
                    }
                }
                _ => {}
            }
            return;
        }

        // If popup is showing, only Esc dismisses it
        if self.popup.is_some() {
            if code == KeyCode::Esc {
                self.popup = None;
            }
            return;
        }

        // Normal key handling
        match code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.exit = true;
            }
            KeyCode::Char('o') | KeyCode::Char('O') => {
                self.file_picker.open();
            }
            KeyCode::Tab => {
                self.next_tab();
            }
            KeyCode::BackTab => {
                self.prev_tab();
            }
            KeyCode::Char('1') => self.active_tab = Tab::Browse,
            KeyCode::Char('2') => self.active_tab = Tab::Export,
            // Hierarchy browser navigation (when on Browse tab with a file loaded)
            KeyCode::Up | KeyCode::Char('k') if self.active_tab == Tab::Browse && self.waveform.is_some() => {
                self.hierarchy_browser.up();
            }
            KeyCode::Down | KeyCode::Char('j') if self.active_tab == Tab::Browse && self.waveform.is_some() => {
                self.hierarchy_browser.down();
            }
            KeyCode::Left | KeyCode::Char('h') if self.active_tab == Tab::Browse && self.waveform.is_some() => {
                self.hierarchy_browser.left();
            }
            KeyCode::Right | KeyCode::Char('l') if self.active_tab == Tab::Browse && self.waveform.is_some() => {
                self.hierarchy_browser.right();
            }
            KeyCode::Enter if self.active_tab == Tab::Browse && self.waveform.is_some() => {
                self.hierarchy_browser.toggle();
            }
            // Toggle signal visibility for current scope
            KeyCode::Char('s') if self.active_tab == Tab::Browse && self.waveform.is_some() => {
                if let Some(showing) = self.hierarchy_browser.toggle_show_signals() {
                    let msg = if showing { "Signals shown" } else { "Signals hidden" };
                    self.show_toast("", msg, Duration::from_secs(1));
                }
            }
            // Open filter configuration popup
            KeyCode::Char('f') if self.active_tab == Tab::Browse => {
                self.filter_popup.open();
            }
            // Rebuild tree with current filter (Shift-R)
            KeyCode::Char('R') if self.active_tab == Tab::Browse && self.waveform.is_some() => {
                self.hierarchy_browser.rebuild();
                self.show_toast("", "Tree rebuilt", Duration::from_secs(1));
            }
            // Export tab: cursor movement
            KeyCode::Left | KeyCode::Char('h') if self.active_tab == Tab::Export && self.export_state.is_some() => {
                self.export_state.as_mut().unwrap().move_cursor_left();
            }
            KeyCode::Right | KeyCode::Char('l') if self.active_tab == Tab::Export && self.export_state.is_some() => {
                self.export_state.as_mut().unwrap().move_cursor_right();
            }
            // Export tab: mark start/end of range
            KeyCode::Enter if self.active_tab == Tab::Export && self.export_state.is_some() => {
                let es = self.export_state.as_mut().unwrap();
                es.mark();
                if es.has_valid_range() {
                    if let Some((t0, t1)) = es.selected_time_range() {
                        self.show_toast("", format!("Range: {}..{}", t0, t1), Duration::from_secs(2));
                    }
                }
            }
            // Export tab: clear selection
            KeyCode::Esc if self.active_tab == Tab::Export && self.export_state.as_ref().is_some_and(|es| es.anchor().is_some()) => {
                self.export_state.as_mut().unwrap().clear_selection();
            }
            // Export tab: execute export (x key)
            KeyCode::Char('x') if self.active_tab == Tab::Export => {
                self.run_export();
            }
            // Toggle selection of current item (Space)
            KeyCode::Char(' ') if self.active_tab == Tab::Browse && self.waveform.is_some() => {
                let result = self.hierarchy_browser.toggle_selection();
                let count = self.hierarchy_browser.selection_count();
                let msg = match result {
                    ToggleResult::Selected(SelectionMode::Recursive) => {
                        Some(format!("Selected (recursive) ({} total)", count))
                    }
                    ToggleResult::Selected(SelectionMode::ScopeOnly) => {
                        Some(format!("Selected (scope only) ({} total)", count))
                    }
                    ToggleResult::Deselected => {
                        Some(format!("Deselected ({} total)", count))
                    }
                    ToggleResult::NoSelection => None,
                };
                if let Some(msg) = msg {
                    self.show_toast("", msg, Duration::from_secs(1));
                }
            }
            _ => {}
        }
    }

    /// Collect selected signal IDs from the hierarchy browser.
    fn selected_signal_ids(&self) -> Vec<fstty_core::types::SignalId> {
        let waveform = match self.waveform.as_ref() {
            Some(w) => w,
            None => return vec![],
        };
        let hierarchy = waveform.hierarchy();
        let signals = collect_selected_signals(self.hierarchy_browser.selected_nodes(), hierarchy);
        signals.into_iter().collect()
    }

    /// Run the filtered export.
    fn run_export(&mut self) {
        let export_state = match self.export_state.as_ref() {
            Some(es) => es,
            None => {
                self.show_warning("Export", "No file loaded");
                return;
            }
        };

        if !export_state.has_valid_range() {
            self.show_warning("Export", "Select a block range first (Enter to mark start/end)");
            return;
        }

        let signal_ids = self.selected_signal_ids();
        if signal_ids.is_empty() {
            self.show_warning("Export", "No signals selected. Use Space in Browse tab to select.");
            return;
        }

        let block_range = export_state.selected_range();

        // Build output filename from source
        let output_path = match self.loaded_file.as_ref() {
            Some(p) => {
                let stem = p.file_stem().unwrap_or_default().to_string_lossy();
                p.with_file_name(format!("{}_filtered.fst", stem))
            }
            None => PathBuf::from("filtered.fst"),
        };

        let config = ExportConfig {
            output_path: output_path.clone(),
            signals: signal_ids,
            block_range,
        };

        match self.waveform.as_mut().unwrap().export_filtered(&config) {
            Ok(result) => {
                let msg = format!(
                    "Exported {} signals, {} blocks to {}",
                    result.signal_count,
                    result.block_count,
                    result.output_path.display()
                );
                self.show_info("Export Complete", msg);
            }
            Err(e) => {
                self.show_error("Export Error", format!("{}", e));
            }
        }
    }

    /// Render the application
    fn render(&mut self, frame: &mut Frame) {
        let area = frame.area();

        // Layout: title bar (2 rows) + tabs + main area + footer
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Title bar + separator
                Constraint::Length(1), // Tab bar
                Constraint::Min(1),    // Main content
                Constraint::Length(1), // Footer
            ])
            .split(area);

        // Title bar: "fstty" left, status right
        self.render_title_bar(frame, chunks[0]);

        // Tab bar
        self.render_tabs(frame, chunks[1]);

        // Main content area - depends on active tab
        self.render_tab_content(frame, chunks[2]);

        // Footer with key hints
        let footer_text = match self.active_tab {
            Tab::Browse => " q: quit | o: open | f: filter | s: signals | Space: select | R: rebuild",
            Tab::Export => " q: quit | o: open | Enter: mark | Esc: clear | x: export",
        };
        let footer = Paragraph::new(footer_text)
            .style(Style::default().reversed());
        frame.render_widget(footer, chunks[3]);

        // Render file picker on top if active
        if self.file_picker.active {
            self.file_picker.render(frame);
        }

        // Render filter popup on top if active
        if self.filter_popup.active {
            self.render_filter_popup(frame);
        }

        // Render popup on top if present
        if let Some(ref popup) = self.popup {
            self.render_popup(frame, popup);
        }
    }

    /// Render tab bar
    fn render_tabs(&self, frame: &mut Frame, area: Rect) {
        let mut spans = Vec::new();
        spans.push(Span::raw(" "));

        for (i, tab) in Tab::ALL.iter().enumerate() {
            let label = format!(" {} ", tab.label());
            let style = if *tab == self.active_tab {
                Style::default().bold().reversed()
            } else {
                Style::default().fg(Color::DarkGray)
            };
            spans.push(Span::styled(label, style));

            // Add separator between tabs
            if i < Tab::ALL.len() - 1 {
                spans.push(Span::raw(" "));
            }
        }

        let tabs = Paragraph::new(Line::from(spans));
        frame.render_widget(tabs, area);
    }

    /// Render content for the active tab
    fn render_tab_content(&mut self, frame: &mut Frame, area: Rect) {
        match self.active_tab {
            Tab::Browse => self.render_browse_tab(frame, area),
            Tab::Export => self.render_export_tab(frame, area),
        }
    }

    /// Render the Browse tab with hierarchy tree
    fn render_browse_tab(&mut self, frame: &mut Frame, area: Rect) {
        if let Some(ref waveform) = self.waveform {
            let hierarchy = waveform.hierarchy();
            let block = Block::default()
                .borders(Borders::ALL)
                .padding(Padding::new(2, 2, 1, 1)); // left, right, top, bottom
            self.hierarchy_browser.render(frame, area, hierarchy, block);
        } else {
            let block = Block::default()
                .borders(Borders::ALL)
                .padding(Padding::horizontal(2));
            let inner = block.inner(area);
            frame.render_widget(block, area);
            let paragraph = Paragraph::new("No file loaded. Press 'o' to open.")
                .alignment(Alignment::Center);
            frame.render_widget(paragraph, inner);
        }
    }

    /// Render the Export tab with block timeline
    fn render_export_tab(&self, frame: &mut Frame, area: Rect) {
        let block = Block::default().borders(Borders::ALL).title(" Export ");
        let inner = block.inner(area);
        frame.render_widget(block, area);

        let export_state = match self.export_state.as_ref() {
            Some(es) if es.block_count() > 0 => es,
            _ => {
                let msg = Paragraph::new("No file loaded. Press 'o' to open.")
                    .alignment(Alignment::Center);
                frame.render_widget(msg, inner);
                return;
            }
        };

        // Layout: info line + timeline + status/help
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Length(2), // Info
                Constraint::Length(3), // Block timeline
                Constraint::Length(2), // Selection details
                Constraint::Min(0),   // Spacer
                Constraint::Length(1), // Help line
            ])
            .split(inner);

        // Info line: signal selection count and block count
        let sel_count = self.hierarchy_browser.selection_count();
        let info = format!(
            " {} VC blocks | {} signals selected for export",
            export_state.block_count(),
            sel_count,
        );
        frame.render_widget(Paragraph::new(info), chunks[0]);

        // Block timeline: a horizontal bar of blocks
        self.render_block_timeline(frame, chunks[1], export_state);

        // Selection details
        if let Some((t0, t1)) = export_state.selected_time_range() {
            let range = export_state.selected_range().unwrap();
            let detail = format!(
                " Selected blocks {}-{} | Time: {}..{}",
                range.start, range.end - 1, t0, t1,
            );
            frame.render_widget(
                Paragraph::new(detail).style(Style::default().fg(Color::Green)),
                chunks[2],
            );
        } else if let Some(cursor_block) = export_state.block(export_state.cursor()) {
            let detail = format!(
                " Cursor: block {} | Time: {}..{}",
                cursor_block.index, cursor_block.start_time, cursor_block.end_time,
            );
            frame.render_widget(
                Paragraph::new(detail).style(Style::default().fg(Color::DarkGray)),
                chunks[2],
            );
        }

        // Help line
        let help = if export_state.has_valid_range() {
            " x: export | Esc: clear selection"
        } else if export_state.anchor().is_some() {
            " Left/Right: move cursor | Enter: set end | Esc: clear"
        } else {
            " Left/Right: move cursor | Enter: set start"
        };
        frame.render_widget(
            Paragraph::new(help).style(Style::default().fg(Color::DarkGray)),
            chunks[4],
        );
    }

    /// Render the block timeline bar.
    fn render_block_timeline(&self, frame: &mut Frame, area: Rect, export_state: &ExportState) {
        if area.width < 2 || area.height < 1 {
            return;
        }

        let block_count = export_state.block_count();
        let available_width = area.width as usize;

        // Each block gets at least 1 column; if there are more blocks than columns,
        // we show a windowed view centered on the cursor.
        let (start_block, end_block) = if block_count <= available_width {
            (0, block_count)
        } else {
            // Window centered on cursor
            let half = available_width / 2;
            let cursor = export_state.cursor();
            let start = cursor.saturating_sub(half);
            let end = (start + available_width).min(block_count);
            let start = end.saturating_sub(available_width);
            (start, end)
        };

        let highlight = export_state.highlighted_range();

        let mut spans = Vec::new();
        for i in start_block..end_block {
            let is_cursor = i == export_state.cursor();
            let is_highlighted = highlight.as_ref().is_some_and(|r| r.contains(&i));

            let style = if is_cursor && is_highlighted {
                Style::default().fg(Color::Black).bg(Color::Cyan).bold()
            } else if is_cursor {
                Style::default().fg(Color::Black).bg(Color::White).bold()
            } else if is_highlighted {
                Style::default().fg(Color::Black).bg(Color::Blue)
            } else {
                Style::default().fg(Color::DarkGray)
            };

            let ch = if is_cursor { "█" } else if is_highlighted { "▓" } else { "░" };
            spans.push(Span::styled(ch, style));
        }

        // Show range indicators on a second line
        let label_line = if block_count <= available_width {
            let mut chars: Vec<char> = vec![' '; available_width];
            // Mark anchor
            if let Some(a) = export_state.anchor() {
                if a >= start_block && a < end_block {
                    chars[a - start_block] = '▲';
                }
            }
            // Mark cursor
            let c = export_state.cursor();
            if c >= start_block && c < end_block {
                chars[c - start_block] = '▲';
            }
            chars.iter().collect::<String>()
        } else {
            format!(
                " blocks {}-{} of {} (cursor: {})",
                start_block,
                end_block - 1,
                block_count,
                export_state.cursor(),
            )
        };

        let timeline = Paragraph::new(vec![
            Line::from(spans),
            Line::from(Span::styled(label_line, Style::default().fg(Color::DarkGray))),
        ]);
        frame.render_widget(timeline, area);
    }

    /// Render title bar with app name and status
    fn render_title_bar(&self, frame: &mut Frame, area: Rect) {
        // Need 2 rows: content + border line
        if area.height < 2 {
            return;
        }

        let content_area = Rect { height: 1, ..area };
        let border_area = Rect { y: area.y + 1, height: 1, ..area };

        // Left side: "fstty" + optional " : filename"
        let (title_base, title_file) = if let Some(ref path) = self.loaded_file {
            let filename = path.file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| path.display().to_string());
            (" fstty : ".to_string(), Some(filename))
        } else {
            (" fstty".to_string(), None)
        };

        // Right side: spinner + status message OR selection count
        let status = if let Some(ref busy_msg) = self.busy_status {
            format!("{} {} ", self.spinner.frame(), busy_msg)
        } else {
            let count = self.hierarchy_browser.selection_count();
            if count > 0 {
                format!("[{} selected] ", count)
            } else {
                String::new()
            }
        };

        // Calculate widths
        let title_width = title_base.len() as u16 + title_file.as_ref().map(|f| f.len() as u16).unwrap_or(0);
        let status_width = status.len() as u16;
        let padding_width = area.width.saturating_sub(title_width + status_width);

        // Build the line with spans
        let mut spans = vec![
            Span::styled(title_base, Style::default().bold()),
        ];
        if let Some(ref filename) = title_file {
            spans.push(Span::styled(filename.clone(), Style::default().fg(Color::DarkGray)));
        }
        spans.push(Span::raw(" ".repeat(padding_width as usize)));
        spans.push(Span::styled(status, Style::default().fg(Color::Cyan)));

        let line = Line::from(spans);

        let title_content = Paragraph::new(line);
        frame.render_widget(title_content, content_area);

        // Draw horizontal line underneath
        let border_line = "─".repeat(area.width as usize);
        let border = Paragraph::new(border_line)
            .style(Style::default().fg(Color::DarkGray));
        frame.render_widget(border, border_area);
    }

    /// Render the filter configuration popup
    fn render_filter_popup(&self, frame: &mut Frame) {
        let area = frame.area();

        // Calculate popup size - needs to fit all scope types + actions
        let popup_width = 50.min(area.width - 4);
        let popup_height = (ALL_SCOPE_TYPES.len() as u16 + 7).min(area.height - 4); // +7 for header, separator, actions, border

        // Center the popup
        let popup_area = Rect {
            x: (area.width - popup_width) / 2,
            y: (area.height - popup_height) / 2,
            width: popup_width,
            height: popup_height,
        };

        // Clear the area behind the popup
        frame.render_widget(Clear, popup_area);

        // Create the popup block
        let block = Block::default()
            .title(" Filter Config ")
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Cyan));

        let inner = block.inner(popup_area);
        frame.render_widget(block, popup_area);

        // Build the list of items
        let mut lines: Vec<Line> = Vec::new();

        // Header
        lines.push(Line::from(Span::styled(
            "Scope Types (Space/Enter to toggle):",
            Style::default().bold(),
        )));
        lines.push(Line::from(""));

        // Scope types with checkboxes
        for (i, (scope_type, name, _desc)) in ALL_SCOPE_TYPES.iter().enumerate() {
            let is_enabled = self.hierarchy_browser.filter().is_scope_enabled(*scope_type);
            let checkbox = if is_enabled { "[x]" } else { "[ ]" };
            let is_selected = self.filter_popup.selected == i;

            let style = if is_selected {
                Style::default().reversed()
            } else {
                Style::default()
            };

            lines.push(Line::from(Span::styled(
                format!(" {} {}", checkbox, name),
                style,
            )));
        }

        // Separator
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("─".repeat(inner.width as usize - 2), Style::default().fg(Color::DarkGray))));

        // Action buttons
        let actions = ["Enable All", "Disable All", "Reset Default"];
        for (i, action) in actions.iter().enumerate() {
            let idx = ALL_SCOPE_TYPES.len() + i;
            let is_selected = self.filter_popup.selected == idx;

            let style = if is_selected {
                Style::default().reversed()
            } else {
                Style::default().fg(Color::Yellow)
            };

            lines.push(Line::from(Span::styled(format!(" > {}", action), style)));
        }

        // Footer hint
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled(
            " Esc: close | R: rebuild tree",
            Style::default().fg(Color::DarkGray),
        )));

        let paragraph = Paragraph::new(lines);
        frame.render_widget(paragraph, inner);
    }

    /// Render a centered popup
    fn render_popup(&self, frame: &mut Frame, popup: &Popup) {
        let area = frame.area();

        // Style based on level
        let (border_style, title_prefix) = match popup.level {
            PopupLevel::Info => (Style::default(), ""),
            PopupLevel::Warning => (Style::default().fg(Color::Yellow), "⚠ "),
            PopupLevel::Error => (Style::default().fg(Color::Red), "✗ "),
        };

        // Calculate popup size based on content
        let has_title = !popup.title.is_empty();
        let content_width = popup.message.len() + 4; // message + padding
        let popup_width = (content_width as u16).max(20).min(area.width - 4);
        let popup_height = if has_title { 5 } else { 3 }; // compact for toasts

        // Center the popup
        let popup_area = Rect {
            x: (area.width - popup_width) / 2,
            y: (area.height - popup_height) / 2,
            width: popup_width,
            height: popup_height,
        };

        // Clear the area behind the popup
        frame.render_widget(Clear, popup_area);

        // Create the popup block - only add title if non-empty
        let mut block = Block::default()
            .borders(Borders::ALL)
            .border_style(border_style);

        if has_title {
            block = block.title(format!(" {}{} ", title_prefix, popup.title));
        }

        // Create the message paragraph
        let message = Paragraph::new(popup.message.as_str())
            .alignment(Alignment::Center)
            .block(block);

        frame.render_widget(message, popup_area);
    }
}

/// Collect signal IDs from a set of selected nodes, respecting selection modes.
/// - `Var` + any mode → add that signal
/// - `Scope` + `Recursive` → all descendant signals
/// - `Scope` + `ScopeOnly` → only direct vars
fn collect_selected_signals(
    selections: &std::collections::HashMap<NodeId, SelectionMode>,
    hierarchy: &fstty_core::hierarchy::Hierarchy,
) -> Vec<fstty_core::types::SignalId> {
    use std::collections::HashSet;
    let mut signal_set = HashSet::new();
    for (node_id, mode) in selections {
        match node_id {
            NodeId::Var(var_id) => {
                signal_set.insert(hierarchy.var_signal_id(*var_id));
            }
            NodeId::Scope(scope_id) => match mode {
                SelectionMode::Recursive => {
                    collect_scope_signals_recursive(hierarchy, *scope_id, &mut signal_set);
                }
                SelectionMode::ScopeOnly => {
                    for &var_id in hierarchy.scope_vars(*scope_id) {
                        signal_set.insert(hierarchy.var_signal_id(var_id));
                    }
                }
            },
            NodeId::Root => {}
        }
    }
    signal_set.into_iter().collect()
}

/// Recursively collect all signal IDs under a scope (including nested child scopes).
fn collect_scope_signals_recursive(
    hierarchy: &fstty_core::hierarchy::Hierarchy,
    scope_id: fstty_core::types::ScopeId,
    signal_set: &mut std::collections::HashSet<fstty_core::types::SignalId>,
) {
    for &var_id in hierarchy.scope_vars(scope_id) {
        signal_set.insert(hierarchy.var_signal_id(var_id));
    }
    for &child_id in hierarchy.scope_children(scope_id) {
        collect_scope_signals_recursive(hierarchy, child_id, signal_set);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use fstty_core::hierarchy::{HierarchyBuilder, HierarchyEvent};
    use fstty_core::types::{SignalId, VarDirection, VarType};

    /// Build a test hierarchy:
    ///   top (Module)              — scope 0
    ///     ├── var_a  (signal 1)   — var 0
    ///     ├── var_b  (signal 2)   — var 1
    ///     └── child (Module)      — scope 1
    ///           ├── var_c  (signal 3) — var 2
    ///           └── var_d  (signal 4) — var 3
    fn test_hierarchy() -> fstty_core::hierarchy::Hierarchy {
        let mut b = HierarchyBuilder::new();
        b.event(HierarchyEvent::EnterScope {
            name: "top".into(),
            scope_type: fstty_core::types::ScopeType::Module,
        });
        b.event(HierarchyEvent::Var {
            name: "var_a".into(),
            var_type: VarType::Wire,
            direction: VarDirection::Implicit,
            width: 1,
            signal_id: SignalId::from_raw(1),
            is_alias: false,
        });
        b.event(HierarchyEvent::Var {
            name: "var_b".into(),
            var_type: VarType::Wire,
            direction: VarDirection::Implicit,
            width: 1,
            signal_id: SignalId::from_raw(2),
            is_alias: false,
        });
        b.event(HierarchyEvent::EnterScope {
            name: "child".into(),
            scope_type: fstty_core::types::ScopeType::Module,
        });
        b.event(HierarchyEvent::Var {
            name: "var_c".into(),
            var_type: VarType::Wire,
            direction: VarDirection::Implicit,
            width: 1,
            signal_id: SignalId::from_raw(3),
            is_alias: false,
        });
        b.event(HierarchyEvent::Var {
            name: "var_d".into(),
            var_type: VarType::Wire,
            direction: VarDirection::Implicit,
            width: 1,
            signal_id: SignalId::from_raw(4),
            is_alias: false,
        });
        b.event(HierarchyEvent::ExitScope);
        b.event(HierarchyEvent::ExitScope);
        b.build()
    }

    #[test]
    fn scope_recursive_collects_all_descendants() {
        let h = test_hierarchy();
        let top = *h.top_scopes().first().unwrap();
        let mut sel = HashMap::new();
        sel.insert(NodeId::Scope(top), SelectionMode::Recursive);
        let signals = collect_selected_signals(&sel, &h);
        assert_eq!(signals.len(), 4);
    }

    #[test]
    fn scope_only_collects_direct_vars() {
        let h = test_hierarchy();
        let top = *h.top_scopes().first().unwrap();
        let mut sel = HashMap::new();
        sel.insert(NodeId::Scope(top), SelectionMode::ScopeOnly);
        let signals = collect_selected_signals(&sel, &h);
        // Only var_a and var_b (direct vars of top), not var_c/var_d in child
        assert_eq!(signals.len(), 2);
        // Verify they're the right signals
        let expected: std::collections::HashSet<_> = h.scope_vars(top).iter()
            .map(|&v| h.var_signal_id(v))
            .collect();
        let actual: std::collections::HashSet<_> = signals.into_iter().collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn var_selection_collects_that_signal() {
        let h = test_hierarchy();
        let top = *h.top_scopes().first().unwrap();
        let child = h.scope_children(top)[0];
        let var_c = h.scope_vars(child)[0];
        let expected_signal = h.var_signal_id(var_c);

        let mut sel = HashMap::new();
        sel.insert(NodeId::Var(var_c), SelectionMode::Recursive);
        let signals = collect_selected_signals(&sel, &h);
        assert_eq!(signals.len(), 1);
        assert_eq!(signals[0], expected_signal);
    }

    #[test]
    fn mixed_selections_deduplicates() {
        let h = test_hierarchy();
        let top = *h.top_scopes().first().unwrap();
        let var_a = h.scope_vars(top)[0];

        let mut sel = HashMap::new();
        // Recursive on top (gets all 4 signals)
        sel.insert(NodeId::Scope(top), SelectionMode::Recursive);
        // Also individually select var_a — should not duplicate
        sel.insert(NodeId::Var(var_a), SelectionMode::Recursive);
        let signals = collect_selected_signals(&sel, &h);
        assert_eq!(signals.len(), 4);
    }

    #[test]
    fn tab_all_contains_only_browse_and_export() {
        assert_eq!(Tab::ALL.len(), 2);
        assert_eq!(Tab::ALL[0], Tab::Browse);
        assert_eq!(Tab::ALL[1], Tab::Export);
    }

    #[test]
    fn tab_labels() {
        assert_eq!(Tab::Browse.label(), "Browse");
        assert_eq!(Tab::Export.label(), "Export");
    }

    #[test]
    fn tab_index_roundtrip() {
        for tab in Tab::ALL {
            assert_eq!(Tab::from_index(tab.index()), *tab);
        }
    }

    #[test]
    fn tab_from_index_out_of_bounds_defaults_to_browse() {
        assert_eq!(Tab::from_index(99), Tab::Browse);
    }

    #[test]
    fn tab_default_is_browse() {
        assert_eq!(Tab::default(), Tab::Browse);
    }

    #[test]
    fn tab_switching_next_wraps() {
        // Browse -> Export -> Browse
        let mut tab = Tab::Browse;
        tab = Tab::from_index((tab.index() + 1) % Tab::ALL.len());
        assert_eq!(tab, Tab::Export);
        tab = Tab::from_index((tab.index() + 1) % Tab::ALL.len());
        assert_eq!(tab, Tab::Browse);
    }

    #[test]
    fn tab_switching_prev_wraps() {
        // Browse -> Export (wrap around)
        let mut tab = Tab::Browse;
        let idx = if tab.index() == 0 {
            Tab::ALL.len() - 1
        } else {
            tab.index() - 1
        };
        tab = Tab::from_index(idx);
        assert_eq!(tab, Tab::Export);
    }
}
