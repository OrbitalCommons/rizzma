"""Reference values for tests/astro_normalize.rs, from astropy and matplotlib.

Writes, next to this script:

- star.f64, nebula.f64, holes.f64: three 64x64 test images as raw
  little-endian float64, row-major: a star field (sky noise plus point
  sources and a saturated star), a nebula (smooth extended emission plus
  noise), and the star field with NaN columns and blobs.
- zscale.txt: astropy's ZScaleInterval limits for each image, and the
  Percentile / AsymmetricPercentile / MinMax limits of the star field.
- stretches.f64: every stretch's forward transform on 2001 points spanning
  [0, 1], one stretch after another in the order listed in stretches.txt.
- render_zscale_asinh.png: matplotlib's imshow of the star field through
  ImageNormalize(interval=ZScaleInterval(), stretch=AsinhStretch(0.1)),
  one RGB pixel per cell (see tests/fixtures/imshow_norms_mpl.py for the
  render-and-sample method).

Regenerate with working astropy and matplotlib:
    python -I astro_mpl.py
"""
import astropy
import matplotlib

matplotlib.use("agg")
import matplotlib.image as mimage
import matplotlib.pyplot as plt
import numpy as np
from astropy.visualization import (
    AsinhStretch, AsymmetricPercentileInterval, HistEqStretch, ImageNormalize,
    LinearStretch, LogStretch, MinMaxInterval, PercentileInterval, PowerStretch,
    SinhStretch, SqrtStretch, ZScaleInterval,
)

N = 64
rng = np.random.default_rng(327)
yy, xx = np.mgrid[0:N, 0:N]

def stars(field, count, amp_lo, amp_hi, sigma):
    for _ in range(count):
        cx, cy = rng.uniform(0, N, 2)
        amp = 10 ** rng.uniform(np.log10(amp_lo), np.log10(amp_hi))
        field += amp * np.exp(-((xx - cx) ** 2 + (yy - cy) ** 2) / (2 * sigma**2))
    return field

star = 100.0 + rng.normal(0, 5, (N, N))
star = stars(star, 40, 20, 5000, 1.2)
star[20:23, 40:43] = 65000.0  # a saturated core

nebula = 50.0 + 400.0 * np.exp(-(((xx - 30) / 14) ** 2 + ((yy - 34) / 9) ** 2))
nebula += 150.0 * np.exp(-(((xx - 45) / 6) ** 2 + ((yy - 18) / 11) ** 2))
nebula += rng.normal(0, 8, (N, N))

holes = star.copy()
holes[:, 10] = np.nan
holes[40:46, 30:36] = np.nan

for name, img in [("star", star), ("nebula", nebula), ("holes", holes)]:
    img.astype("<f8").tofile(f"{name}.f64")

with open("zscale.txt", "w") as out:
    out.write(f"# astropy {astropy.__version__}\n")
    for name, img in [("star", star), ("nebula", nebula), ("holes", holes)]:
        lo, hi = ZScaleInterval().get_limits(img)
        out.write(f"zscale {name} {float(lo)!r} {float(hi)!r}\n")
    for label, interval in [
        ("minmax", MinMaxInterval()),
        ("percentile99.5", PercentileInterval(99.5)),
        ("asymmetric1-99.9", AsymmetricPercentileInterval(1.0, 99.9)),
    ]:
        lo, hi = interval.get_limits(star)
        out.write(f"{label} star {float(lo)!r} {float(hi)!r}\n")

grid = np.linspace(0.0, 1.0, 2001)
STRETCHES = [
    ("linear", LinearStretch(slope=0.8, intercept=0.1)),
    ("sqrt", SqrtStretch()),
    ("power", PowerStretch(2.5)),
    ("log", LogStretch(a=1000.0)),
    ("asinh", AsinhStretch(a=0.1)),
    ("sinh", SinhStretch(a=1.0 / 3.0)),
    ("histeq", HistEqStretch(star)),
]
with open("stretches.txt", "w") as out:
    out.write("# stretches.f64 holds these forward transforms on linspace(0, 1, 2001)\n")
    curves = []
    for name, stretch in STRETCHES:
        out.write(name + "\n")
        curves.append(stretch(grid.copy(), clip=False))
    np.concatenate(curves).astype("<f8").tofile("stretches.f64")

norm = ImageNormalize(star, interval=ZScaleInterval(), stretch=AsinhStretch(0.1))
fig = plt.figure(figsize=(N * 4 / 100, N * 4 / 100), dpi=100)
ax = fig.add_axes([0, 0, 1, 1])
ax.set_axis_off()
ax.imshow(star, norm=norm, cmap="gray", interpolation="nearest", interpolation_stage="rgba")
fig.canvas.draw()
cells = np.asarray(fig.canvas.buffer_rgba())[2::4, 2::4, :3].copy()
mimage.imsave("render_zscale_asinh.png", cells)
print("astropy", astropy.__version__, "matplotlib", matplotlib.__version__)
