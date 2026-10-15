"""Reference colors for tests/imshow_norms.rs: matplotlib's imshow(norm=...).

The 64x64 field spans four decades of both signs, with NaN holes and values
beyond every norm's limits, and each norm draws through a colormap with bad,
over and under colors set. Each figure is 256 px square with the image filling
it (4 px per cell); the center pixel of every cell is written to
imshow_norm_<name>.png, a 64x64 RGB image, row 0 first.

interpolation_stage="rgba" colors the original values before the (nearest)
resampling, so the reference holds exactly the colors matplotlib assigns.

Regenerate with a working matplotlib:
    python -I imshow_norms_mpl.py
"""
import matplotlib

matplotlib.use("agg")
import matplotlib.colors as mcolors
import matplotlib.image as mimage
import matplotlib.pyplot as plt
import numpy as np

N = 64
r, c = np.mgrid[0:N, 0:N]
mag = 10.0 ** (-1.0 + 4.0 * c / (N - 1))
sign = np.where(r < 40, 1.0, -1.0)
data = sign * mag * (1.0 + 0.3 * np.sin(r))
data[(r % 9 == 4) & (c % 7 == 3)] = np.nan

EXTREMES = dict(bad="#ff00ff", over="#00ff00", under="#ffff00")
NORMS = {
    "linear": ("viridis", mcolors.Normalize(vmin=-200, vmax=600)),
    "log": ("viridis", mcolors.LogNorm(vmin=0.3, vmax=300)),
    "power": ("magma", mcolors.PowerNorm(gamma=0.5, vmin=-100, vmax=800)),
    "twoslope": ("coolwarm", mcolors.TwoSlopeNorm(vmin=-300, vcenter=0, vmax=900)),
    "centered": ("coolwarm", mcolors.CenteredNorm(vcenter=50, halfrange=400)),
    "symlog": ("RdBu", mcolors.SymLogNorm(linthresh=1, linscale=1, vmin=-500, vmax=500, base=10)),
    "boundary": (
        "viridis",
        mcolors.BoundaryNorm([-100, -10, -1, 0, 1, 10, 100, 1000], ncolors=256),
    ),
}

for name, (cmap, norm) in NORMS.items():
    fig = plt.figure(figsize=(N * 4 / 100, N * 4 / 100), dpi=100)
    ax = fig.add_axes([0, 0, 1, 1])
    ax.set_axis_off()
    ax.imshow(
        data,
        norm=norm,
        cmap=matplotlib.colormaps[cmap].with_extremes(**EXTREMES),
        interpolation="nearest",
        interpolation_stage="rgba",
    )
    fig.canvas.draw()
    rgba = np.asarray(fig.canvas.buffer_rgba())
    plt.close(fig)
    cells = rgba[2::4, 2::4, :3].copy()
    assert cells.shape == (N, N, 3)
    mimage.imsave(f"imshow_norm_{name}.png", cells)
print("matplotlib", matplotlib.__version__)
