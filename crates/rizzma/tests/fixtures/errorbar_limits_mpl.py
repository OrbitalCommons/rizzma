"""Reference caret placement for tests/errorbar_limits.rs.

A mixed series — detections, upper limits and a lower limit, all black, no
caps — is drawn by matplotlib's errorbar at 72, 100 and 200 dpi on a fixed
axes rect and limits. For every limit point the caret is measured inside a
window around the bar's far end (where matplotlib puts it): the columns its
ink spans and the row of its tip. One line per caret:

    dpi index kind x0 x1 tip

(pixel columns/rows, top-left origin, inclusive; kind is "up" for an upper
limit, whose caret points down from y - err, or "lo" for a lower limit).

Regenerate with a working matplotlib:
    python -I errorbar_limits_mpl.py > errorbar_limits_mpl.txt
"""
import matplotlib

matplotlib.use("agg")
import matplotlib.pyplot as plt
import numpy as np

X = [1.0, 2.0, 3.0, 4.0, 5.0]
Y = [5.0, 6.0, 5.0, 6.0, 5.0]
YERR = [2.0] * 5
UPLIMS = [False, True, False, True, False]
LOLIMS = [False, False, True, False, False]

print(f"# matplotlib {matplotlib.__version__}")
for dpi in (72, 100, 200):
    fig = plt.figure(figsize=(4, 3), dpi=dpi)
    ax = fig.add_axes([0.1, 0.1, 0.8, 0.8])
    ax.set_xlim(0, 6)
    ax.set_ylim(0, 10)
    ax.set_axis_off()
    ax.errorbar(X, Y, yerr=YERR, uplims=UPLIMS, lolims=LOLIMS, color="k", ecolor="k")
    fig.canvas.draw()
    rgba = np.asarray(fig.canvas.buffer_rgba())
    height = rgba.shape[0]
    pt = dpi / 72.0
    for i in range(len(X)):
        if not (UPLIMS[i] or LOLIMS[i]):
            continue
        up = UPLIMS[i]
        end = Y[i] - YERR[i] if up else Y[i] + YERR[i]
        cx, cy = ax.transData.transform((X[i], end))
        cy = height - cy  # top-left rows
        half = 8 * pt
        if up:
            rows = (int(cy - 1 * pt), int(np.ceil(cy + 8 * pt)))
        else:
            rows = (int(cy - 8 * pt), int(np.ceil(cy + 1 * pt)))
        cols = (int(cx - half), int(np.ceil(cx + half)))
        win = rgba[rows[0]:rows[1] + 1, cols[0]:cols[1] + 1, :3].mean(axis=2) < 128
        ys, xs = np.nonzero(win)
        x0, x1 = xs.min() + cols[0], xs.max() + cols[0]
        tip = (ys.max() if up else ys.min()) + rows[0]
        print(dpi, i, "up" if up else "lo", x0, x1, tip)
    plt.close(fig)
