//! Fixed-size blocks of rows for long code and diff views.
//!
//! A file of thousands of lines renders as a run of blocks of [`BLOCK_ROWS`] rows each. A
//! block is one `content-visibility: auto` container (`.vblock`, app.css): off screen it
//! skips style, layout and paint as a unit, and because every row is exactly one row tall
//! (`--row-h`), its placeholder size is exact. Scrolling then never shows blank lines or
//! moves the scrollbar, and `#L…` links, find in page and screen readers still see every
//! line (`content-visibility: auto` keeps skipped content in the DOM and searchable).
//!
//! Templates give each block `data-rows="{{ block.len() }}"`; app.css turns it into the
//! block's exact placeholder height where the browser supports typed `attr()`, and
//! `blocks.js` does the same through the CSSOM elsewhere (the CSP forbids `style=`).
//! See docs/design/frontend-virtualization.md.

/// Rows per block: large enough that a long file has few containment boundaries (a
/// 20,000-line file has 157), small enough that a block is about two screens tall.
pub const BLOCK_ROWS: usize = 128;

/// A run of at most [`BLOCK_ROWS`] consecutive rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block<T> {
    rows: Vec<T>,
}

impl<T> Block<T> {
    /// The rows, in order.
    #[must_use]
    pub fn rows(&self) -> &[T] {
        &self.rows
    }

    /// The number of rows: the block's height in rows, rendered as `data-rows`.
    #[must_use]
    pub fn len(&self) -> usize {
        self.rows.len()
    }

    /// Never true for a block from [`blocks`] or [`Blocker`]; required by clippy next to `len`.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// Splits `rows` into blocks of [`BLOCK_ROWS`]; only the last may be shorter. No rows, no
/// blocks.
pub fn blocks<T>(rows: impl IntoIterator<Item = T>) -> Vec<Block<T>> {
    let mut blocker = Blocker::new();
    for row in rows {
        blocker.push(row);
    }
    blocker.finish()
}

/// Builds blocks one row at a time, for views that must also end a block early: a diff
/// ends the current block before a comment thread, which is not a fixed-height row and so
/// is rendered between blocks.
#[derive(Debug)]
pub struct Blocker<T> {
    done: Vec<Block<T>>,
    current: Vec<T>,
}

impl<T> Default for Blocker<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Blocker<T> {
    #[must_use]
    pub fn new() -> Self {
        Self { done: Vec::new(), current: Vec::new() }
    }

    /// Appends a row, starting a new block once the current one is full.
    pub fn push(&mut self, row: T) {
        if self.current.len() == BLOCK_ROWS {
            self.end_block();
        }
        self.current.push(row);
    }

    /// Ends the current block early; nothing happens when it is empty.
    pub fn end_block(&mut self) {
        if !self.current.is_empty() {
            let rows = std::mem::replace(&mut self.current, Vec::with_capacity(BLOCK_ROWS));
            self.done.push(Block { rows });
        }
    }

    /// Ends the current block and takes every block finished so far, leaving the blocker
    /// empty and ready for the next run of rows.
    pub fn take(&mut self) -> Vec<Block<T>> {
        self.end_block();
        std::mem::take(&mut self.done)
    }

    /// Every block, the last one possibly short.
    #[must_use]
    pub fn finish(mut self) -> Vec<Block<T>> {
        self.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lengths<T>(blocks: &[Block<T>]) -> Vec<usize> {
        blocks.iter().map(Block::len).collect()
    }

    #[test]
    fn rows_fill_whole_blocks_and_only_the_last_is_short() {
        assert_eq!(lengths(&blocks(0..0)), [0_usize; 0]);
        assert_eq!(lengths(&blocks(0..1)), [1]);
        assert_eq!(lengths(&blocks(0..BLOCK_ROWS)), [BLOCK_ROWS]);
        assert_eq!(lengths(&blocks(0..2 * BLOCK_ROWS + 5)), [BLOCK_ROWS, BLOCK_ROWS, 5]);
    }

    #[test]
    fn blocks_keep_every_row_in_order() {
        let rows: Vec<usize> = (0..1_000).collect();
        let flat: Vec<usize> = blocks(rows.clone()).iter().flat_map(|block| block.rows().to_vec()).collect();
        assert_eq!(flat, rows);
    }

    #[test]
    fn ending_a_block_early_starts_the_next_one_fresh() {
        let mut blocker = Blocker::new();
        for row in 0..3 {
            blocker.push(row);
        }
        blocker.end_block();
        blocker.end_block(); // no empty block
        let first = blocker.take();
        assert_eq!(lengths(&first), [3]);
        for row in 0..=BLOCK_ROWS {
            blocker.push(row);
        }
        assert_eq!(lengths(&blocker.finish()), [BLOCK_ROWS, 1]);
    }
}
