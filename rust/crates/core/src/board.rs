//! The board: a 9 x 9 grid of coloured cells.
//!
//! Layout facts recovered from the original (docs/03_FEATURE_SPEC.md S3):
//!
//! * the grid is 9 x 9 = 81 cells, stored row-major;
//! * a cell is a single byte whose low six bits carry the colour
//!   (`0` = empty, `1..=7` = a colour) and whose upper two bits are flags;
//! * an empty cell is drawn but carries no colour.
//!
//! The type is deliberately a plain value: no interior mutability, no
//! interior mutability, no globals.  Every mutation goes through
//! [`Board::set`] so that later rules can be added without touching call
//! sites.

/// Board side length in cells.
pub const BOARD_SIDE: usize = 9;
/// Total number of cells.
pub const BOARD_CELLS: usize = BOARD_SIDE * BOARD_SIDE;
/// Value of an empty cell.
pub const EMPTY: u8 = 0;

/// A single cell value.
pub type Cell = u8;

/// Row-major index of `(row, col)`.
#[inline]
pub const fn index(row: usize, col: usize) -> usize {
    row * BOARD_SIDE + col
}

/// Row and column of a row-major index.
#[inline]
pub const fn coords(i: usize) -> (usize, usize) {
    (i / BOARD_SIDE, i % BOARD_SIDE)
}

/// `true` when `(row, col)` is inside the grid.
#[inline]
pub const fn in_bounds(row: isize, col: isize) -> bool {
    row >= 0 && row < BOARD_SIDE as isize && col >= 0 && col < BOARD_SIDE as isize
}

/// The board state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Board {
    cells: [Cell; BOARD_CELLS],
}

impl Default for Board {
    fn default() -> Self {
        Board {
            cells: [EMPTY; BOARD_CELLS],
        }
    }
}

impl Board {
    /// An all-empty board.
    pub const fn new() -> Self {
        Board {
            cells: [EMPTY; BOARD_CELLS],
        }
    }

    /// The raw cells.
    pub fn cells(&self) -> &[Cell; BOARD_CELLS] {
        &self.cells
    }

    /// Mutable access to the raw cells (used by the board generator).
    pub fn cells_mut(&mut self) -> &mut [Cell; BOARD_CELLS] {
        &mut self.cells
    }

    /// Colour at `(row, col)`; out-of-bounds reads yield [`EMPTY`].
    #[inline]
    pub fn get(&self, row: usize, col: usize) -> Cell {
        if row < BOARD_SIDE && col < BOARD_SIDE {
            self.cells[index(row, col)]
        } else {
            EMPTY
        }
    }

    /// Write a cell; out-of-bounds writes are ignored.
    #[inline]
    pub fn set(&mut self, row: usize, col: usize, v: Cell) {
        if row < BOARD_SIDE && col < BOARD_SIDE {
            self.cells[index(row, col)] = v;
        }
    }

    /// The colour masked down to the six meaningful bits.
    #[inline]
    pub fn colour_at(&self, row: usize, col: usize) -> Cell {
        self.get(row, col) & 0x3F
    }

    /// `true` when no cell is empty.
    pub fn is_full(&self) -> bool {
        self.cells.iter().all(|&c| c != EMPTY)
    }

    /// Indices of the empty cells, in ascending order.
    pub fn empty_cells(&self) -> Vec<usize> {
        self.cells
            .iter()
            .enumerate()
            .filter(|(_, &c)| c == EMPTY)
            .map(|(i, _)| i)
            .collect()
    }

    /// Render the board as the trace format uses it (`0` = empty).
    pub fn to_trace_string(&self) -> String {
        let mut s = String::with_capacity(BOARD_CELLS);
        for &c in self.cells.iter() {
            s.push(if c == EMPTY { '.' } else { (b'0' + c) as char });
        }
        s
    }

    /// Parse the trace representation.
    pub fn from_trace_string(s: &str) -> Option<Board> {
        let bytes = s.as_bytes();
        if bytes.len() < BOARD_CELLS {
            return None;
        }
        let mut b = Board::new();
        for (i, &ch) in bytes.iter().take(BOARD_CELLS).enumerate() {
            b.cells[i] = match ch {
                b'.' | b'_' => EMPTY,
                b'0'..=b'9' => ch - b'0',
                b'a'..=b'f' => ch - b'a' + 10,
                _ => return None,
            };
        }
        Some(b)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_the_trace_string() {
        let mut b = Board::new();
        b.set(0, 0, 3);
        b.set(8, 8, 7);
        let s = b.to_trace_string();
        assert_eq!(s.len(), BOARD_CELLS);
        assert_eq!(Board::from_trace_string(&s).unwrap(), b);
    }

    #[test]
    fn out_of_bounds_access_is_safe() {
        let mut b = Board::new();
        b.set(99, 99, 5);
        assert_eq!(b.get(99, 99), EMPTY);
        assert_eq!(b.get(0, 0), EMPTY);
    }
}
