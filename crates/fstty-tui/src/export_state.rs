//! Export tab state: VC block timeline, cursor, and range selection.

use fstty_core::fst::BlockInfo;

/// State for the Export tab's block range selection.
///
/// State machine:
/// - `Idle`: no selection, cursor visible
/// - `StartMarked`: one end of the range is set, cursor selects the other end
/// - `RangeSelected`: both ends set, ready to export
#[derive(Debug)]
pub struct ExportState {
    /// Block metadata from the loaded FST.
    blocks: Vec<BlockInfo>,
    /// Current cursor position (block index).
    cursor: usize,
    /// First marked block index (set on first mark action).
    anchor: Option<usize>,
    /// Whether the range is finalized (second mark action).
    range_finalized: bool,
}

impl ExportState {
    /// Create a new export state from block metadata.
    pub fn new(blocks: Vec<BlockInfo>) -> Self {
        Self {
            blocks,
            cursor: 0,
            anchor: None,
            range_finalized: false,
        }
    }

    /// Number of blocks.
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Get block info by index.
    pub fn block(&self, index: usize) -> Option<&BlockInfo> {
        self.blocks.get(index)
    }

    /// All block infos.
    pub fn blocks(&self) -> &[BlockInfo] {
        &self.blocks
    }

    /// Current cursor position.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// Move cursor left (towards block 0), clamped.
    pub fn move_cursor_left(&mut self) {
        if self.blocks.is_empty() || self.range_finalized {
            return;
        }
        self.cursor = self.cursor.saturating_sub(1);
    }

    /// Move cursor right (towards last block), clamped.
    pub fn move_cursor_right(&mut self) {
        if self.blocks.is_empty() || self.range_finalized {
            return;
        }
        let max = self.blocks.len().saturating_sub(1);
        self.cursor = (self.cursor + 1).min(max);
    }

    /// Mark the current cursor position.
    ///
    /// First call sets the anchor. Second call finalizes the range.
    pub fn mark(&mut self) {
        if self.blocks.is_empty() {
            return;
        }
        if self.range_finalized {
            return;
        }
        match self.anchor {
            None => {
                self.anchor = Some(self.cursor);
            }
            Some(_) => {
                self.range_finalized = true;
            }
        }
    }

    /// Clear the selection, returning to idle state.
    pub fn clear_selection(&mut self) {
        self.anchor = None;
        self.range_finalized = false;
    }

    /// The anchor position, if set.
    pub fn anchor(&self) -> Option<usize> {
        self.anchor
    }

    /// Whether a complete range has been selected (ready to export).
    pub fn has_valid_range(&self) -> bool {
        self.range_finalized && self.anchor.is_some()
    }

    /// Get the selected block index range (inclusive start, exclusive end)
    /// suitable for passing to `ExportConfig::block_range`.
    ///
    /// Returns `None` if no valid range is selected.
    pub fn selected_range(&self) -> Option<std::ops::Range<usize>> {
        if !self.has_valid_range() {
            return None;
        }
        let a = self.anchor.unwrap();
        let b = self.cursor;
        let start = a.min(b);
        let end = a.max(b) + 1; // exclusive end
        Some(start..end)
    }

    /// Get the time range covered by the selected blocks.
    pub fn selected_time_range(&self) -> Option<(u64, u64)> {
        let range = self.selected_range()?;
        let start_time = self.blocks[range.start].start_time;
        let end_time = self.blocks[range.end - 1].end_time;
        Some((start_time, end_time))
    }

    /// Highlighted range (for rendering): the blocks between anchor and cursor.
    /// Returns `None` if no anchor is set.
    pub fn highlighted_range(&self) -> Option<std::ops::Range<usize>> {
        let a = self.anchor?;
        let b = self.cursor;
        let start = a.min(b);
        let end = a.max(b) + 1;
        Some(start..end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_blocks(n: usize) -> Vec<BlockInfo> {
        (0..n)
            .map(|i| BlockInfo {
                index: i,
                start_time: (i * 100) as u64,
                end_time: ((i + 1) * 100 - 1) as u64,
            })
            .collect()
    }

    // --- State machine tests ---

    #[test]
    fn initial_state_is_idle() {
        let state = ExportState::new(make_blocks(5));
        assert_eq!(state.cursor(), 0);
        assert!(state.anchor().is_none());
        assert!(!state.has_valid_range());
        assert!(state.selected_range().is_none());
    }

    #[test]
    fn mark_once_sets_anchor() {
        let mut state = ExportState::new(make_blocks(5));
        state.move_cursor_right();
        state.move_cursor_right();
        state.mark();

        assert_eq!(state.anchor(), Some(2));
        assert!(!state.has_valid_range());
    }

    #[test]
    fn mark_twice_finalizes_range() {
        let mut state = ExportState::new(make_blocks(5));
        state.mark(); // anchor at 0
        state.move_cursor_right();
        state.move_cursor_right();
        state.move_cursor_right();
        state.mark(); // finalize at 3

        assert!(state.has_valid_range());
        assert_eq!(state.selected_range(), Some(0..4));
    }

    #[test]
    fn mark_reverse_order() {
        let mut state = ExportState::new(make_blocks(5));
        // Move to block 4, mark anchor there
        for _ in 0..4 {
            state.move_cursor_right();
        }
        state.mark(); // anchor at 4

        // Move back to block 1
        for _ in 0..3 {
            state.move_cursor_left();
        }
        state.mark(); // finalize at 1

        assert!(state.has_valid_range());
        assert_eq!(state.selected_range(), Some(1..5)); // min..max+1
    }

    #[test]
    fn clear_returns_to_idle() {
        let mut state = ExportState::new(make_blocks(5));
        state.mark();
        state.move_cursor_right();
        state.mark();
        assert!(state.has_valid_range());

        state.clear_selection();
        assert!(state.anchor().is_none());
        assert!(!state.has_valid_range());
        assert!(state.selected_range().is_none());
    }

    #[test]
    fn mark_after_clear_starts_fresh() {
        let mut state = ExportState::new(make_blocks(5));
        state.mark();
        state.move_cursor_right();
        state.mark();
        state.clear_selection();

        // Cursor position is preserved, start a new selection
        state.mark();
        assert_eq!(state.anchor(), Some(1)); // cursor was at 1 from before
        assert!(!state.has_valid_range());
    }

    // --- Cursor clamping tests ---

    #[test]
    fn move_cursor_left_clamped_at_zero() {
        let mut state = ExportState::new(make_blocks(3));
        state.move_cursor_left();
        state.move_cursor_left();
        assert_eq!(state.cursor(), 0);
    }

    #[test]
    fn move_cursor_right_clamped_at_max() {
        let mut state = ExportState::new(make_blocks(3));
        state.move_cursor_right();
        state.move_cursor_right();
        state.move_cursor_right(); // already at 2, clamped
        state.move_cursor_right();
        assert_eq!(state.cursor(), 2);
    }

    #[test]
    fn cursor_locked_when_range_finalized() {
        let mut state = ExportState::new(make_blocks(5));
        state.mark();
        state.move_cursor_right();
        state.move_cursor_right();
        state.mark(); // finalized at 2
        assert_eq!(state.cursor(), 2);

        state.move_cursor_right(); // should not move
        assert_eq!(state.cursor(), 2);
        state.move_cursor_left(); // should not move
        assert_eq!(state.cursor(), 2);
    }

    // --- Empty blocks ---

    #[test]
    fn empty_blocks() {
        let mut state = ExportState::new(vec![]);
        assert_eq!(state.block_count(), 0);
        state.move_cursor_right();
        assert_eq!(state.cursor(), 0);
        state.mark();
        assert!(state.anchor().is_none());
        assert!(!state.has_valid_range());
    }

    // --- has_valid_range tests ---

    #[test]
    fn has_valid_range_only_after_two_marks() {
        let mut state = ExportState::new(make_blocks(3));
        assert!(!state.has_valid_range());

        state.mark();
        assert!(!state.has_valid_range());

        state.move_cursor_right();
        state.mark();
        assert!(state.has_valid_range());
    }

    #[test]
    fn single_block_range() {
        let mut state = ExportState::new(make_blocks(3));
        state.move_cursor_right(); // cursor at 1
        state.mark(); // anchor at 1
        state.mark(); // finalize at 1

        assert!(state.has_valid_range());
        assert_eq!(state.selected_range(), Some(1..2));
    }

    // --- Time range tests ---

    #[test]
    fn selected_time_range() {
        let mut state = ExportState::new(make_blocks(5));
        state.move_cursor_right(); // cursor at 1
        state.mark(); // anchor at 1
        state.move_cursor_right();
        state.move_cursor_right(); // cursor at 3
        state.mark(); // finalize

        let (start, end) = state.selected_time_range().unwrap();
        assert_eq!(start, 100); // block 1 start
        assert_eq!(end, 399);   // block 3 end
    }

    // --- Highlighted range tests ---

    #[test]
    fn highlighted_range_none_when_idle() {
        let state = ExportState::new(make_blocks(5));
        assert!(state.highlighted_range().is_none());
    }

    #[test]
    fn highlighted_range_follows_cursor() {
        let mut state = ExportState::new(make_blocks(5));
        state.move_cursor_right(); // cursor at 1
        state.mark(); // anchor at 1
        state.move_cursor_right();
        state.move_cursor_right(); // cursor at 3

        assert_eq!(state.highlighted_range(), Some(1..4));
    }

    // --- Third mark is ignored ---

    #[test]
    fn mark_ignored_when_finalized() {
        let mut state = ExportState::new(make_blocks(5));
        state.mark();
        state.move_cursor_right();
        state.mark(); // finalized

        let range = state.selected_range();
        state.mark(); // should do nothing
        assert_eq!(state.selected_range(), range);
    }
}
