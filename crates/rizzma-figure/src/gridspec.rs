//! Subplot grid geometry.
//!
//! [`GridSpec`] describes a regular grid of cells within a figure, with
//! configurable outer margins, inter-cell spacing, and per-row/per-column
//! size ratios. It is a faithful port of the geometry in matplotlib's
//! `matplotlib.gridspec.GridSpec.get_grid_positions`.
//!
//! All coordinates are in figure fractions: `0.0` is the left/bottom edge of
//! the figure and `1.0` is the right/top edge.

use crate::subplotspec::SubplotSpec;

/// A regular grid of subplot cells expressed in figure-fraction coordinates.
///
/// The grid occupies the rectangle `[left, right] x [bottom, top]`. Within that
/// rectangle, `nrows * ncols` cells are laid out with gaps between them. The
/// gap between columns is `wspace` times the average cell width, and the gap
/// between rows is `hspace` times the average cell height (matplotlib
/// semantics). Optional `width_ratios` and `height_ratios` scale the relative
/// sizes of the columns and rows respectively.
#[derive(Debug, Clone, PartialEq)]
pub struct GridSpec {
    /// Number of rows in the grid.
    pub nrows: usize,
    /// Number of columns in the grid.
    pub ncols: usize,
    /// Left edge of the grid as a figure fraction.
    pub left: f64,
    /// Right edge of the grid as a figure fraction.
    pub right: f64,
    /// Bottom edge of the grid as a figure fraction.
    pub bottom: f64,
    /// Top edge of the grid as a figure fraction.
    pub top: f64,
    /// Width of inter-column gaps as a fraction of the average cell width.
    pub wspace: f64,
    /// Height of inter-row gaps as a fraction of the average cell height.
    pub hspace: f64,
    /// Optional relative widths of the columns. When `None`, columns are equal.
    pub width_ratios: Option<Vec<f64>>,
    /// Optional relative heights of the rows. When `None`, rows are equal.
    pub height_ratios: Option<Vec<f64>>,
}

impl GridSpec {
    /// Create a grid with `nrows` rows and `ncols` columns using
    /// matplotlib-like default margins and spacing.
    ///
    /// Defaults: `left = 0.125`, `right = 0.9`, `bottom = 0.11`, `top = 0.88`,
    /// `wspace = 0.2`, `hspace = 0.2`, and equal row/column ratios.
    ///
    /// # Panics
    ///
    /// Panics if `nrows` or `ncols` is zero.
    #[must_use]
    pub fn new(nrows: usize, ncols: usize) -> Self {
        assert!(nrows > 0, "nrows must be greater than zero");
        assert!(ncols > 0, "ncols must be greater than zero");
        Self {
            nrows,
            ncols,
            left: 0.125,
            right: 0.9,
            bottom: 0.11,
            top: 0.88,
            wspace: 0.2,
            hspace: 0.2,
            width_ratios: None,
            height_ratios: None,
        }
    }

    /// Set the outer margins (figure fractions) and return `self`.
    ///
    /// The arguments are `left`, `right`, `bottom`, `top`.
    #[must_use]
    pub fn with_margins(mut self, left: f64, right: f64, bottom: f64, top: f64) -> Self {
        self.left = left;
        self.right = right;
        self.bottom = bottom;
        self.top = top;
        self
    }

    /// Set the inter-cell spacing fractions and return `self`.
    ///
    /// `wspace` is the column gap as a fraction of the average cell width;
    /// `hspace` is the row gap as a fraction of the average cell height.
    #[must_use]
    pub fn with_spacing(mut self, wspace: f64, hspace: f64) -> Self {
        self.wspace = wspace;
        self.hspace = hspace;
        self
    }

    /// Set the relative column widths and return `self`.
    ///
    /// # Panics
    ///
    /// Panics if `ratios.len()` does not equal `ncols`.
    #[must_use]
    pub fn with_width_ratios(mut self, ratios: Vec<f64>) -> Self {
        assert_eq!(
            ratios.len(),
            self.ncols,
            "width_ratios length must equal ncols"
        );
        self.width_ratios = Some(ratios);
        self
    }

    /// Set the relative row heights and return `self`.
    ///
    /// # Panics
    ///
    /// Panics if `ratios.len()` does not equal `nrows`.
    #[must_use]
    pub fn with_height_ratios(mut self, ratios: Vec<f64>) -> Self {
        assert_eq!(
            ratios.len(),
            self.nrows,
            "height_ratios length must equal nrows"
        );
        self.height_ratios = Some(ratios);
        self
    }

    /// Compute the per-cell edge positions in figure-fraction coordinates.
    ///
    /// Returns `(fig_bottoms, fig_tops, fig_lefts, fig_rights)` where
    /// `fig_bottoms[i]` / `fig_tops[i]` are the bottom/top edges of row `i`
    /// (row `0` is the topmost, with the largest `y`), and `fig_lefts[j]` /
    /// `fig_rights[j]` are the left/right edges of column `j` (column `0` is
    /// the leftmost).
    ///
    /// This is a direct port of matplotlib's `GridSpec.get_grid_positions`.
    #[must_use]
    pub fn get_grid_positions(&self) -> (Vec<f64>, Vec<f64>, Vec<f64>, Vec<f64>) {
        let (fig_tops, fig_bottoms) = positions_1d(
            self.nrows,
            self.hspace,
            self.height_ratios.as_deref(),
            self.bottom,
            self.top,
            true,
        );
        let (fig_lefts, fig_rights) = positions_1d(
            self.ncols,
            self.wspace,
            self.width_ratios.as_deref(),
            self.left,
            self.right,
            false,
        );
        (fig_bottoms, fig_tops, fig_lefts, fig_rights)
    }

    /// Create a [`SubplotSpec`] for the single cell at `(row, col)`.
    ///
    /// # Panics
    ///
    /// Panics if `row >= nrows` or `col >= ncols`.
    #[must_use]
    pub fn subplot(&self, row: usize, col: usize) -> SubplotSpec {
        assert!(row < self.nrows, "row out of range");
        assert!(col < self.ncols, "col out of range");
        SubplotSpec {
            rows: (row, row + 1),
            cols: (col, col + 1),
        }
    }

    /// Create a [`SubplotSpec`] spanning the half-open row and column ranges.
    ///
    /// `rows` spans rows `rows.start..rows.end` and `cols` spans columns
    /// `cols.start..cols.end` (both inclusive of the start, exclusive of the
    /// end).
    ///
    /// # Panics
    ///
    /// Panics if either range is empty or extends past the grid bounds.
    #[must_use]
    pub fn subplot_span(
        &self,
        rows: std::ops::Range<usize>,
        cols: std::ops::Range<usize>,
    ) -> SubplotSpec {
        assert!(rows.start < rows.end, "row range must be non-empty");
        assert!(cols.start < cols.end, "col range must be non-empty");
        assert!(rows.end <= self.nrows, "row range out of bounds");
        assert!(cols.end <= self.ncols, "col range out of bounds");
        SubplotSpec {
            rows: (rows.start, rows.end),
            cols: (cols.start, cols.end),
        }
    }
}

/// Compute the start/end edges of each cell along one axis.
///
/// `n` is the number of cells, `space` the gap fraction, `ratios` the optional
/// relative cell sizes, and `lo`/`hi` the figure-fraction bounds along the
/// axis. When `descending` is true (the row axis), cells run from `hi` down to
/// `lo` so that cell `0` has the largest coordinate; the returned pair is then
/// `(near_edges, far_edges)` = `(tops, bottoms)`. When false (the column axis),
/// cells run from `lo` up to `hi` and the pair is `(lefts, rights)`.
fn positions_1d(
    n: usize,
    space: f64,
    ratios: Option<&[f64]>,
    lo: f64,
    hi: f64,
    descending: bool,
) -> (Vec<f64>, Vec<f64>) {
    let nf = n as f64;
    let tot = hi - lo;
    // Size of one cell if all cells were equal, accounting for the gaps.
    let cell = tot / (nf + space * (nf - 1.0));
    let sep = space * cell;

    let equal = vec![1.0; n];
    let ratios = ratios.unwrap_or(&equal);
    let sum_ratios: f64 = ratios.iter().sum();
    // Normalize so the cell sizes again total `cell * n`.
    let norm = cell * nf / sum_ratios;
    let cell_sizes: Vec<f64> = ratios.iter().map(|r| r * norm).collect();

    // Accumulate alternating separators and cells, mirroring matplotlib's
    // cumulative-sum over the interleaved [sep, cell] sequence.
    let mut near = Vec::with_capacity(n);
    let mut far = Vec::with_capacity(n);
    let mut acc = 0.0;
    for (i, size) in cell_sizes.iter().enumerate() {
        if i > 0 {
            acc += sep;
        }
        let start = acc;
        acc += size;
        let end = acc;
        if descending {
            near.push(hi - start);
            far.push(hi - end);
        } else {
            near.push(lo + start);
            far.push(lo + end);
        }
    }
    (near, far)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-12, "expected {b}, got {a}");
    }

    #[test]
    fn single_cell_fills_inner_rectangle() {
        let gs = GridSpec::new(1, 1);
        let (bottoms, tops, lefts, rights) = gs.get_grid_positions();
        assert_eq!(bottoms.len(), 1);
        approx(lefts[0], gs.left);
        approx(rights[0], gs.right);
        approx(bottoms[0], gs.bottom);
        approx(tops[0], gs.top);
    }

    #[test]
    fn two_by_two_ordering_and_gaps() {
        let gs = GridSpec::new(2, 2);
        let (bottoms, tops, lefts, rights) = gs.get_grid_positions();

        // Row 0 is the top row: larger y than row 1.
        assert!(tops[0] > tops[1]);
        assert!(bottoms[0] > bottoms[1]);
        // Column 1 is to the right of column 0.
        assert!(lefts[1] > lefts[0]);
        assert!(rights[1] > rights[0]);

        // Cells are non-overlapping with a positive gap between them.
        let row_gap = bottoms[0] - tops[1];
        let col_gap = lefts[1] - rights[0];
        assert!(row_gap > 0.0, "rows must not overlap");
        assert!(col_gap > 0.0, "cols must not overlap");

        // Gap reflects spacing: hspace/wspace times the average cell size.
        let cell_h = tops[0] - bottoms[0];
        let cell_w = rights[0] - lefts[0];
        approx(row_gap, gs.hspace * cell_h);
        approx(col_gap, gs.wspace * cell_w);

        // Outer edges match the margins.
        approx(tops[0], gs.top);
        approx(bottoms[1], gs.bottom);
        approx(lefts[0], gs.left);
        approx(rights[1], gs.right);
    }

    #[test]
    fn width_ratios_scale_columns() {
        let gs = GridSpec::new(1, 2).with_width_ratios(vec![2.0, 1.0]);
        let (_, _, lefts, rights) = gs.get_grid_positions();
        let left_w = rights[0] - lefts[0];
        let right_w = rights[1] - lefts[1];
        approx(left_w, 2.0 * right_w);
    }

    #[test]
    fn spanning_whole_grid_is_inner_rectangle() {
        let gs = GridSpec::new(3, 4);
        let span = gs.subplot_span(0..3, 0..4);
        let bbox = span.get_position(&gs);
        approx(bbox.xmin(), gs.left);
        approx(bbox.xmax(), gs.right);
        approx(bbox.ymin(), gs.bottom);
        approx(bbox.ymax(), gs.top);
    }
}
