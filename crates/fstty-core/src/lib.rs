//! fstty-core - Core waveform abstractions for fstty
//!
//! This crate provides the foundation for loading and manipulating FST/VCD waveform files.

pub mod error;
pub mod filter;
pub mod hierarchy;
pub mod hierarchy_legacy;
pub mod types;
pub mod waveform;
pub mod waveform_legacy;
pub mod writer;

pub use error::{Error, Result};
pub use filter::{FilterPattern, SignalSelection};
pub use hierarchy_legacy::{HierarchyNavigator, HierarchyNode};
pub use waveform::WaveformSource;
pub use waveform_legacy::{WaveformFile, WaveformFormat};
pub use writer::FilteredFstWriter;
