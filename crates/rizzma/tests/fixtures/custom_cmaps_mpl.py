"""References for tests/custom_colormaps.rs.

1. starfield_hsl.txt: the 256 lookup-table colors of OrbitalCommons/starfield's
   B-V ramp, plotters' HSLColor(0.7 - 0.7 * t, 0.65, 0.45) at t = i / 255,
   converted as plotters 0.3.7 does (standard HSL, each channel
   (value * 255).round(), half away from zero), via colorsys as an
   independent implementation. One "r g b" row per entry.
2. listed5.png: matplotlib's imshow of a 16x16 field spanning [0, 1] through
   ListedColormap of five colors, 4 px per cell; the center pixel of every
   cell, a 16x16 RGB image, row 0 first.

Regenerate with a working matplotlib:
    python -I custom_cmaps_mpl.py
"""
import colorsys
import math

import matplotlib

matplotlib.use("agg")
import matplotlib.image as mimage
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.colors import ListedColormap

with open("starfield_hsl.txt", "w") as out:
    for i in range(256):
        t = i / 255
        r, g, b = colorsys.hls_to_rgb(0.7 - 0.7 * t, 0.45, 0.65)
        out.write(" ".join(str(math.floor(v * 255 + 0.5)) for v in (r, g, b)) + "\n")

N = 16
field = (np.arange(N * N).reshape(N, N) % 37) / 36.0
colors = ["#440154", "#3b528b", "#21918c", "#5ec962", "#fde725"]
fig = plt.figure(figsize=(N * 4 / 100, N * 4 / 100), dpi=100)
ax = fig.add_axes([0, 0, 1, 1])
ax.set_axis_off()
ax.imshow(field, cmap=ListedColormap(colors), vmin=0, vmax=1,
          interpolation="nearest", interpolation_stage="rgba")
fig.canvas.draw()
rgba = np.asarray(fig.canvas.buffer_rgba())
plt.close(fig)
mimage.imsave("listed5.png", rgba[2::4, 2::4, :3].copy())
print("matplotlib", matplotlib.__version__)
