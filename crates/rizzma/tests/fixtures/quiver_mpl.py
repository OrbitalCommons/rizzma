"""Reference arrow outlines for tests in src/figure/plotting_quiver.rs.

matplotlib 3.11's quiver on unequal axes (a 600x350 px figure, axes
[0.1, 0.1, 0.8, 0.8], xlim (-1, 5), ylim (-1, 2)) for each option set below.
For every arrow, its eight outline vertices in display pixels (y up):

    case arrow x0 y0 x1 y1 ... x7 y7

Regenerate with a working matplotlib:
    python -I quiver_mpl.py > quiver_mpl.txt
"""
import matplotlib

matplotlib.use("agg")
import matplotlib.pyplot as plt
import numpy as np

X = np.array([0.0, 1.5, 3.0, 0.0, 1.5, 3.0, 4.2])
Y = np.array([0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.6])
U = np.array([1.0, 0.5, -0.8, 0.02, 1.2, 0.3, 0.001])
V = np.array([0.5, 1.0, 0.4, 0.01, -0.6, 0.3, 0.0])
cases = {
    "auto_width": dict(),
    "s10_width": dict(scale=10),
    "s30_width": dict(scale=30),
    "auto_xy": dict(units="xy"),
    "s10_xy": dict(units="xy", scale=10),
    "s30_xy": dict(units="xy", scale=30),
    "xy_xy_1": dict(scale_units="xy", angles="xy", scale=1),
    "mid_inches": dict(units="inches", width=0.02, pivot="middle", scale=8),
}
print(f"# matplotlib {matplotlib.__version__}")
for name, kw in cases.items():
    fig = plt.figure(figsize=(6, 3.5), dpi=100)
    ax = fig.add_axes([0.1, 0.1, 0.8, 0.8])
    q = ax.quiver(X, Y, U, V, **kw)
    ax.set_xlim(-1, 5)
    ax.set_ylim(-1, 2)
    fig.canvas.draw()
    base = ax.transData.transform(np.c_[X, Y])
    trans = q.get_transform()
    for i, path in enumerate(q.get_paths()):
        pts = trans.transform(path.vertices) + base[i]
        print(name, i, " ".join(repr(float(c)) for c in pts.ravel()))
    plt.close(fig)
