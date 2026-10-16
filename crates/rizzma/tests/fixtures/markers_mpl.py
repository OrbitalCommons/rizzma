"""Reference marker ink for the test in tests/marker_geometry.rs.

matplotlib 3.11 draws one black `plot` marker of 12 pt at the center of a
100x100 px figure (100 dpi, axes filling it, limits -1..1, axis off). For
every marker code, the ink (1 - luminance) gives sub-pixel ink extents and
the ink-weighted centroid, in pixels from the top-left:

    code filled x0 y0 x1 y1 cx cy

A pixel `(x, y)` with ink `c` puts a left edge at `x + 1 - c` and a right
edge at `x + c` (likewise for top and bottom); the extents are the outermost
over all inked pixels.

Regenerate with a working matplotlib:
    python -I markers_mpl.py > markers_mpl.txt
"""
import matplotlib

matplotlib.use("agg")
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.markers import MarkerStyle

CODES = "o.,s^v<>DdphH*+x"

for code in CODES:
    fig = plt.figure(figsize=(1, 1), dpi=100)
    ax = fig.add_axes([0, 0, 1, 1])
    ax.set_axis_off()
    ax.plot([0], [0], marker=code, markersize=12, linestyle="none", color="k")
    ax.set_xlim(-1, 1)
    ax.set_ylim(-1, 1)
    fig.canvas.draw()
    rgb = np.asarray(fig.canvas.buffer_rgba())[..., :3].astype(float) / 255
    ink = 1 - rgb.mean(axis=2)
    ys, xs = np.nonzero(ink > 0)
    c = ink[ys, xs]
    x0, x1 = (xs + 1 - c).min(), (xs + c).max()
    y0, y1 = (ys + 1 - c).min(), (ys + c).max()
    yy, xx = np.indices(ink.shape)
    total = ink.sum()
    cx = (ink * (xx + 0.5)).sum() / total
    cy = (ink * (yy + 0.5)).sum() / total
    filled = int(MarkerStyle(code).is_filled())
    print(
        f"{code} {filled} {x0:.4f} {y0:.4f} {x1:.4f} {y1:.4f} {cx:.4f} {cy:.4f}"
    )
    plt.close(fig)
