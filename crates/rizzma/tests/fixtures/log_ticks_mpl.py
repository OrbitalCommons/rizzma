"""Reference log-axis ticks for the test in tests/log_ticks.rs.

matplotlib 3.11's default log y axis (6.4x4.8 in, 100 dpi) over each range
below. For every major tick in view, its value and label (mathtext with
`\\mathdefault{..}` unwrapped); then every minor tick value in view:

    case major value label
    case minor value

Regenerate with a working matplotlib:
    python -I log_ticks_mpl.py > log_ticks_mpl.txt
"""
import matplotlib

matplotlib.use("agg")
import matplotlib.pyplot as plt

CASES = [
    ("b10_1e1_1e6", 10, 1e1, 1e6),
    ("b10_1em3_1", 10, 1e-3, 1.0),
    ("b2_16_1024", 2, 16, 1024),
]

for name, base, lo, hi in CASES:
    fig, ax = plt.subplots()
    ax.set_yscale("log", base=base)
    ax.set_ylim(lo, hi)
    fig.canvas.draw()
    for tick in ax.yaxis.get_major_ticks():
        value = tick.get_loc()
        if not lo * (1 - 1e-9) <= value <= hi * (1 + 1e-9):
            continue
        label = tick.label1.get_text().replace("\\mathdefault{", "").replace("}$", "$")
        label = label.replace("$$", "$")
        print(f"{name} major {float(value)!r} {label}")
    for value in ax.yaxis.get_minorticklocs():
        if lo * (1 - 1e-9) <= value <= hi * (1 + 1e-9):
            print(f"{name} minor {float(value)!r}")
    plt.close(fig)
