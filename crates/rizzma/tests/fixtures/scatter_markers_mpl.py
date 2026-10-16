"""Reference scatter-marker ink for the test in tests/marker_geometry.rs.

matplotlib 3.11 scatters one marker of 12 pt across (s=144), black face and a
1 pt black edge, at the center of a 100x100 px figure (100 dpi, axes filling
it, limits -1..1, axis off). For every marker code, sub-pixel ink extents and
the ink-weighted centroid, in pixels from the top-left, measured as in
markers_mpl.py:

    code x0 y0 x1 y1 cx cy

Regenerate with a working matplotlib:
    python -I scatter_markers_mpl.py > scatter_markers_mpl.txt
"""
import matplotlib

matplotlib.use("agg")
import matplotlib.pyplot as plt
import numpy as np

CODES = "osD^v<>dph*"

for code in CODES:
    fig = plt.figure(figsize=(1, 1), dpi=100)
    ax = fig.add_axes([0, 0, 1, 1])
    ax.set_axis_off()
    ax.scatter([0], [0], s=144, marker=code, c="k", edgecolors="k", linewidths=1.0)
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
    print(f"{code} {x0:.4f} {y0:.4f} {x1:.4f} {y1:.4f} {cx:.4f} {cy:.4f}")
    plt.close(fig)
