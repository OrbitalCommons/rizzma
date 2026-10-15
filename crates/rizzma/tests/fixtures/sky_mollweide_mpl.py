"""Reference render for tests/sky_projection.rs: matplotlib's Mollweide
pcolormesh.

A 2° longitude/latitude grid (180 x 90 cells) of a smooth field drawn with
viridis on a projection="mollweide" axes filling a 600x300 px figure's
[0.05, 0.05, 0.9, 0.9] rect, tick labels and grid off. The whole canvas is
written to sky_mollweide_mesh.png (RGB).

Regenerate with a working matplotlib:
    python -I sky_mollweide_mpl.py
"""
import matplotlib

matplotlib.use("agg")
import matplotlib.image as mimage
import matplotlib.pyplot as plt
import numpy as np

lon = np.radians(np.arange(-180, 181, 2))
lat = np.radians(np.arange(-90, 91, 2))
clon = 0.5 * (lon[:-1] + lon[1:])
clat = 0.5 * (lat[:-1] + lat[1:])
c = np.sin(2 * clon)[None, :] * np.cos(clat)[:, None] + 0.5 * np.sin(3 * clat)[:, None]

fig = plt.figure(figsize=(6, 3), dpi=100)
ax = fig.add_axes([0.05, 0.05, 0.9, 0.9], projection="mollweide")
ax.pcolormesh(lon, lat, c, cmap="viridis")
ax.set_xticklabels([])
ax.set_yticklabels([])
ax.grid(False)
fig.canvas.draw()
rgba = np.asarray(fig.canvas.buffer_rgba())
plt.close(fig)
mimage.imsave("sky_mollweide_mesh.png", rgba[:, :, :3].copy())
print("matplotlib", matplotlib.__version__)
