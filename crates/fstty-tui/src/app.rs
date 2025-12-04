//! Application state and lifecycle - Minimal TUI

use std::io;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use ratatui::prelude::*;
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

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
        });
    }

    /// Show a warning popup
    pub fn show_warning(&mut self, title: impl Into<String>, message: impl Into<String>) {
        self.popup = Some(Popup {
            title: title.into(),
            message: message.into(),
            level: PopupLevel::Warning,
        });
    }

    /// Show an error popup
    pub fn show_error(&mut self, title: impl Into<String>, message: impl Into<String>) {
        self.popup = Some(Popup {
            title: title.into(),
            message: message.into(),
            level: PopupLevel::Error,
        });
    }

    /// Run the application main loop
    pub fn run(&mut self) -> Result<()> {
        let mut terminal = ratatui::init();

        while !self.exit {
            terminal.draw(|frame| self.render(frame))?;
            self.handle_events()?;
        }

        ratatui::restore();
        Ok(())
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
        // If popup is showing, any key dismisses it
        if self.popup.is_some() {
            self.popup = None;
            return;
        }

        match code {
            KeyCode::Char('q') | KeyCode::Char('Q') => {
                self.exit = true;
            }
            KeyCode::Char('o') | KeyCode::Char('O') => {
                self.show_info("Open", "File browser not yet implemented.\n\nPress any key to dismiss.");
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
        let footer = Paragraph::new(" Q:quit O:open")
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

        // Create the popup block
        let block = Block::default()
            .title(format!(" {}{} ", title_prefix, popup.title))
            .borders(Borders::ALL)
            .border_style(border_style);

        // Create the message paragraph
        let message = Paragraph::new(popup.message.as_str())
            .wrap(Wrap { trim: false })
            .block(block);

        frame.render_widget(message, popup_area);
    }
}
