//! Application state and lifecycle - Minimal TUI

use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap};

use crate::file_picker::FilePicker;

/// Available tabs/tools
#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Tab {
    #[default]
    Browse,
    Convert,
    Filter,
    Analyze,
}

impl Tab {
    pub const ALL: &'static [Tab] = &[Tab::Browse, Tab::Convert, Tab::Filter, Tab::Analyze];

    pub fn label(&self) -> &'static str {
        match self {
            Tab::Browse => "Browse",
            Tab::Convert => "Convert",
            Tab::Filter => "Filter",
            Tab::Analyze => "Analyze",
        }
    }

    pub fn index(&self) -> usize {
        match self {
            Tab::Browse => 0,
            Tab::Convert => 1,
            Tab::Filter => 2,
            Tab::Analyze => 3,
        }
    }

    pub fn from_index(idx: usize) -> Self {
        match idx {
            0 => Tab::Browse,
            1 => Tab::Convert,
            2 => Tab::Filter,
            3 => Tab::Analyze,
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

/// Main application state
pub struct App {
    /// Should quit
    exit: bool,
    /// Popup message (dismisses on any key)
    popup: Option<Popup>,
    /// File picker
    file_picker: FilePicker,
    /// Currently loaded file
    loaded_file: Option<PathBuf>,
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
        Ok(Self {
            exit: false,
            popup: None,
            file_picker,
            loaded_file: None,
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

    /// Set active tab by name or number (for testing/screenshots)
    pub fn set_tab(&mut self, tab: &str) {
        self.active_tab = match tab.to_lowercase().as_str() {
            "1" | "browse" => Tab::Browse,
            "2" | "convert" => Tab::Convert,
            "3" | "filter" => Tab::Filter,
            "4" | "analyze" => Tab::Analyze,
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
    pub fn run(&mut self) -> Result<()> {
        let mut terminal = ratatui::init();

        while !self.exit {
            // Check for expired popups
            if let Some(ref popup) = self.popup {
                if let Some(expires_at) = popup.expires_at {
                    if Instant::now() >= expires_at {
                        self.popup = None;
                    }
                }
            }

            // Tick spinner if busy
            if self.busy_status.is_some() {
                self.spinner.tick();
            }

            terminal.draw(|frame| self.render(frame))?;
            self.handle_events()?;
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
            Err(e) => self.show_error("Screenshot", &format!("Failed to save: {}", e)),
        }
    }

    /// Handle events with sync poll + read pattern
    fn handle_events(&mut self) -> io::Result<()> {
        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    self.handle_key(key.code);
                }
            }
        }
        Ok(())
    }

    /// Handle a key press
    fn handle_key(&mut self, code: KeyCode) {
        // Screenshot always works
        if matches!(code, KeyCode::Char('s') | KeyCode::Char('S')) {
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
                            self.loaded_file = Some(path.clone());
                            self.file_picker.close();
                            self.show_toast(
                                "Opened",
                                format!("{}", path.display()),
                                Duration::from_secs(2),
                            );
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
            KeyCode::Char('2') => self.active_tab = Tab::Convert,
            KeyCode::Char('3') => self.active_tab = Tab::Filter,
            KeyCode::Char('4') => self.active_tab = Tab::Analyze,
            _ => {}
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
        let footer = Paragraph::new(" q: quit | o: open | tab/1-4: switch tabs | s: screenshot")
            .style(Style::default().reversed());
        frame.render_widget(footer, chunks[3]);

        // Render file picker on top if active
        if self.file_picker.active {
            self.file_picker.render(frame);
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
    fn render_tab_content(&self, frame: &mut Frame, area: Rect) {
        let content = match self.active_tab {
            Tab::Browse => {
                if self.loaded_file.is_some() {
                    // Will show hierarchy tree later
                    "Hierarchy tree will go here"
                } else {
                    "No file loaded. Press 'o' to open."
                }
            }
            Tab::Convert => "VCD → FST conversion tools",
            Tab::Filter => "Signal filtering and time windowing",
            Tab::Analyze => "Analysis plugins and queries",
        };

        let paragraph = Paragraph::new(content)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL));
        frame.render_widget(paragraph, area);
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

        // Right side: spinner + status message
        let status = if let Some(ref busy_msg) = self.busy_status {
            format!("{} {} ", self.spinner.frame(), busy_msg)
        } else {
            String::new()
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

    /// Render a centered popup
    fn render_popup(&self, frame: &mut Frame, popup: &Popup) {
        let area = frame.area();

        // Calculate popup size (50% width, auto height based on content)
        let popup_width = (area.width / 2).max(40).min(area.width - 4);
        let popup_height = 7; // title + border + message lines + padding

        // Center the popup
        let popup_area = Rect {
            x: (area.width - popup_width) / 2,
            y: (area.height - popup_height) / 2,
            width: popup_width,
            height: popup_height,
        };

        // Style based on level
        let (border_style, title_prefix) = match popup.level {
            PopupLevel::Info => (Style::default(), ""),
            PopupLevel::Warning => (Style::default().fg(Color::Yellow), "⚠ "),
            PopupLevel::Error => (Style::default().fg(Color::Red), "✗ "),
        };

        // Clear the area behind the popup
        frame.render_widget(Clear, popup_area);

        // Create the popup block with padding
        let block = Block::default()
            .title(format!(" {}{} ", title_prefix, popup.title))
            .borders(Borders::ALL)
            .border_style(border_style)
            .padding(Padding::new(2, 2, 1, 1)); // left, right, top, bottom

        // Create the message paragraph
        let message = Paragraph::new(popup.message.as_str())
            .wrap(Wrap { trim: false })
            .block(block);

        frame.render_widget(message, popup_area);
    }
}
