"""Reference colormap lookups for tests/cmap_lookup.rs.

matplotlib picks lookup-table entry floor(t * N) (the last entry at t = 1).
Samples t = k / 512 hit every one of the 256 entries twice, including the
exact bin edges t = j / 256, and print the 8-bit RGB matplotlib produces:

    cmap t r g b

`cmap` is the rizzma registry name; the quarantined vendor maps (jet) are
looked up in matplotlib under their bare name.

Regenerate with a working matplotlib:
    python -I cmap_lookup_mpl.py > cmap_lookup_mpl.txt
"""
import matplotlib
import numpy as np

print(f"# matplotlib {matplotlib.__version__}")
for name, mpl_name in (
    ("viridis", "viridis"),
    ("coolwarm", "coolwarm"),
    ("misleading:jet", "jet"),
):
    cmap = matplotlib.colormaps[mpl_name]
    ts = np.arange(513) / 512
    for t, (r, g, b, _) in zip(ts, cmap(ts, bytes=True)):
        print(name, repr(float(t)), r, g, b)
