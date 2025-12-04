//! Status bar component

use ratatui::prelude::*;
use ratatui::widgets::Paragraph;

use fstty_core::{SignalSelection, WaveformFile};

/// Status bar showing file info and key hints
pub struct StatusBar<'a> {
    waveform: Option<&'a WaveformFile>,
    selection: &'a SignalSelection,
    message: Option<&'a str>,
}

impl<'a> StatusBar<'a> {
    pub fn new(
        waveform: Option<&'a WaveformFile>,
        selection: &'a SignalSelection,
        message: Option<&'a str>,
    ) -> Self {
        Self {
            waveform,
            selection,
            message,
        }
    }

    pub fn build(&self) -> Paragraph<'a> {
        let content = if let Some(msg) = self.message {
            msg.to_string()
        } else if let Some(_waveform) = self.waveform {
            let selected = self.selection.selected_signal_count();

            if selected > 0 {
                format!(
                    " {} selected | q:quit j/k:nav l/h:expand/collapse",
                    selected
                )
            } else {
                " q:quit j/k:nav l/h:expand/collapse space:toggle".to_string()
            }
        } else {
            " q:quit".to_string()
        };

        Paragraph::new(content).style(Style::default().bg(Color::Blue).fg(Color::White))
    }
}
