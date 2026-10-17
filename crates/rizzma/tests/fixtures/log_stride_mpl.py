"""Reference log major/minor ticks over many decades, for the test in
src/figure/axes.rs.

matplotlib 3.11's default log axes (6.4x4.8 in, 100 dpi) over each range,
on x and on y. Every major tick in view, then every minor tick in view:

    case axis major value
    case axis minor value

Regenerate with a working matplotlib:
    python -I log_stride_mpl.py > log_stride_mpl.txt
"""
import matplotlib

matplotlib.use("agg")
import matplotlib.pyplot as plt

CASES = [("1em6_1e6", 1e-6, 1e6), ("1_1e12", 1.0, 1e12), ("1_1e30", 1.0, 1e30),
         ("1_1e9", 1.0, 1e9), ("2_8", 2.0, 8.0)]

for name, lo, hi in CASES:
    for axis_name in ("x", "y"):
        fig, ax = plt.subplots()
        getattr(ax, f"set_{axis_name}scale")("log")
        getattr(ax, f"set_{axis_name}lim")(lo, hi)
        fig.canvas.draw()
        axis = getattr(ax, f"{axis_name}axis")
        inside = lambda v: lo * (1 - 1e-9) <= v <= hi * (1 + 1e-9)
        for v in axis.get_majorticklocs():
            if inside(v):
                print(f"{name} {axis_name} major {float(v)!r}")
        for v in axis.get_minorticklocs():
            if inside(v):
                print(f"{name} {axis_name} minor {float(v)!r}")
        plt.close(fig)
