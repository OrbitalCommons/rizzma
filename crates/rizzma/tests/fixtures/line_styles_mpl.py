"""Reference line ink for the test in tests/line_styles.rs.

matplotlib 3.11 draws a black zig-zag (x 0.2..0.8, y between 0.3 and 0.7) at
4 pt on a 200x200 px figure (100 dpi, axes filling it, limits 0..1, axis
off), solid and dashed. For each case, sub-pixel ink extents and the
ink-weighted centroid, measured as in markers_mpl.py:

    case x0 y0 x1 y1 cx cy

Regenerate with a working matplotlib:
    python -I line_styles_mpl.py > line_styles_mpl.txt
"""
import matplotlib

matplotlib.use("agg")
import matplotlib.pyplot as plt
import numpy as np

X = [0.2, 0.4, 0.6, 0.8]
Y = [0.3, 0.7, 0.3, 0.7]

for case, style in [("solid", "-"), ("dashed", "--")]:
    fig = plt.figure(figsize=(2, 2), dpi=100)
    ax = fig.add_axes([0, 0, 1, 1])
    ax.set_axis_off()
    ax.plot(X, Y, style, color="k", linewidth=4)
    ax.set_xlim(0, 1)
    ax.set_ylim(0, 1)
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
    print(f"{case} {x0:.4f} {y0:.4f} {x1:.4f} {y1:.4f} {cx:.4f} {cy:.4f}")
    plt.close(fig)
