//! `FstSource` — `WaveformSource` implementation for FST files.
//!
//! Uses wellen for hierarchy parsing (via the wellen adapter) and fst-reader
//! for signal value reading.

use std::collections::HashMap;
use std::io::BufReader;
use std::path::Path;

use fst_reader::{FstFilter, FstHierarchyEntry, FstReader, FstSignalHandle, FstSignalValue};

use crate::error::{Error, Result};
use crate::hierarchy::Hierarchy;
use crate::types::{SignalId, SignalValue, VarId, WaveformMetadata};
use crate::waveform::WaveformSource;
use crate::wellen_adapter::build_hierarchy_from_wellen;

/// FST file waveform source.
///
/// Opens the file twice: once with wellen (for hierarchy) and once with
/// fst-reader (for signal data). A `signal_map` bridges the two ID spaces.
pub struct FstSource {
    hierarchy: Hierarchy,
    metadata: WaveformMetadata,
    reader: FstReader<BufReader<std::fs::File>>,
    /// Maps `SignalId` → `FstSignalHandle` (bridges wellen and fst-reader ID spaces).
    signal_map: HashMap<SignalId, FstSignalHandle>,
}

impl FstSource {
    /// Open an FST file.
    ///
    /// 1. Parses hierarchy with wellen, builds `Hierarchy` via the adapter.
    /// 2. Opens with fst-reader for data access.
    /// 3. Reads fst-reader's hierarchy to build the signal handle mapping.
    pub fn open(path: &Path) -> Result<Self> {
        // 1. Parse hierarchy with wellen
        let opts = wellen::LoadOptions::default();
        let wellen_header = wellen::viewers::read_header_from_file(path, &opts)
            .map_err(|e| Error::FileOpen(format!("wellen: {e}")))?;
        let hierarchy = build_hierarchy_from_wellen(&wellen_header.hierarchy);

        // 2. Open with fst-reader for data access
        let file = std::fs::File::open(path)?;
        let mut reader = FstReader::open_and_read_time_table(BufReader::new(file))
            .map_err(|e| Error::FileOpen(format!("fst-reader: {e}")))?;

        // 3. Build metadata from fst-reader header
        let fst_header = reader.get_header();
        let metadata = WaveformMetadata {
            timescale_exponent: fst_header.timescale_exponent,
            start_time: fst_header.start_time,
            end_time: fst_header.end_time,
            var_count: fst_header.var_count,
            signal_count: hierarchy.signal_count(),
        };

        // 4. Read fst-reader's hierarchy to collect FstSignalHandle per var in
        //    declaration order, then match with our hierarchy (same order).
        let mut fst_handles: Vec<FstSignalHandle> = Vec::new();
        reader
            .read_hierarchy(|entry| {
                if let FstHierarchyEntry::Var { handle, .. } = entry {
                    fst_handles.push(handle);
                }
            })
            .map_err(|e| Error::FileOpen(format!("fst-reader hierarchy: {e}")))?;

        // Build signal_map: SignalId → FstSignalHandle.
        // Both wellen and fst-reader iterate the same FST hierarchy in declaration
        // order, so var index i in our hierarchy corresponds to fst_handles[i].
        let mut signal_map = HashMap::new();
        let var_count = hierarchy.var_count().min(fst_handles.len());
        for (var_idx, &handle) in fst_handles.iter().enumerate().take(var_count) {
            let var_id = VarId(var_idx as u32);
            let signal_id = hierarchy.var_signal_id(var_id);
            // For aliases (multiple vars with the same SignalId), the last write
            // wins, but all aliased handles read the same data so this is fine.
            signal_map.insert(signal_id, handle);
        }

        Ok(Self {
            hierarchy,
            metadata,
            reader,
            signal_map,
        })
    }
}

impl WaveformSource for FstSource {
    fn metadata(&self) -> &WaveformMetadata {
        &self.metadata
    }

    fn hierarchy(&self) -> &Hierarchy {
        &self.hierarchy
    }

    fn read_signals(
        &mut self,
        signals: &[SignalId],
        time_range: std::ops::Range<u64>,
        callback: &mut dyn FnMut(u64, SignalId, SignalValue),
    ) -> Result<()> {
        if time_range.is_empty() {
            return Ok(());
        }

        // Map SignalId → FstSignalHandle
        let handles: Vec<FstSignalHandle> = signals
            .iter()
            .filter_map(|sig| self.signal_map.get(sig).copied())
            .collect();

        if handles.is_empty() {
            return Ok(());
        }

        // Build reverse map: FstSignalHandle → SignalId
        let mut handle_to_signal: HashMap<FstSignalHandle, SignalId> = HashMap::new();
        for sig in signals {
            if let Some(&handle) = self.signal_map.get(sig) {
                handle_to_signal.insert(handle, *sig);
            }
        }

        // fst-reader filter uses inclusive end
        let fst_end = time_range.end.saturating_sub(1);
        let filter = FstFilter::new(time_range.start, fst_end, handles);

        self.reader
            .read_signals(&filter, |time, handle, value| {
                if let Some(&signal_id) = handle_to_signal.get(&handle) {
                    if time_range.contains(&time) {
                        match value {
                            FstSignalValue::String(bytes) => {
                                callback(time, signal_id, SignalValue::Binary(bytes));
                            }
                            FstSignalValue::Real(r) => {
                                callback(time, signal_id, SignalValue::Real(r));
                            }
                        }
                    }
                }
            })
            .map_err(|e| Error::SignalLoad(format!("{e}")))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_FST: &str = "/home/sjalloq/Work/fst-reader/fsts/icarus/rv32_soc_TB.vcd.fst";

    #[test]
    fn open_and_metadata() {
        let source = FstSource::open(Path::new(TEST_FST)).expect("failed to open FST");
        let meta = source.metadata();
        assert!(meta.end_time >= meta.start_time);
        assert!(meta.var_count > 0);
        assert!(meta.signal_count > 0);
    }

    #[test]
    fn hierarchy_navigable() {
        let source = FstSource::open(Path::new(TEST_FST)).expect("failed to open FST");
        let h = source.hierarchy();
        assert!(h.scope_count() > 0);
        assert!(h.var_count() > 0);
        let top = h.top_scopes();
        assert!(!top.is_empty());
        let name = h.scope_name(top[0]);
        assert!(!name.is_empty(), "top scope should have a name");
    }

    #[test]
    fn read_signals_one_signal() {
        let mut source = FstSource::open(Path::new(TEST_FST)).expect("failed to open FST");
        let h = source.hierarchy();

        // Pick the first var's signal
        let first_var = VarId(0);
        let signal_id = h.var_signal_id(first_var);

        let meta = source.metadata();
        let mut results: Vec<(u64, SignalId)> = Vec::new();

        source
            .read_signals(
                &[signal_id],
                meta.start_time..meta.end_time + 1,
                &mut |time, sig, _val| {
                    results.push((time, sig));
                },
            )
            .expect("read_signals failed");

        // Should get at least one value change
        assert!(!results.is_empty(), "expected at least one value change");
        // All results should be for the requested signal
        assert!(
            results.iter().all(|(_, s)| *s == signal_id),
            "all results should be for the requested signal"
        );
    }

    #[test]
    fn read_signals_time_filter() {
        let mut source = FstSource::open(Path::new(TEST_FST)).expect("failed to open FST");
        let signal_id = source.hierarchy().var_signal_id(VarId(0));
        let start_time = source.metadata().start_time;
        let end_time = source.metadata().end_time;

        // Use a restricted time range (first half of simulation)
        let midpoint = (start_time + end_time) / 2;
        let mut results: Vec<u64> = Vec::new();

        source
            .read_signals(
                &[signal_id],
                start_time..midpoint,
                &mut |time, _, _| {
                    results.push(time);
                },
            )
            .expect("read_signals failed");

        // All times should be within the requested range
        for &t in &results {
            assert!(
                t >= start_time && t < midpoint,
                "time {t} outside range {start_time}..{midpoint}",
            );
        }
    }

    #[test]
    fn read_signals_multiple() {
        let mut source = FstSource::open(Path::new(TEST_FST)).expect("failed to open FST");
        let h = source.hierarchy();
        let meta = source.metadata();

        // Collect signal IDs for the first two vars (if they have different signals)
        let sig0 = h.var_signal_id(VarId(0));
        let sig1 = h.var_signal_id(VarId(1));
        let signals = if sig0 == sig1 {
            vec![sig0]
        } else {
            vec![sig0, sig1]
        };

        let mut seen_signals: std::collections::HashSet<SignalId> =
            std::collections::HashSet::new();

        source
            .read_signals(
                &signals,
                meta.start_time..meta.end_time + 1,
                &mut |_, sig, _| {
                    seen_signals.insert(sig);
                },
            )
            .expect("read_signals failed");

        // Should see changes for all requested signals
        for s in &signals {
            assert!(
                seen_signals.contains(s),
                "expected to see changes for signal {s:?}"
            );
        }
    }

    #[test]
    fn usable_as_trait_object() {
        let source = FstSource::open(Path::new(TEST_FST)).expect("failed to open FST");
        let boxed: Box<dyn WaveformSource> = Box::new(source);
        assert!(boxed.metadata().var_count > 0);
        assert!(boxed.hierarchy().scope_count() > 0);
    }
}
