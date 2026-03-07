//! Filtered FST export — raw block copying for maximum performance.
//!
//! Copies compressed value change data directly from source to output,
//! avoiding decompression/recompression. Only the frame (initial values),
//! position table, and hierarchy are rewritten.

use std::collections::{HashMap, HashSet};
use std::io::BufWriter;
use std::ops::Range;
use std::path::PathBuf;

use fst_reader::{FstHierarchyEntry, VcPackType as ReaderPackType};
use fst_writer::{
    extract_filtered_frame, FstFileType, FstInfo, FstRawWriter, SignalGeometry, VcPackType,
};

use crate::error::{Error, Result};
use crate::types::SignalId;

use super::FstSource;

/// Information about a VC (value change) block in an FST file.
#[derive(Debug, Clone)]
pub struct BlockInfo {
    /// Block index (0-based).
    pub index: usize,
    /// Start time of this block's time window.
    pub start_time: u64,
    /// End time of this block's time window.
    pub end_time: u64,
}

/// Configuration for filtered FST export.
pub struct ExportConfig {
    /// Output file path.
    pub output_path: PathBuf,
    /// Signals to include in the output.
    pub signals: Vec<SignalId>,
    /// Block index range to export (exclusive end). If None, all blocks.
    pub block_range: Option<Range<usize>>,
}

/// Result of a filtered export.
#[derive(Debug)]
pub struct ExportResult {
    /// Path to the output file.
    pub output_path: PathBuf,
    /// Number of signals in the output.
    pub signal_count: usize,
    /// Number of VC blocks written.
    pub block_count: usize,
}

impl FstSource {
    /// Get metadata about VC blocks in the FST file.
    pub fn block_infos(&self) -> Vec<BlockInfo> {
        self.reader
            .vc_block_infos()
            .into_iter()
            .map(|info| BlockInfo {
                index: info.index,
                start_time: info.start_time,
                end_time: info.end_time,
            })
            .collect()
    }

    /// Export a filtered FST file containing only selected signals and blocks.
    ///
    /// Uses raw block copying (no decompression of value change data) for
    /// maximum performance.
    pub fn export_filtered(&mut self, config: &ExportConfig) -> Result<ExportResult> {
        // 1. Map SignalId → fst-reader signal index
        let mut kept_indices: Vec<usize> = config
            .signals
            .iter()
            .filter_map(|sig| self.signal_map.get(sig).map(|h| h.get_index()))
            .collect();
        kept_indices.sort();
        kept_indices.dedup();

        if kept_indices.is_empty() {
            return Err(Error::SignalNotFound("no signals match".into()));
        }

        // 2. Get signal geometries
        let geom_tuples = self.reader.signal_geometries();
        let geometries: Vec<SignalGeometry> = geom_tuples
            .iter()
            .map(|&(frame_bytes, is_real)| SignalGeometry {
                frame_bytes,
                is_real,
            })
            .collect();

        // 3. Determine block range
        let all_blocks = self.reader.vc_block_infos();
        let block_range = config
            .block_range
            .clone()
            .unwrap_or(0..all_blocks.len());
        let blocks: Vec<_> = all_blocks[block_range].to_vec();

        // 4. Create output writer
        let header = self.reader.get_header();
        let info = FstInfo {
            start_time: header.start_time,
            timescale_exponent: header.timescale_exponent,
            version: header.version.clone(),
            date: header.date.clone(),
            file_type: FstFileType::Verilog,
        };
        let output = std::fs::File::create(&config.output_path)?;
        let mut writer =
            FstRawWriter::new(BufWriter::new(output), &info)
                .map_err(|e| Error::FstWrite(format!("{e}")))?;

        // 5. Copy filtered VC blocks
        let mut block_count = 0;
        for block_info in &blocks {
            let source_frame = self
                .reader
                .read_frame_raw(block_info)
                .map_err(|e| Error::FstWrite(format!("read frame: {e}")))?;

            let (time_data, time_uncomp, time_comp, time_count) = self
                .reader
                .read_time_table_raw(block_info)
                .map_err(|e| Error::FstWrite(format!("read time table: {e}")))?;

            // Read signal data from position table
            let mut source_offset_to_data: HashMap<(u64, u32), Vec<u8>> = HashMap::new();
            let mut signal_to_source_key: HashMap<usize, (u64, u32)> = HashMap::new();
            let mut pack_type = VcPackType::Lz4;

            self.reader
                .with_vc_block(block_info, |block_reader| {
                    let pos_table = block_reader.read_position_table()?;
                    pack_type = match block_reader.pack_type().unwrap_or(ReaderPackType::Lz4) {
                        ReaderPackType::Lz4 => VcPackType::Lz4,
                        ReaderPackType::FastLz => VcPackType::FastLz,
                        ReaderPackType::Zlib => VcPackType::Zlib,
                    };

                    let mut loc_map: HashMap<usize, _> = HashMap::new();
                    for loc in pos_table.iter() {
                        loc_map.insert(loc.signal_idx, loc);
                    }

                    for &sig_idx in &kept_indices {
                        if let Some(loc) = loc_map.get(&sig_idx) {
                            let key = (loc.offset, loc.length);
                            signal_to_source_key.insert(sig_idx, key);
                            if let std::collections::hash_map::Entry::Vacant(e) =
                                source_offset_to_data.entry(key)
                            {
                                let data = block_reader.read_signal_data_raw(loc)?;
                                e.insert(data);
                            }
                        }
                    }
                    Ok(())
                })
                .map_err(|e| Error::FstWrite(format!("read block: {e}")))?;

            // Write filtered block
            let filtered_frame =
                extract_filtered_frame(&geometries, &kept_indices, &source_frame);
            let mut block_writer = writer
                .begin_vc_block(block_info.start_time, block_info.end_time)
                .map_err(|e| Error::FstWrite(format!("{e}")))?;

            block_writer
                .write_frame(&filtered_frame, kept_indices.len())
                .map_err(|e| Error::FstWrite(format!("{e}")))?;
            block_writer
                .begin_waves(kept_indices.len(), pack_type)
                .map_err(|e| Error::FstWrite(format!("{e}")))?;

            // Write signal data with deduplication
            let mut source_to_output_offset: HashMap<(u64, u32), u64> = HashMap::new();
            let mut signal_offsets: Vec<(bool, u64)> = Vec::new();
            let mut current_offset = 1u64; // FST offsets start at 1 (0 = no data)

            for &sig_idx in &kept_indices {
                if let Some(&key) = signal_to_source_key.get(&sig_idx) {
                    if let Some(&out_offset) = source_to_output_offset.get(&key) {
                        // Alias — data already written
                        signal_offsets.push((true, out_offset));
                    } else {
                        let data = source_offset_to_data.get(&key).unwrap();
                        signal_offsets.push((true, current_offset));
                        source_to_output_offset.insert(key, current_offset);
                        block_writer
                            .write_raw_wave_data(data)
                            .map_err(|e| Error::FstWrite(format!("{e}")))?;
                        current_offset += data.len() as u64;
                    }
                } else {
                    // Signal has no data in this block
                    signal_offsets.push((false, 0));
                }
            }

            block_writer
                .write_position_table(&signal_offsets)
                .map_err(|e| Error::FstWrite(format!("{e}")))?;
            block_writer
                .write_time_table_raw(&time_data, time_uncomp, time_comp, time_count)
                .map_err(|e| Error::FstWrite(format!("{e}")))?;

            let block_waves: u64 =
                source_offset_to_data.values().map(|d| d.len() as u64).sum();
            let mem_required = filtered_frame.len() as u64 + block_waves;
            block_writer
                .finish(mem_required)
                .map_err(|e| Error::FstWrite(format!("{e}")))?;

            writer.vc_block_written();
            block_count += 1;
        }

        // 6. Write geometry
        let signal_lengths: Vec<u32> = kept_indices
            .iter()
            .map(|&idx| {
                let g = &geometries[idx];
                if g.is_real {
                    0
                } else {
                    g.frame_bytes
                }
            })
            .collect();
        writer
            .write_geometry(&signal_lengths)
            .map_err(|e| Error::FstWrite(format!("{e}")))?;

        // 7. Build and write filtered hierarchy
        let kept_set: HashSet<usize> = kept_indices.iter().copied().collect();
        let (hier_bytes, scope_count, var_count) =
            build_filtered_hierarchy(&mut self.reader, &kept_set)?;
        writer
            .write_hierarchy(&hier_bytes)
            .map_err(|e| Error::FstWrite(format!("{e}")))?;
        writer.set_counts(scope_count, var_count);

        // 8. Finish
        writer
            .finish()
            .map_err(|e| Error::FstWrite(format!("{e}")))?;

        Ok(ExportResult {
            output_path: config.output_path.clone(),
            signal_count: kept_indices.len(),
            block_count,
        })
    }
}

/// Build filtered hierarchy bytes from the source FST, keeping only signals
/// in `kept_signals`. Parent scopes are emitted lazily (only when a kept
/// variable is encountered).
fn build_filtered_hierarchy<R: std::io::BufRead + std::io::Seek>(
    reader: &mut fst_reader::FstReader<R>,
    kept_signals: &HashSet<usize>,
) -> Result<(Vec<u8>, u64, u64)> {
    let mut hierarchy_entries: Vec<FstHierarchyEntry> = Vec::new();
    reader
        .read_hierarchy(|entry| {
            hierarchy_entries.push(entry.clone());
        })
        .map_err(|e| Error::FstWrite(format!("read hierarchy: {e}")))?;

    let mut hier_data = Vec::new();
    let mut scope_stack: Vec<(String, u8, String)> = Vec::new();
    let mut scopes_written: usize = 0;
    let mut scope_count: u64 = 0;
    let mut var_count: u64 = 0;

    for entry in &hierarchy_entries {
        match entry {
            FstHierarchyEntry::Scope {
                name,
                tpe,
                component,
            } => {
                scope_stack.push((name.clone(), *tpe as u8, component.clone()));
            }
            FstHierarchyEntry::UpScope => {
                if scopes_written >= scope_stack.len() && scopes_written > 0 {
                    hier_data.push(255); // VCD_UPSCOPE
                    scopes_written -= 1;
                }
                scope_stack.pop();
            }
            FstHierarchyEntry::Var {
                handle,
                is_alias,
                name,
                tpe,
                direction,
                length,
                ..
            } => {
                if *is_alias {
                    continue;
                }

                if !kept_signals.contains(&handle.get_index()) {
                    continue;
                }

                // Emit pending parent scopes
                while scopes_written < scope_stack.len() {
                    let (scope_name, scope_type, component) = &scope_stack[scopes_written];
                    hier_data.push(254); // VCD_SCOPE
                    hier_data.push(*scope_type);
                    hier_data.extend_from_slice(scope_name.as_bytes());
                    hier_data.push(0);
                    hier_data.extend_from_slice(component.as_bytes());
                    hier_data.push(0);
                    scopes_written += 1;
                    scope_count += 1;
                }

                // Write var entry
                hier_data.push(*tpe as u8);
                hier_data.push(*direction as u8);
                hier_data.extend_from_slice(name.as_bytes());
                hier_data.push(0);
                write_varint(&mut hier_data, *length as u64);
                write_varint(&mut hier_data, 0); // alias = 0 (new signal)
                var_count += 1;
            }
            _ => {}
        }
    }

    // Close remaining open scopes
    while scopes_written > 0 {
        hier_data.push(255); // VCD_UPSCOPE
        scopes_written -= 1;
    }

    Ok((hier_data, scope_count, var_count))
}

fn write_varint(output: &mut Vec<u8>, mut value: u64) {
    if value <= 0x7f {
        output.push(value as u8);
        return;
    }
    while value != 0 {
        let next_value = value >> 7;
        let mask: u8 = if next_value == 0 { 0 } else { 0x80 };
        output.push((value & 0x7f) as u8 | mask);
        value = next_value;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fst::FstSource;
    use crate::types::VarId;
    use crate::waveform::WaveformSource;
    use std::path::Path;

    const TEST_FST: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/rv32_soc_TB.vcd.fst");

    #[test]
    fn block_infos_nonzero() {
        let source = FstSource::open(Path::new(TEST_FST)).expect("failed to open FST");
        let blocks = source.block_infos();
        assert!(!blocks.is_empty(), "expected at least one VC block");
    }

    #[test]
    fn block_infos_times_non_overlapping() {
        let source = FstSource::open(Path::new(TEST_FST)).expect("failed to open FST");
        let blocks = source.block_infos();

        for block in &blocks {
            assert!(
                block.end_time >= block.start_time,
                "block {} has end_time < start_time",
                block.index,
            );
        }

        // Adjacent blocks should be contiguous (no gaps, no overlaps)
        for pair in blocks.windows(2) {
            assert!(
                pair[1].start_time >= pair[0].start_time,
                "block {} starts before block {} ends",
                pair[1].index,
                pair[0].index,
            );
        }
    }

    #[test]
    fn export_filtered_creates_file() {
        let mut source = FstSource::open(Path::new(TEST_FST)).expect("failed to open FST");

        // Pick the first signal
        let signal_id = source.hierarchy().var_signal_id(VarId(0));
        let output_path = std::env::temp_dir().join("fstty_test_export.fst");

        let result = source
            .export_filtered(&ExportConfig {
                output_path: output_path.clone(),
                signals: vec![signal_id],
                block_range: None,
            })
            .expect("export_filtered failed");

        assert!(output_path.exists(), "output file should exist");
        assert_eq!(result.signal_count, 1);
        assert!(result.block_count > 0);

        // Clean up
        let _ = std::fs::remove_file(&output_path);
    }

    #[test]
    fn export_roundtrip_signal_count() {
        let mut source = FstSource::open(Path::new(TEST_FST)).expect("failed to open FST");

        // Pick first two distinct signals
        let sig0 = source.hierarchy().var_signal_id(VarId(0));
        let sig1 = source.hierarchy().var_signal_id(VarId(1));
        let signals = if sig0 == sig1 {
            vec![sig0]
        } else {
            vec![sig0, sig1]
        };
        let expected_count = signals.len();

        let output_path = std::env::temp_dir().join("fstty_test_roundtrip_count.fst");
        source
            .export_filtered(&ExportConfig {
                output_path: output_path.clone(),
                signals,
                block_range: None,
            })
            .expect("export_filtered failed");

        // Open the exported file with fst-reader and verify signal count
        let file = std::fs::File::open(&output_path).expect("failed to open exported FST");
        let reader = fst_reader::FstReader::open(std::io::BufReader::new(file))
            .expect("failed to parse exported FST");

        assert_eq!(
            reader.signal_count(),
            expected_count,
            "exported FST should have {expected_count} signal(s)",
        );

        let _ = std::fs::remove_file(&output_path);
    }

    #[test]
    fn export_roundtrip_values_match() {
        let mut source = FstSource::open(Path::new(TEST_FST)).expect("failed to open FST");

        let signal_id = source.hierarchy().var_signal_id(VarId(0));
        let meta = source.metadata().clone();

        // Read values from source
        let mut source_values: Vec<(u64, Vec<u8>)> = Vec::new();
        source
            .read_signals(
                &[signal_id],
                meta.start_time..meta.end_time + 1,
                &mut |time, _, val| {
                    if let crate::types::SignalValue::Binary(b) = val {
                        source_values.push((time, b.to_vec()));
                    }
                },
            )
            .expect("read_signals failed");

        // Export
        let output_path = std::env::temp_dir().join("fstty_test_roundtrip_values.fst");
        source
            .export_filtered(&ExportConfig {
                output_path: output_path.clone(),
                signals: vec![signal_id],
                block_range: None,
            })
            .expect("export_filtered failed");

        // Read values from exported file
        let mut exported_source =
            FstSource::open(&output_path).expect("failed to open exported FST");
        let exported_meta = exported_source.metadata().clone();
        let exported_signal = exported_source.hierarchy().var_signal_id(VarId(0));

        let mut exported_values: Vec<(u64, Vec<u8>)> = Vec::new();
        exported_source
            .read_signals(
                &[exported_signal],
                exported_meta.start_time..exported_meta.end_time + 1,
                &mut |time, _, val| {
                    if let crate::types::SignalValue::Binary(b) = val {
                        exported_values.push((time, b.to_vec()));
                    }
                },
            )
            .expect("read_signals on exported file failed");

        assert!(
            !source_values.is_empty(),
            "source should have value changes",
        );
        assert_eq!(
            source_values, exported_values,
            "exported values should match source values",
        );

        let _ = std::fs::remove_file(&output_path);
    }
}
