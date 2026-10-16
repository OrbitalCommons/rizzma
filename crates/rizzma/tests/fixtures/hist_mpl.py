"""Reference counts and step outlines for tests/hist_options.rs.

matplotlib 3.11's Axes.hist on a fixed sample (data and weights below):
`counts <case>` rows hold the returned n for each option set, and
`path <histtype>` rows the vertices of the step / stepfilled patch, one
"x y" pair per row after a header giving the vertex count.

Regenerate with a working matplotlib:
    python -I hist_mpl.py > hist_mpl.txt
"""
import matplotlib

matplotlib.use("agg")
import matplotlib.pyplot as plt
import numpy as np

data = np.array([0.3, 1.7, 2.2, 2.2, 2.9, 3.4, 3.4, 3.4, 4.1, 4.8, 5.5, 6.0, 7.25, 9.9])
weights = np.linspace(0.5, 2.0, len(data))
cases = {
    "count8": dict(bins=8),
    "edges": dict(bins=[0.0, 1.0, 2.5, 3.4, 5.0, 10.0]),
    "range": dict(bins=5, range=(2.0, 7.0)),
    "density": dict(bins=8, density=True),
    "cumulative": dict(bins=8, cumulative=True),
    "density_cumulative": dict(bins=[0.0, 1.0, 2.5, 3.4, 5.0, 10.0], density=True, cumulative=True),
    "weights": dict(bins=6, weights=weights),
    "weights_density": dict(bins=[0.0, 1.0, 2.5, 3.4, 5.0, 10.0], weights=weights, density=True),
}
print(f"# matplotlib {matplotlib.__version__}")
for name, kw in cases.items():
    fig, ax = plt.subplots()
    n, edges, _ = ax.hist(data, **kw)
    plt.close(fig)
    print("counts", name, " ".join(repr(float(v)) for v in n))
    print("edges", name, " ".join(repr(float(v)) for v in edges))
for histtype in ("step", "stepfilled"):
    fig, ax = plt.subplots()
    _, _, patches = ax.hist(data, bins=[0.0, 1.0, 2.5, 3.4, 5.0, 10.0], histtype=histtype)
    v = patches[0].get_path().vertices
    plt.close(fig)
    print("path", histtype, len(v))
    for x, y in v:
        print(repr(float(x)), repr(float(y)))
