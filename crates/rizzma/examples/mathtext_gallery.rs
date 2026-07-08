//! A "formula sheet": famous equations rendered through rizzma's mathtext engine
//! as centered titles over hidden axes. Exercises fractions, radicals, integrals
//! and sums with limits, matrices, delimiters, accents, Greek, and blackboard/
//! script fonts. Writes `target/mathtext_gallery.png`.

use rizzma::figure::Figure;

fn main() {
    // Each entry is a single inline-math title ($...$). Raw strings keep the
    // TeX backslashes readable.
    let equations: &[&str] = &[
        r"$e^{i\pi} + 1 = 0$",
        r"$\int_{-\infty}^{\infty} e^{-x^2}\,dx = \sqrt{\pi}$",
        r"$x = \frac{-b \pm \sqrt{b^{2} - 4ac}}{2a}$",
        r"$\sum_{n=1}^{\infty} \frac{1}{n^{2}} = \frac{\pi^{2}}{6}$",
        r"$f(x) = \frac{1}{\sigma\sqrt{2\pi}}\, e^{-\frac{(x-\mu)^{2}}{2\sigma^{2}}}$",
        r"$\binom{n}{k} = \frac{n!}{k!\,(n-k)!}$",
        r"$\nabla \times \vec{B} = \mu_0 \vec{J} + \mu_0\varepsilon_0 \frac{\partial \vec{E}}{\partial t}$",
        r"$A = \begin{pmatrix} a & b \\ c & d \end{pmatrix},\quad \det A = ad - bc$",
        r"$\hat{f}(\xi) = \int_{-\infty}^{\infty} f(x)\, e^{-2\pi i x \xi}\,dx$",
        r"$\mathbb{E}[X] = \sum_{x} x\, \mathbb{P}(X = x) \in \mathbb{R}$",
    ];

    let n = equations.len();
    let mut fig = Figure::new(8.0, 0.9 * n as f64);

    for (i, eq) in equations.iter().enumerate() {
        // Vertical center of this equation's horizontal band (figure fractions,
        // bottom-origin), placed over a thin, invisible axes.
        let center_y = 1.0 - (i as f64 + 0.5) / n as f64;
        let ax = fig.add_axes(0.04, center_y, 0.92, 0.001);
        ax.set_axis_off();
        ax.set_title(*eq);
    }

    let path = "target/mathtext_gallery.png";
    fig.save_png(path).expect("save PNG");
    println!("wrote {path}");
}
