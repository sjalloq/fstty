//! WaveformSource trait — the fundamental data access abstraction.
//!
//! Every backend (FST, VCD, GHW) implements this trait. Consumers (TUI, server,
//! export) program against it and never touch backend types directly.

use std::ops::Range;

use crate::error::Result;
use crate::hierarchy::Hierarchy;
use crate::types::{SignalId, SignalValue, WaveformMetadata};

/// The one trait that backends implement.
///
/// Provides access to hierarchy metadata and signal value changes.
pub trait WaveformSource {
    /// File-level metadata (timescale, time range, counts).
    fn metadata(&self) -> &WaveformMetadata;

    /// The parsed hierarchy tree.
    fn hierarchy(&self) -> &Hierarchy;

    /// Stream value changes for selected signals in a time range.
    ///
    /// Calls `callback` with `(time, signal_id, value)` in time order within
    /// each VC block. This is the fundamental data access primitive.
    fn read_signals(
        &mut self,
        signals: &[SignalId],
        time_range: Range<u64>,
        callback: &mut dyn FnMut(u64, SignalId, SignalValue),
    ) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hierarchy::{HierarchyBuilder, HierarchyEvent};
    use crate::types::*;

    /// A mock WaveformSource with a hardcoded hierarchy and canned signal data.
    struct TestSource {
        hierarchy: Hierarchy,
        metadata: WaveformMetadata,
        /// Canned value changes: (time, signal_id, value_bytes).
        changes: Vec<(u64, SignalId, Vec<u8>)>,
    }

    impl TestSource {
        fn new() -> Self {
            // Build a tiny hierarchy: top > clk (1-bit), data (8-bit)
            let mut b = HierarchyBuilder::new();
            b.event(HierarchyEvent::EnterScope {
                name: "top".into(),
                scope_type: ScopeType::Module,
            });
            b.event(HierarchyEvent::Var {
                name: "clk".into(),
                var_type: VarType::Wire,
                direction: VarDirection::Implicit,
                width: 1,
                signal_id: SignalId(0),
                is_alias: false,
            });
            b.event(HierarchyEvent::Var {
                name: "data".into(),
                var_type: VarType::Reg,
                direction: VarDirection::Implicit,
                width: 8,
                signal_id: SignalId(1),
                is_alias: false,
            });
            b.event(HierarchyEvent::ExitScope);
            let hierarchy = b.build();

            let metadata = WaveformMetadata {
                timescale_exponent: -9,
                start_time: 0,
                end_time: 100,
                var_count: 2,
                signal_count: 2,
            };

            let changes = vec![
                (0, SignalId(0), b"0".to_vec()),
                (0, SignalId(1), b"00000000".to_vec()),
                (10, SignalId(0), b"1".to_vec()),
                (20, SignalId(0), b"0".to_vec()),
                (20, SignalId(1), b"11111111".to_vec()),
                (30, SignalId(0), b"1".to_vec()),
            ];

            Self {
                hierarchy,
                metadata,
                changes,
            }
        }
    }

    impl WaveformSource for TestSource {
        fn metadata(&self) -> &WaveformMetadata {
            &self.metadata
        }

        fn hierarchy(&self) -> &Hierarchy {
            &self.hierarchy
        }

        fn read_signals(
            &mut self,
            signals: &[SignalId],
            time_range: Range<u64>,
            callback: &mut dyn FnMut(u64, SignalId, SignalValue),
        ) -> Result<()> {
            for (time, sig_id, value_bytes) in &self.changes {
                if time_range.contains(time) && signals.contains(sig_id) {
                    callback(*time, *sig_id, SignalValue::Binary(value_bytes));
                }
            }
            Ok(())
        }
    }

    #[test]
    fn object_safe() {
        // Verify WaveformSource is object-safe: Box<dyn WaveformSource> compiles.
        let source: Box<dyn WaveformSource> = Box::new(TestSource::new());
        assert_eq!(source.metadata().start_time, 0);
        assert_eq!(source.metadata().end_time, 100);
        assert_eq!(source.hierarchy().scope_count(), 1);
        assert_eq!(source.hierarchy().var_count(), 2);
    }

    #[test]
    fn mock_read_signals_all() {
        let mut source = TestSource::new();
        let signals = vec![SignalId(0), SignalId(1)];
        let mut results: Vec<(u64, SignalId, Vec<u8>)> = Vec::new();

        source
            .read_signals(&signals, 0..100, &mut |time, sig, val| {
                if let SignalValue::Binary(b) = val {
                    results.push((time, sig, b.to_vec()));
                }
            })
            .unwrap();

        // All 6 changes are in range 0..100
        assert_eq!(results.len(), 6);
        // First change: time 0, signal 0, value "0"
        assert_eq!(results[0], (0, SignalId(0), b"0".to_vec()));
        // Last change: time 30, signal 0, value "1"
        assert_eq!(results[5], (30, SignalId(0), b"1".to_vec()));
    }

    #[test]
    fn mock_read_signals_time_filter() {
        let mut source = TestSource::new();
        let signals = vec![SignalId(0), SignalId(1)];
        let mut results: Vec<(u64, SignalId)> = Vec::new();

        source
            .read_signals(&signals, 10..25, &mut |time, sig, _val| {
                results.push((time, sig));
            })
            .unwrap();

        // Only changes at time 10 and 20 are in range 10..25
        assert_eq!(results.len(), 3);
        assert_eq!(results[0], (10, SignalId(0)));
        assert_eq!(results[1], (20, SignalId(0)));
        assert_eq!(results[2], (20, SignalId(1)));
    }

    #[test]
    fn mock_read_signals_signal_filter() {
        let mut source = TestSource::new();
        // Only request signal 1
        let signals = vec![SignalId(1)];
        let mut results: Vec<(u64, SignalId)> = Vec::new();

        source
            .read_signals(&signals, 0..100, &mut |time, sig, _val| {
                results.push((time, sig));
            })
            .unwrap();

        // Only 2 changes for signal 1
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|(_, sig)| *sig == SignalId(1)));
    }

    #[test]
    fn mock_via_trait_object() {
        // Use through Box<dyn WaveformSource> to prove dynamic dispatch works.
        let mut source: Box<dyn WaveformSource> = Box::new(TestSource::new());
        let mut count = 0u32;

        source
            .read_signals(&[SignalId(0)], 0..100, &mut |_time, _sig, _val| {
                count += 1;
            })
            .unwrap();

        // 4 changes for signal 0: at times 0, 10, 20, 30
        assert_eq!(count, 4);
    }
}
