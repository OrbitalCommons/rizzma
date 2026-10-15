"""Reference render for tests/sky_projection.rs: matplotlib's PolyCollection
on a Mollweide axes.

A 10° longitude/latitude grid of quadrilateral polygons (36 x 18), colored
through viridis by PolyCollection.set_array, added to a projection="mollweide"
axes filling a 600x300 px figure's [0.05, 0.05, 0.9, 0.9] rect, tick labels
and grid off. The whole canvas is written to sky_polys.png (RGB).

Regenerate with a working matplotlib:
    python -I sky_polys_mpl.py
"""
import matplotlib

matplotlib.use("agg")
import matplotlib.image as mimage
import matplotlib.pyplot as plt
import numpy as np
from matplotlib.collections import PolyCollection

lon = np.radians(np.arange(-180, 181, 10))
lat = np.radians(np.arange(-90, 91, 10))
polys, values = [], []
for j in range(len(lat) - 1):
    for i in range(len(lon) - 1):
        polys.append([(lon[i], lat[j]), (lon[i + 1], lat[j]),
                      (lon[i + 1], lat[j + 1]), (lon[i], lat[j + 1])])
        lo, la = 0.5 * (lon[i] + lon[i + 1]), 0.5 * (lat[j] + lat[j + 1])
        values.append(np.sin(2 * lo) * np.cos(la) + 0.5 * np.sin(3 * la))

fig = plt.figure(figsize=(6, 3), dpi=100)
ax = fig.add_axes([0.05, 0.05, 0.9, 0.9], projection="mollweide")
coll = PolyCollection(polys, cmap="viridis")
coll.set_array(np.array(values))
ax.add_collection(coll)
ax.set_xticklabels([])
ax.set_yticklabels([])
ax.grid(False)
fig.canvas.draw()
rgba = np.asarray(fig.canvas.buffer_rgba())
plt.close(fig)
mimage.imsave("sky_polys.png", rgba[:, :, :3].copy())
print("matplotlib", matplotlib.__version__)
