//! Event handling for the TUI

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event, KeyEvent};
use tokio::sync::mpsc;

/// Event handler for terminal events
pub struct EventHandler {
    rx: mpsc::UnboundedReceiver<Event>,
    _tx: mpsc::UnboundedSender<Event>,
}

impl EventHandler {
    /// Create a new event handler
    pub fn new() -> Self {
        let (tx, rx) = mpsc::unbounded_channel();
        let _tx = tx.clone();

        // Spawn event reading task
        tokio::spawn(async move {
            loop {
                if event::poll(Duration::from_millis(100)).unwrap_or(false) {
                    if let Ok(event) = event::read() {
                        if tx.send(event).is_err() {
                            break;
                        }
                    }
                }
            }
        });

        Self { rx, _tx }
    }

    /// Wait for the next event
    pub async fn next(&mut self) -> Option<Event> {
        self.rx.recv().await
    }
}
