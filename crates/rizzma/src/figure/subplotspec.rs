//! A rectangular span of cells within a [`GridSpec`].
//!
//! [`SubplotSpec`] identifies a contiguous block of grid cells (a single cell
//! or a multi-cell span) and resolves it to a figure-fraction rectangle via
//! [`SubplotSpec::get_position`], mirroring matplotlib's
//! `SubplotSpec.get_position`.

use crate::core::Bbox;
use crate::figure::gridspec::GridSpec;

/// A half-open rectangular span of cells over a [`GridSpec`].
///
/// `rows` is `(row_start, row_end)` and `cols` is `(col_start, col_end)`, each
/// inclusive of the start and exclusive of the end. A single cell at
/// `(r, c)` is `rows = (r, r + 1)`, `cols = (c, c + 1)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SubplotSpec {
    /// Half-open `(start, end)` range of rows covered by this span.
    pub rows: (usize, usize),
    /// Half-open `(start, end)` range of columns covered by this span.
    pub cols: (usize, usize),
}

impl SubplotSpec {
    /// Resolve this span to its figure-fraction rectangle on `gs`.
    ///
    /// The returned [`Bbox`] spans from the left edge of the first column to
    /// the right edge of the last column, and from the bottom edge of the
    /// bottom-most row to the top edge of the top-most row in the span.
    #[must_use]
    pub fn get_position(&self, gs: &GridSpec) -> Bbox {
        let (fig_bottoms, fig_tops, fig_lefts, fig_rights) = gs.get_grid_positions();

        let (row0, row1) = self.rows;
        let (col0, col1) = self.cols;

        // Row 0 is the top row, so the first row in the span supplies the top
        // edge and the last row in the span supplies the bottom edge.
        let top = fig_tops[row0];
        let bottom = fig_bottoms[row1 - 1];
        let left = fig_lefts[col0];
        let right = fig_rights[col1 - 1];

        Bbox::from_extents(left, bottom, right, top)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-12, "expected {b}, got {a}");
    }

    #[test]
    fn single_cell_matches_grid_position() {
        let gs = GridSpec::new(2, 2);
        let (bottoms, tops, lefts, rights) = gs.get_grid_positions();
        let bbox = gs.subplot(0, 1).get_position(&gs);
        approx(bbox.xmin(), lefts[1]);
        approx(bbox.xmax(), rights[1]);
        approx(bbox.ymin(), bottoms[0]);
        approx(bbox.ymax(), tops[0]);
    }

    #[test]
    fn span_merges_cell_extents() {
        let gs = GridSpec::new(3, 3);
        let (bottoms, tops, lefts, rights) = gs.get_grid_positions();
        let bbox = gs.subplot_span(0..2, 1..3).get_position(&gs);
        approx(bbox.ymax(), tops[0]);
        approx(bbox.ymin(), bottoms[1]);
        approx(bbox.xmin(), lefts[1]);
        approx(bbox.xmax(), rights[2]);
    }
}
