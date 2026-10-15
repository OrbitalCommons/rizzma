"""Reference output of matplotlib's PathSimplifier for tests/path_simplify.rs.

A noisy display-space polyline (a light curve squeezed into 1600 px, so many
vertices share each pixel column), with a NaN gap so the path has a pen lift,
goes through `Path.cleaned(remove_nans=True, simplify=True)` — the NaN
remover Agg runs before it, then the C++ PathSimplifier with
path.simplify_threshold = 1/9. Writes the input and the output as raw
little-endian float64 (x, y pairs), with the output's codes as one byte each.

Regenerate with a working matplotlib:
    python -I simplify_mpl.py
"""
import matplotlib
import numpy as np
from matplotlib.path import Path

rng = np.random.default_rng(329)
n = 20000
x = 100.0 + 1400.0 * np.arange(n) / n
y = 600.0 + 300.0 * np.sin(x / 140.0) + rng.uniform(-40, 40, n)
y[9000:9010] = np.nan
verts = np.column_stack([x, y])
verts.astype("<f8").tofile("simplify_in.f64")

path = Path(verts)
path.should_simplify = True
out = path.cleaned(remove_nans=True, simplify=True)
keep = out.codes != Path.STOP
out.vertices[keep].astype("<f8").tofile("simplify_out.f64")
out.codes[keep].astype("u1").tofile("simplify_out_codes.u8")
print("matplotlib", matplotlib.__version__, "in", n, "out", int(keep.sum()))
