//! Waveform file abstraction over wellen

use std::path::{Path, PathBuf};

use wellen::{Hierarchy, LoadOptions};
use wellen::viewers;

use crate::error::{Error, Result};

/// Detected waveform file format
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaveformFormat {
    Fst,
    Vcd,
    Ghw,
}

impl WaveformFormat {
    /// Detect format from file extension
    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(|ext| match ext.to_lowercase().as_str() {
                "fst" => Some(WaveformFormat::Fst),
                "vcd" => Some(WaveformFormat::Vcd),
                "ghw" => Some(WaveformFormat::Ghw),
                _ => None,
            })
    }
}

/// Main waveform handle - wraps wellen with lazy loading
pub struct WaveformFile {
    hierarchy: Hierarchy,
    path: PathBuf,
    format: WaveformFormat,
}

impl WaveformFile {
    /// Load waveform header only (hierarchy metadata) - fast!
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let format = WaveformFormat::from_path(path)
            .ok_or_else(|| Error::FileOpen(format!("Unknown file format: {}", path.display())))?;

        let load_opts = LoadOptions::default();

        let header = viewers::read_header_from_file(path, &load_opts)
            .map_err(|e| Error::FileOpen(format!("{}: {}", path.display(), e)))?;

        Ok(Self {
            hierarchy: header.hierarchy,
            path: path.to_path_buf(),
            format,
        })
    }

    /// Get the file path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Get the detected format
    pub fn format(&self) -> WaveformFormat {
        self.format
    }

    /// Access the hierarchy for navigation
    pub fn hierarchy(&self) -> &Hierarchy {
        &self.hierarchy
    }

    /// Get the total number of unique signals
    pub fn num_unique_signals(&self) -> usize {
        self.hierarchy.num_unique_signals()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_detection() {
        assert_eq!(
            WaveformFormat::from_path(Path::new("test.fst")),
            Some(WaveformFormat::Fst)
        );
        assert_eq!(
            WaveformFormat::from_path(Path::new("test.vcd")),
            Some(WaveformFormat::Vcd)
        );
        assert_eq!(
            WaveformFormat::from_path(Path::new("test.FST")),
            Some(WaveformFormat::Fst)
        );
        assert_eq!(WaveformFormat::from_path(Path::new("test.txt")), None);
    }
}
