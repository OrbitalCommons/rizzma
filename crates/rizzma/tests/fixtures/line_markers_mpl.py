"""Reference marker geometry for tests/line_markers.rs.

matplotlib 3.11 draws plot(x, y, marker=code) at 100 DPI in a 400x300 px
figure (axes [0.1, 0.1, 0.8, 0.8], axis off, fixed limits). The line is white
and the markers C0, so only the markers ink the canvas. For each marker code
and data point, line_markers_mpl.txt holds the ink centroid (darkness
weighted, pixel centers, y down) and the inked bounding box:

    code index cx cy xmin xmax ymin ymax

Regenerate with a working matplotlib:
    python -I line_markers_mpl.py > line_markers_mpl.txt
"""
import matplotlib

matplotlib.use("agg")
import matplotlib.pyplot as plt
import numpy as np

X = [0.3, 1.1, 2.0, 2.7, 3.6, 4.4, 5.2]
Y = [1.0, 2.6, 1.9, 3.4, 2.2, 3.9, 3.1]
print(f"# matplotlib {matplotlib.__version__}")
for code in "osx":
    fig = plt.figure(figsize=(4, 3), dpi=100)
    ax = fig.add_axes([0.1, 0.1, 0.8, 0.8])
    ax.set_axis_off()
    ax.plot(X, Y, "-", marker=code, color="white", mfc="C0", mec="C0")
    ax.set_xlim(0, 5.5)
    ax.set_ylim(0.5, 4.5)
    fig.canvas.draw()
    rgb = np.asarray(fig.canvas.buffer_rgba())[:, :, :3].astype(float)
    pts = ax.transData.transform(np.c_[X, Y])
    plt.close(fig)
    dark = (255.0 * 3 - rgb.sum(axis=2))
    yy, xx = np.mgrid[0 : dark.shape[0], 0 : dark.shape[1]]
    for i, (px, py) in enumerate(pts):
        cx, cy = int(px), int(300 - py)
        win = np.zeros_like(dark, dtype=bool)
        win[cy - 9 : cy + 10, cx - 9 : cx + 10] = True
        w = dark * win
        ink = (w > 0)
        ys, xs = np.nonzero(ink)
        print(code, i,
              f"{(w * (xx + 0.5)).sum() / w.sum():.4f}",
              f"{(w * (yy + 0.5)).sum() / w.sum():.4f}",
              xs.min(), xs.max(), ys.min(), ys.max())
