//! Application state and lifecycle - Minimal TUI

use std::io;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Padding, Paragraph, Wrap};

/// Popup message level
#[derive(Clone)]
pub enum PopupLevel {
    Info,
    Warning,
    Error,
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
}

impl App {
    /// Create a new application
    pub fn new() -> Self {
        Self {
            exit: false,
            popup: None,
        }
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

            terminal.draw(|frame| self.render(frame))?;
            self.handle_events()?;
        }

        ratatui::restore();
        Ok(())
    }

    /// Render a single frame to string (for screenshots/testing)
    pub fn screenshot(&self, width: u16, height: u16) -> String {
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
        // Screenshot always works, even with popup
        if matches!(code, KeyCode::Char('s') | KeyCode::Char('S')) {
            self.save_screenshot();
            return;
        }

        // If popup is showing, only Esc dismisses it
        if self.popup.is_some() {
            if code == KeyCode::Esc {
                self.popup = None;
            }
            return;
        }

        match code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.exit = true;
            }
            KeyCode::Char('o') | KeyCode::Char('O') => {
                self.show_info("Open", "File browser not yet implemented.\n\nPress Esc to dismiss.");
            }
            _ => {}
        }
    }

    /// Render the application
    fn render(&self, frame: &mut Frame) {
        let area = frame.area();

        // Layout: main area + footer
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Min(1),    // Main content
                Constraint::Length(1), // Footer
            ])
            .split(area);

        // Main content area - empty for now
        let main_content = Paragraph::new("");
        frame.render_widget(main_content, chunks[0]);

        // Footer with key hints
        let footer = Paragraph::new(" q: Quit | o: Open | s: Screenshot")
            .style(Style::default().reversed());
        frame.render_widget(footer, chunks[1]);

        // Render popup on top if present
        if let Some(ref popup) = self.popup {
            self.render_popup(frame, popup);
        }
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
