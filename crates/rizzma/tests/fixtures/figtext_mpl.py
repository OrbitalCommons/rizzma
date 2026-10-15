"""Reference ink boxes for `Figure::text` (tests/figtext_reference.rs).

Renders `fig.text` with matplotlib for each anchor at 100 and 200 dpi and
prints the bounding box of the inked pixels, one case per line:

    dpi x y ha va rotation  x0 y0 x1 y1

(pixel columns/rows, top-left origin, inclusive). rizzma sizes text in px at
100 dpi, so a rizzma size of SIZE_PX is matplotlib's SIZE_PX * 72 / 100 pt.

The string is all straight stems and bars, so its ink edges sit where the
layout puts them in both libraries. matplotlib rasterizes through FreeType's
autohinter, which reshapes curves and shortens descenders by a pixel or two at
these sizes; that is glyph shape, not placement, and would swamp a 1 px check.

Regenerate with a working matplotlib:
    python -I figtext_mpl.py > figtext_mpl.txt
"""
import matplotlib

matplotlib.use("agg")
import matplotlib.pyplot as plt
import numpy as np

SIZE_PX = 20.0
TEXT = "HELL"
CASES = [
    (0.05, 0.05, "left", "bottom", 0),
    (0.95, 0.05, "right", "bottom", 0),
    (0.05, 0.95, "left", "top", 0),
    (0.95, 0.95, "right", "top", 0),
    (0.5, 0.5, "center", "center", 0),
    (0.5, 0.5, "center", "center", 90),
]

print(f"# matplotlib {matplotlib.__version__}, text {TEXT!r}, {SIZE_PX} px at 100 dpi")
for dpi in (100, 200):
    for x, y, ha, va, rot in CASES:
        fig = plt.figure(figsize=(4, 3), dpi=dpi)
        fig.text(x, y, TEXT, ha=ha, va=va, rotation=rot, fontsize=SIZE_PX * 72 / 100)
        fig.canvas.draw()
        rgba = np.asarray(fig.canvas.buffer_rgba())
        plt.close(fig)
        ink = np.argwhere(rgba[:, :, :3].mean(axis=2) < 128)
        (y0, x0), (y1, x1) = ink.min(axis=0), ink.max(axis=0)
        print(dpi, x, y, ha, va, rot, x0, y0, x1, y1)
