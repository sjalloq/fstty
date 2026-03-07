//! fstty-core - Core waveform abstractions for fstty
//!
//! This crate provides the foundation for loading and manipulating FST/VCD waveform files.

pub mod error;
pub mod filter;
pub mod fst;
pub mod hierarchy;
pub mod hierarchy_legacy;
pub mod types;
pub mod waveform;
pub mod waveform_legacy;
pub mod wellen_adapter;
pub mod writer;

pub use error::{Error, Result};
pub use filter::{FilterPattern, SignalSelection};
pub use fst::FstSource;
pub use hierarchy_legacy::{HierarchyNavigator, HierarchyNode};
pub use waveform::WaveformSource;
pub use waveform_legacy::{WaveformFile, WaveformFormat};
pub use writer::FilteredFstWriter;

#[cfg(test)]
mod fst_reader_smoke_tests {
    use std::io::BufReader;

    const TEST_FST: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/rv32_soc_TB.vcd.fst");

    #[test]
    fn open_fst_and_read_header() {
        let file = std::fs::File::open(TEST_FST).expect("failed to open test FST file");
        let reader =
            fst_reader::FstReader::open(BufReader::new(file)).expect("failed to parse FST");
        let header = reader.get_header();
        assert!(
            reader.signal_count() > 0,
            "expected at least one signal in test FST"
        );
        assert!(
            header.var_count > 0,
            "expected at least one var in test FST"
        );
        assert!(
            header.end_time >= header.start_time,
            "end_time should be >= start_time"
        );
    }
}
