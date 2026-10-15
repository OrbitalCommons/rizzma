"""Reference geometry and ticks for tests/colorbar_reference.rs.

A 6.4 x 4.8 in, 100 dpi figure holds a 2 x 2 grid of axes at fixed figure
rects (so no layout engine is involved). Panel 0 shows an image through a
norm and gets `fig.colorbar(im, ax=ax0)`; the other panels are plain. After
a draw, this prints for each norm:

    <norm> axes <i> x0 y0 x1 y1        every panel's frame (figure fractions)
    <norm> cbar x0 y0 x1 y1            the colorbar's frame after its aspect
    <norm> tick <value> <label>        each major tick inside the range

`aspect="auto"` keeps the image from squaring its axes, which is beside the
point here.

Regenerate with a working matplotlib:
    python -I colorbar_mpl.py > colorbar_mpl.txt
"""
import matplotlib

matplotlib.use("agg")
import matplotlib.colors as mcolors
import matplotlib.pyplot as plt
import numpy as np

RECTS = [(0.08, 0.56, 0.38, 0.38), (0.56, 0.56, 0.38, 0.38),
         (0.08, 0.08, 0.38, 0.38), (0.56, 0.08, 0.38, 0.38)]
NORMS = {
    "linear": mcolors.Normalize(vmin=-200, vmax=600),
    "log": mcolors.LogNorm(vmin=0.3, vmax=300),
    "twoslope": mcolors.TwoSlopeNorm(vmin=-300, vcenter=0, vmax=900),
}
data = np.linspace(-300, 900, 64).reshape(8, 8)

print(f"# matplotlib {matplotlib.__version__}")
for name, norm in NORMS.items():
    fig = plt.figure(figsize=(6.4, 4.8), dpi=100)
    axes = [fig.add_axes(r) for r in RECTS]
    im = axes[0].imshow(np.abs(data) if name == "log" else data, norm=norm, aspect="auto")
    cbar = fig.colorbar(im, ax=axes[0])
    fig.canvas.draw()
    for i, ax in enumerate(axes):
        print(name, "axes", i, *np.round(ax.get_position().extents, 6))
    print(name, "cbar", *np.round(cbar.ax.get_position().extents, 6))
    lo, hi = cbar.ax.get_ylim()
    for value, label in zip(cbar.get_ticks(), cbar.ax.get_yticklabels()):
        if min(lo, hi) - 1e-9 <= value <= max(lo, hi) + 1e-9:
            print(name, "tick", repr(float(value)), label.get_text().replace(" ", "_"))
    plt.close(fig)
