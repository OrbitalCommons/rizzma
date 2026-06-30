//! Mathtext for rizzma.
//!
//! A small, deterministic TeX-subset box-and-glue engine for math spans. It
//! parses inline math content into a compact layout tree, positions glyph
//! outlines from [`rizzma_text::FontSource`], and emits backend-independent
//! [`rizzma_core::Path`] geometry. Integration into figure text artists is a
//! later step; this crate owns only parsing and layout.
//!
//! Supported in this first pass: ordinary symbols, whitespace glue, `{...}`
//! groups, superscripts/subscripts, `\frac{...}{...}`, and a small table of
//! common named symbols. Unsupported commands are preserved as literal fallback
//! text and reported as structured warnings.
//!
//! This is intentionally a scoped approximation: it uses the embedded DejaVu
//! Sans face for wasm-clean, deterministic glyph geometry, so it does not yet
//! provide math italic, dedicated math-font metrics, stretch delimiters, or
//! publication-grade TeX spacing.
//!
//! Build-order home: Phase 10 of `design/04-implementation-plan.md`.

use rizzma_core::{Affine2D, Path};
use rizzma_text::{FontSource, TextSpan, TextSpanKind};

const SCRIPT_SCALE: f64 = 0.7;
const SCRIPT_GAP_EM: f64 = 0.08;
const FRAC_GAP_EM: f64 = 0.18;
const FRAC_RULE_EM: f64 = 0.04;
const FRAC_PAD_EM: f64 = 0.12;
const SPACE_EM: f64 = 0.28;

/// A laid-out math expression in y-up coordinates.
#[derive(Clone, Debug, PartialEq)]
pub struct MathLayout {
    /// Positioned vector paths that make up the expression.
    pub elements: Vec<MathElement>,
    /// Total advance width in pixels.
    pub width: f64,
    /// Distance from baseline to top in pixels.
    pub ascent: f64,
    /// Distance from baseline to bottom in pixels.
    pub descent: f64,
    /// Warnings produced while parsing or laying out supported fallback forms.
    pub warnings: Vec<MathTextWarning>,
}

impl MathLayout {
    /// Total vertical extent in pixels.
    #[must_use]
    pub fn height(&self) -> f64 {
        self.ascent + self.descent
    }

    /// Returns `true` when no visible paths were produced.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
    }
}

/// One positioned geometry element in a [`MathLayout`].
#[derive(Clone, Debug, PartialEq)]
pub enum MathElement {
    /// Glyph outline geometry produced by [`FontSource::text_to_path`].
    Glyph {
        /// Original character or fallback string represented by this path.
        text: String,
        /// Glyph outline path in final math-layout coordinates.
        path: Path,
    },
    /// A filled rule, currently used for fraction bars.
    Rule {
        /// Rectangle path in final math-layout coordinates.
        path: Path,
    },
}

/// A non-fatal parser/layout warning.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MathTextWarning {
    /// Byte range in the source math string, when known.
    pub range: Option<std::ops::Range<usize>>,
    /// Warning reason.
    pub reason: MathTextWarningReason,
    /// Source fragment that triggered the warning.
    pub source: String,
}

/// Why mathtext fell back or recovered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathTextWarningReason {
    /// A TeX command is not in the supported subset.
    UnsupportedCommand,
    /// A group opener had no matching closing brace.
    UnclosedGroup,
    /// `^` or `_` appeared without a following atom/group.
    MissingScript,
    /// A closing brace appeared without a matching opener.
    UnmatchedCloseBrace,
    /// `\frac` was missing one of its required groups.
    MissingFractionArgument,
}

/// Error returned when an API receives a non-math span.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MathTextError {
    /// The supplied [`TextSpan`] was plain text.
    PlainTextSpan,
    /// The supplied [`TextSpan`] was an explicit raw-TeX passthrough span.
    RawTexSpan,
}

/// Layout a math expression at `font_size_px`.
///
/// `source` should be the inner content of a math span, without delimiters. The
/// result uses a y-up coordinate system with the expression baseline at `y = 0`.
#[must_use]
pub fn layout_math(source: &str, font: &FontSource, font_size_px: f64) -> MathLayout {
    let mut parser = Parser::new(source);
    let ast = parser.parse_row(None);
    let mut layout = layout_nodes(&ast.nodes, font, font_size_px);
    layout.warnings.extend(parser.warnings);
    layout
}

/// Layout a parsed math [`TextSpan`].
///
/// The span must be [`TextSpanKind::Math`]. Plain and raw-TeX spans are rejected
/// so higher layers can route them through their normal text or passthrough
/// paths.
pub fn layout_span(
    span: &TextSpan,
    font: &FontSource,
    font_size_px: f64,
) -> Result<MathLayout, MathTextError> {
    match span.kind() {
        TextSpanKind::Math(_) => Ok(layout_math(span.content(), font, font_size_px)),
        TextSpanKind::Plain => Err(MathTextError::PlainTextSpan),
        TextSpanKind::RawTex(_) => Err(MathTextError::RawTexSpan),
    }
}

#[derive(Clone, Debug, PartialEq)]
struct Row {
    nodes: Vec<Node>,
}

#[derive(Clone, Debug, PartialEq)]
enum Node {
    Text(String),
    Space,
    Fraction {
        numerator: Row,
        denominator: Row,
    },
    Script {
        base: Box<Node>,
        sup: Option<Row>,
        sub: Option<Row>,
    },
}

struct Parser<'a> {
    source: &'a str,
    pos: usize,
    warnings: Vec<MathTextWarning>,
}

impl<'a> Parser<'a> {
    fn new(source: &'a str) -> Self {
        Self {
            source,
            pos: 0,
            warnings: Vec::new(),
        }
    }

    fn parse_row(&mut self, terminator: Option<char>) -> Row {
        let mut nodes = Vec::new();

        while self.pos < self.source.len() {
            if let Some(term) = terminator
                && self.peek_char() == Some(term)
            {
                self.pos += term.len_utf8();
                return Row { nodes };
            }

            match self.peek_char() {
                Some('}') => {
                    self.warn_here(MathTextWarningReason::UnmatchedCloseBrace, "}");
                    self.pos += 1;
                }
                Some('{') => {
                    self.pos += 1;
                    let group = self.parse_group_from_open();
                    nodes.extend(group.nodes);
                }
                Some('^') | Some('_') => {
                    let source = self.peek_slice().to_owned();
                    self.warn_here(MathTextWarningReason::MissingScript, &source);
                    self.pos += 1;
                }
                Some('\\') => nodes.push(self.parse_command()),
                Some(ch) if ch.is_whitespace() => {
                    self.pos += ch.len_utf8();
                    if !matches!(nodes.last(), Some(Node::Space)) {
                        nodes.push(Node::Space);
                    }
                }
                Some(_) => {
                    let base = self.parse_atom();
                    nodes.push(self.parse_scripts(base));
                }
                None => break,
            }
        }

        if terminator.is_some() {
            self.warnings.push(MathTextWarning {
                range: Some(self.source.len()..self.source.len()),
                reason: MathTextWarningReason::UnclosedGroup,
                source: String::new(),
            });
        }

        Row { nodes }
    }

    fn parse_atom(&mut self) -> Node {
        match self.peek_char() {
            Some('{') => {
                self.pos += 1;
                let group = self.parse_group_from_open();
                Node::Text(flatten_row_text(&group))
            }
            Some('\\') => self.parse_command(),
            Some(ch) if ch.is_whitespace() => {
                self.pos += ch.len_utf8();
                Node::Space
            }
            Some(ch) => {
                self.pos += ch.len_utf8();
                Node::Text(ch.to_string())
            }
            None => Node::Text(String::new()),
        }
    }

    fn parse_scripts(&mut self, base: Node) -> Node {
        let mut sup = None;
        let mut sub = None;

        loop {
            match self.peek_char() {
                Some('^') => {
                    self.pos += 1;
                    sup = self.parse_script_argument();
                }
                Some('_') => {
                    self.pos += 1;
                    sub = self.parse_script_argument();
                }
                _ => break,
            }
        }

        if sup.is_some() || sub.is_some() {
            Node::Script {
                base: Box::new(base),
                sup,
                sub,
            }
        } else {
            base
        }
    }

    fn parse_script_argument(&mut self) -> Option<Row> {
        if self.pos >= self.source.len() {
            self.warnings.push(MathTextWarning {
                range: Some(self.pos..self.pos),
                reason: MathTextWarningReason::MissingScript,
                source: String::new(),
            });
            return None;
        }

        if self.peek_char() == Some('{') {
            self.pos += 1;
            Some(self.parse_group_from_open())
        } else {
            let atom = self.parse_atom();
            Some(Row { nodes: vec![atom] })
        }
    }

    fn parse_group_from_open(&mut self) -> Row {
        self.parse_row(Some('}'))
    }

    fn parse_command(&mut self) -> Node {
        let start = self.pos;
        self.pos += 1; // leading slash
        let name_start = self.pos;
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_alphabetic() {
                self.pos += ch.len_utf8();
            } else {
                break;
            }
        }
        let name = &self.source[name_start..self.pos];
        if name.is_empty() {
            if let Some(ch) = self.peek_char() {
                self.pos += ch.len_utf8();
                return Node::Text(ch.to_string());
            }
            return Node::Text("\\".to_owned());
        }

        if name == "frac" {
            return self.parse_fraction(start);
        }

        if let Some(symbol) = command_symbol(name) {
            return self.parse_scripts(Node::Text(symbol.to_owned()));
        }

        let source = self.source[start..self.pos].to_owned();
        self.warnings.push(MathTextWarning {
            range: Some(start..self.pos),
            reason: MathTextWarningReason::UnsupportedCommand,
            source: source.clone(),
        });
        Node::Text(source)
    }

    fn parse_fraction(&mut self, start: usize) -> Node {
        let Some(numerator) = self.parse_required_group(start, "\\frac") else {
            return Node::Text("\\frac".to_owned());
        };
        let Some(denominator) = self.parse_required_group(start, "\\frac") else {
            return Node::Text("\\frac".to_owned());
        };
        self.parse_scripts(Node::Fraction {
            numerator,
            denominator,
        })
    }

    fn parse_required_group(&mut self, command_start: usize, command: &str) -> Option<Row> {
        if self.peek_char() == Some('{') {
            self.pos += 1;
            Some(self.parse_group_from_open())
        } else {
            self.warnings.push(MathTextWarning {
                range: Some(command_start..self.pos),
                reason: MathTextWarningReason::MissingFractionArgument,
                source: command.to_owned(),
            });
            None
        }
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.pos..].chars().next()
    }

    fn peek_slice(&self) -> &str {
        self.peek_char()
            .map(|ch| &self.source[self.pos..self.pos + ch.len_utf8()])
            .unwrap_or("")
    }

    fn warn_here(&mut self, reason: MathTextWarningReason, source: &str) {
        self.warnings.push(MathTextWarning {
            range: Some(self.pos..self.pos + source.len()),
            reason,
            source: source.to_owned(),
        });
    }
}

#[derive(Clone, Debug, PartialEq)]
struct LayoutBox {
    elements: Vec<MathElement>,
    width: f64,
    ascent: f64,
    descent: f64,
}

fn layout_nodes(nodes: &[Node], font: &FontSource, font_size_px: f64) -> MathLayout {
    let box_ = layout_row(nodes, font, font_size_px);
    MathLayout {
        elements: box_.elements,
        width: box_.width,
        ascent: box_.ascent,
        descent: box_.descent,
        warnings: Vec::new(),
    }
}

fn layout_row(nodes: &[Node], font: &FontSource, font_size_px: f64) -> LayoutBox {
    let mut elements = Vec::new();
    let mut x = 0.0;
    let mut ascent: f64 = 0.0;
    let mut descent: f64 = 0.0;

    for node in nodes {
        let child = layout_node(node, font, font_size_px);
        ascent = ascent.max(child.ascent);
        descent = descent.max(child.descent);
        elements.extend(shift_elements(child.elements, x, 0.0));
        x += child.width;
    }

    LayoutBox {
        elements,
        width: x,
        ascent,
        descent,
    }
}

fn layout_node(node: &Node, font: &FontSource, font_size_px: f64) -> LayoutBox {
    match node {
        Node::Text(text) => layout_text(text, font, font_size_px),
        Node::Space => LayoutBox {
            elements: Vec::new(),
            width: font_size_px * SPACE_EM,
            ascent: 0.0,
            descent: 0.0,
        },
        Node::Fraction {
            numerator,
            denominator,
        } => layout_fraction(numerator, denominator, font, font_size_px),
        Node::Script { base, sup, sub } => {
            layout_script(base, sup.as_ref(), sub.as_ref(), font, font_size_px)
        }
    }
}

fn layout_text(text: &str, font: &FontSource, font_size_px: f64) -> LayoutBox {
    if text.is_empty() {
        return LayoutBox {
            elements: Vec::new(),
            width: 0.0,
            ascent: 0.0,
            descent: 0.0,
        };
    }

    let extent = font.measure(text, font_size_px);
    let path = font.text_to_path(text, font_size_px, [0.0, 0.0]);
    LayoutBox {
        elements: vec![MathElement::Glyph {
            text: text.to_owned(),
            path,
        }],
        width: extent.width,
        ascent: extent.ascent,
        descent: extent.descent,
    }
}

fn layout_fraction(
    numerator: &Row,
    denominator: &Row,
    font: &FontSource,
    font_size_px: f64,
) -> LayoutBox {
    let script_size = font_size_px * SCRIPT_SCALE;
    let num = layout_row(&numerator.nodes, font, script_size);
    let den = layout_row(&denominator.nodes, font, script_size);
    let pad = font_size_px * FRAC_PAD_EM;
    let gap = font_size_px * FRAC_GAP_EM;
    let rule_thickness = (font_size_px * FRAC_RULE_EM).max(1.0);
    let width = num.width.max(den.width) + 2.0 * pad;

    let num_x = (width - num.width) / 2.0;
    let den_x = (width - den.width) / 2.0;
    let num_baseline = gap + rule_thickness / 2.0 + num.descent;
    let den_baseline = -gap - rule_thickness / 2.0 - den.ascent;

    let mut elements = Vec::new();
    elements.extend(shift_elements(num.elements, num_x, num_baseline));
    elements.push(MathElement::Rule {
        path: rect_path(0.0, -rule_thickness / 2.0, width, rule_thickness),
    });
    elements.extend(shift_elements(den.elements, den_x, den_baseline));

    LayoutBox {
        elements,
        width,
        ascent: num_baseline + num.ascent,
        descent: -den_baseline + den.descent,
    }
}

fn layout_script(
    base: &Node,
    sup: Option<&Row>,
    sub: Option<&Row>,
    font: &FontSource,
    font_size_px: f64,
) -> LayoutBox {
    let base_box = layout_node(base, font, font_size_px);
    let script_size = font_size_px * SCRIPT_SCALE;
    let script_gap = font_size_px * SCRIPT_GAP_EM;
    let mut elements = base_box.elements;
    let mut width = base_box.width;
    let mut ascent = base_box.ascent;
    let mut descent = base_box.descent;
    let mut script_width: f64 = 0.0;

    if let Some(sup) = sup {
        let sup_box = layout_row(&sup.nodes, font, script_size);
        let y = base_box.ascent * 0.68 + script_gap;
        ascent = ascent.max(y + sup_box.ascent);
        elements.extend(shift_elements(sup_box.elements, base_box.width, y));
        script_width = script_width.max(sup_box.width);
    }

    if let Some(sub) = sub {
        let sub_box = layout_row(&sub.nodes, font, script_size);
        let y = -base_box.descent - script_gap - sub_box.ascent * 0.35;
        descent = descent.max(-y + sub_box.descent);
        elements.extend(shift_elements(sub_box.elements, base_box.width, y));
        script_width = script_width.max(sub_box.width);
    }

    width += script_width;

    LayoutBox {
        elements,
        width,
        ascent,
        descent,
    }
}

fn shift_elements(elements: Vec<MathElement>, dx: f64, dy: f64) -> Vec<MathElement> {
    if dx == 0.0 && dy == 0.0 {
        return elements;
    }
    let transform = Affine2D::from_translation(dx, dy);
    elements
        .into_iter()
        .map(|element| match element {
            MathElement::Glyph { text, path } => MathElement::Glyph {
                text,
                path: path.transformed(&transform),
            },
            MathElement::Rule { path } => MathElement::Rule {
                path: path.transformed(&transform),
            },
        })
        .collect()
}

fn rect_path(x: f64, y: f64, width: f64, height: f64) -> Path {
    Path::unit_rectangle()
        .transformed(&Affine2D::from_scale(width, height).then(&Affine2D::from_translation(x, y)))
}

fn flatten_row_text(row: &Row) -> String {
    let mut out = String::new();
    for node in &row.nodes {
        match node {
            Node::Text(text) => out.push_str(text),
            Node::Space => out.push(' '),
            Node::Fraction { .. } => out.push_str("\\frac"),
            Node::Script { base, .. } => out.push_str(&flatten_node_text(base)),
        }
    }
    out
}

fn flatten_node_text(node: &Node) -> String {
    match node {
        Node::Text(text) => text.clone(),
        Node::Space => " ".to_owned(),
        Node::Fraction { .. } => "\\frac".to_owned(),
        Node::Script { base, .. } => flatten_node_text(base),
    }
}

fn command_symbol(name: &str) -> Option<&'static str> {
    match name {
        "alpha" => Some("α"),
        "beta" => Some("β"),
        "gamma" => Some("γ"),
        "delta" => Some("δ"),
        "epsilon" => Some("ε"),
        "theta" => Some("θ"),
        "lambda" => Some("λ"),
        "mu" => Some("μ"),
        "pi" => Some("π"),
        "sigma" => Some("σ"),
        "phi" => Some("φ"),
        "omega" => Some("ω"),
        "Gamma" => Some("Γ"),
        "Delta" => Some("Δ"),
        "Theta" => Some("Θ"),
        "Lambda" => Some("Λ"),
        "Pi" => Some("Π"),
        "Sigma" => Some("Σ"),
        "Phi" => Some("Φ"),
        "Omega" => Some("Ω"),
        "times" => Some("×"),
        "pm" => Some("±"),
        "leq" => Some("≤"),
        "geq" => Some("≥"),
        "neq" => Some("≠"),
        "infty" => Some("∞"),
        "partial" => Some("∂"),
        "sum" => Some("∑"),
        "int" => Some("∫"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rizzma_text::{MathMode, TextRun};

    fn font() -> FontSource {
        FontSource::dejavu_sans()
    }

    #[test]
    fn simple_symbols_produce_paths_and_metrics() {
        let layout = layout_math("x+y", &font(), 20.0);
        assert_eq!(layout.elements.len(), 3);
        assert!(layout.width > 0.0);
        assert!(layout.ascent > 0.0);
        assert!(layout.descent >= 0.0);
        assert!(layout.warnings.is_empty());
    }

    #[test]
    fn named_symbols_map_to_unicode_glyphs() {
        let layout = layout_math("\\alpha+\\beta", &font(), 20.0);
        let texts: Vec<_> = layout
            .elements
            .iter()
            .filter_map(|element| match element {
                MathElement::Glyph { text, .. } => Some(text.as_str()),
                MathElement::Rule { .. } => None,
            })
            .collect();
        assert_eq!(texts, ["α", "+", "β"]);
        assert!(layout.warnings.is_empty());
    }

    #[test]
    fn superscript_increases_ascent_and_width() {
        let plain = layout_math("x", &font(), 20.0);
        let scripted = layout_math("x^2", &font(), 20.0);
        assert!(scripted.width > plain.width);
        assert!(scripted.ascent > plain.ascent);
        assert!(scripted.descent >= plain.descent);
    }

    #[test]
    fn subscript_increases_descent_and_width() {
        let plain = layout_math("x", &font(), 20.0);
        let scripted = layout_math("x_i", &font(), 20.0);
        assert!(scripted.width > plain.width);
        assert!(scripted.descent > plain.descent);
    }

    #[test]
    fn grouped_script_keeps_group_together() {
        let scripted = layout_math("x^{10}", &font(), 20.0);
        let glyph_count = scripted
            .elements
            .iter()
            .filter(|element| matches!(element, MathElement::Glyph { .. }))
            .count();
        assert_eq!(glyph_count, 3);
        assert!(scripted.width > layout_math("x^1", &font(), 20.0).width);
    }

    #[test]
    fn fraction_emits_rule_and_centers_parts() {
        let layout = layout_math("\\frac{a}{b}", &font(), 24.0);
        assert_eq!(
            layout
                .elements
                .iter()
                .filter(|element| matches!(element, MathElement::Rule { .. }))
                .count(),
            1
        );
        assert!(layout.ascent > 0.0);
        assert!(layout.descent > 0.0);
        assert!(layout.height() > 24.0);
    }

    #[test]
    fn unsupported_command_warns_and_preserves_source() {
        let layout = layout_math("\\unknown+x", &font(), 20.0);
        assert_eq!(layout.warnings.len(), 1);
        assert_eq!(
            layout.warnings[0].reason,
            MathTextWarningReason::UnsupportedCommand
        );
        assert_eq!(layout.warnings[0].source, "\\unknown");
        assert!(layout.width > 0.0);
    }

    #[test]
    fn missing_fraction_argument_warns() {
        let layout = layout_math("\\frac{x", &font(), 20.0);
        assert!(
            layout
                .warnings
                .iter()
                .any(|w| w.reason == MathTextWarningReason::MissingFractionArgument)
        );
    }

    #[test]
    fn layout_span_accepts_math_spans_only() {
        let run = TextRun::parse("plain $x^2$");
        let plain = &run.spans()[0];
        let math = &run.spans()[1];
        assert_eq!(math.kind(), TextSpanKind::Math(MathMode::Inline));

        assert_eq!(
            layout_span(plain, &font(), 20.0),
            Err(MathTextError::PlainTextSpan)
        );
        assert!(layout_span(math, &font(), 20.0).is_ok());
    }

    #[test]
    fn empty_expression_is_empty_layout() {
        let layout = layout_math("", &font(), 20.0);
        assert!(layout.is_empty());
        assert_eq!(layout.width, 0.0);
        assert_eq!(layout.height(), 0.0);
    }
}
