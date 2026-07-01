//! Mathtext for rizzma.
//!
//! A small, deterministic TeX-subset box-and-glue engine for math spans. It
//! parses inline math content into a compact layout tree, positions glyph
//! outlines from [`crate::text::FontSource`], and emits backend-independent
//! [`crate::core::Path`] geometry. Integration into figure text artists is a
//! later step; this crate owns only parsing and layout.
//!
//! Supported in this first pass: ordinary symbols, whitespace glue, `{...}`
//! groups, superscripts/subscripts, `\frac{...}{...}`, `\binom{...}{...}`,
//! `\sqrt{...}` and `\sqrt[n]{...}`, `\overline{...}`, `\underline{...}`,
//! `\text{...}`, `\operatorname{...}`, common named operators,
//! `\mathbb{...}`/`\mathcal{...}`/`\mathfrak{...}`,
//! `\substack{...}`,
//! `\begin{matrix}`/`pmatrix`/`bmatrix`/`cases` environments,
//! `\left...\right` delimiters, large operators, and a table of common named
//! symbols and accents. Unsupported commands are preserved as literal fallback
//! text and reported as structured warnings. The
//! [`richtext`] module combines plain text spans and math spans into reusable
//! label geometry for axes, titles, and other text artists.
//!
//! This is intentionally a scoped approximation: it uses the embedded DejaVu
//! Sans face for wasm-clean, deterministic glyph geometry, so it does not yet
//! provide math italic, dedicated math-font metrics, true extensible delimiter
//! assembly, or publication-grade TeX spacing.
//!
//! Build-order home: Phase 10 of `design/04-implementation-plan.md`.

pub mod richtext;

pub use richtext::{RichText, layout_rich_text};

use crate::core::{Affine2D, Path};
use crate::text::{FontSource, TextSpan, TextSpanKind};

const SCRIPT_SCALE: f64 = 0.7;
const SCRIPT_GAP_EM: f64 = 0.08;
const FRAC_GAP_EM: f64 = 0.18;
const FRAC_RULE_EM: f64 = 0.04;
const FRAC_PAD_EM: f64 = 0.12;
const LARGE_OPERATOR_SCALE: f64 = 1.35;
const RADICAL_GAP_EM: f64 = 0.10;
const RADICAL_PAD_EM: f64 = 0.08;
const RADICAL_RULE_EM: f64 = 0.04;
const LINE_DECORATION_GAP_EM: f64 = 0.08;
const LINE_DECORATION_RULE_EM: f64 = 0.04;
const MATRIX_COL_GAP_EM: f64 = 0.75;
const MATRIX_ROW_GAP_EM: f64 = 0.28;
const SPACE_EM: f64 = 0.28;
const THIN_SPACE_EM: f64 = 3.0 / 18.0;
const MEDIUM_SPACE_EM: f64 = 4.0 / 18.0;
const THICK_SPACE_EM: f64 = 5.0 / 18.0;
const QUAD_SPACE_EM: f64 = 1.0;

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

    /// Returns a copy of this layout translated by `(dx, dy)` pixels.
    ///
    /// This is the preferred way for higher-level text layout to place math
    /// runs on a baseline without matching on individual [`MathElement`]
    /// variants.
    #[must_use]
    pub fn translated(&self, dx: f64, dy: f64) -> Self {
        Self {
            elements: self
                .elements
                .iter()
                .map(|element| element.translated(dx, dy))
                .collect(),
            width: self.width,
            ascent: self.ascent,
            descent: self.descent,
            warnings: self.warnings.clone(),
        }
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
    /// A filled rule used by composite constructs such as radical overbars.
    Rule {
        /// Rectangle path in final math-layout coordinates.
        path: Path,
    },
    /// Combined geometry for a `\frac{...}{...}` expression.
    Fraction {
        /// Numerator, denominator, and rule geometry as one path in final
        /// math-layout coordinates.
        path: Path,
    },
    /// Accent mark geometry positioned above a base expression.
    Accent {
        /// Accent kind.
        kind: AccentKind,
        /// Accent mark path in final math-layout coordinates.
        path: Path,
    },
    /// Delimiter geometry for a `\left...\right` group.
    Delimiter {
        /// Delimiter kind.
        kind: DelimiterKind,
        /// Delimiter path in final math-layout coordinates.
        path: Path,
    },
    /// Large operator geometry, such as `\sum` or `\int`.
    LargeOperator {
        /// Operator kind.
        kind: LargeOperatorKind,
        /// Operator path in final math-layout coordinates.
        path: Path,
    },
    /// Radical sign geometry for `\sqrt{...}`.
    Radical {
        /// Radical sign path in final math-layout coordinates.
        path: Path,
    },
}

impl MathElement {
    /// Returns this element's geometry path.
    ///
    /// Consumers should use this accessor instead of exhaustively matching on
    /// [`MathElement`] variants when they only need geometry.
    #[must_use]
    pub fn path(&self) -> &Path {
        match self {
            MathElement::Glyph { path, .. }
            | MathElement::Rule { path }
            | MathElement::Fraction { path }
            | MathElement::Accent { path, .. }
            | MathElement::Delimiter { path, .. }
            | MathElement::LargeOperator { path, .. }
            | MathElement::Radical { path } => path,
        }
    }

    /// Returns a copy of this element translated by `(dx, dy)` pixels.
    #[must_use]
    pub fn translated(&self, dx: f64, dy: f64) -> Self {
        if dx == 0.0 && dy == 0.0 {
            return self.clone();
        }
        let transform = Affine2D::from_translation(dx, dy);
        self.transformed(&transform)
    }

    fn transformed(&self, transform: &Affine2D) -> Self {
        match self {
            MathElement::Glyph { text, path } => MathElement::Glyph {
                text: text.clone(),
                path: path.transformed(transform),
            },
            MathElement::Rule { path } => MathElement::Rule {
                path: path.transformed(transform),
            },
            MathElement::Fraction { path } => MathElement::Fraction {
                path: path.transformed(transform),
            },
            MathElement::Accent { kind, path } => MathElement::Accent {
                kind: *kind,
                path: path.transformed(transform),
            },
            MathElement::Delimiter { kind, path } => MathElement::Delimiter {
                kind: *kind,
                path: path.transformed(transform),
            },
            MathElement::LargeOperator { kind, path } => MathElement::LargeOperator {
                kind: *kind,
                path: path.transformed(transform),
            },
            MathElement::Radical { path } => MathElement::Radical {
                path: path.transformed(transform),
            },
        }
    }
}

/// Supported accent commands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccentKind {
    /// `\hat{x}`.
    Hat,
    /// `\bar{x}`.
    Bar,
    /// `\vec{x}`.
    Vec,
    /// `\tilde{x}`.
    Tilde,
    /// `\dot{x}`.
    Dot,
    /// `\ddot{x}`.
    Ddot,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LineDecorationKind {
    Overline,
    Underline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MathStyle {
    Blackboard,
    Calligraphic,
    Fraktur,
}

/// Supported delimiter commands for `\left...\right`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DelimiterKind {
    /// `(`.
    Paren,
    /// `[`.
    Bracket,
    /// `{`.
    Brace,
    /// `|`.
    Bar,
    /// `\|`.
    DoubleBar,
    /// `\langle` or `\rangle`.
    Angle,
    /// `.` invisible delimiter.
    None,
}

/// Supported large operators.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LargeOperatorKind {
    /// `\sum`.
    Sum,
    /// `\prod`.
    Prod,
    /// `\int`.
    Integral,
    /// `\oint`.
    ContourIntegral,
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
    /// A command was missing a required group argument.
    MissingCommandArgument,
    /// `\left` or `\right` was missing its required delimiter token.
    MissingDelimiter,
    /// A `\left` group reached end-of-input before `\right`.
    MissingRightDelimiter,
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
    Kern(f64),
    Fraction {
        numerator: Row,
        denominator: Row,
    },
    Binomial {
        upper: Row,
        lower: Row,
    },
    Radical {
        index: Option<Row>,
        body: Row,
    },
    Matrix {
        rows: Vec<Vec<Row>>,
        left: DelimiterKind,
        right: DelimiterKind,
    },
    Substack {
        rows: Vec<Row>,
    },
    LineDecoration {
        kind: LineDecorationKind,
        body: Row,
    },
    Delimited {
        left: DelimiterKind,
        body: Row,
        right: DelimiterKind,
    },
    Accent {
        kind: AccentKind,
        body: Row,
    },
    Styled {
        style: MathStyle,
        body: Row,
    },
    LargeOperator {
        kind: LargeOperatorKind,
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
                if let Some(kern) = single_char_spacing_command(ch) {
                    return Node::Kern(kern);
                }
                return Node::Text(ch.to_string());
            }
            return Node::Text("\\".to_owned());
        }

        if name == "frac" {
            return self.parse_fraction(start);
        }

        if name == "binom" {
            return self.parse_binomial(start);
        }

        if name == "sqrt" {
            return self.parse_radical(start);
        }

        if name == "text" {
            return self.parse_text_command(start);
        }

        if name == "operatorname" {
            return self.parse_operatorname(start);
        }

        if name == "substack" {
            return self.parse_substack(start);
        }

        if let Some(style) = math_style_command(name) {
            return self.parse_math_style_command(start, name, style);
        }

        if name == "begin" {
            return self.parse_environment(start);
        }

        if name == "overline" {
            return self.parse_line_decoration(start, name, LineDecorationKind::Overline);
        }

        if name == "underline" {
            return self.parse_line_decoration(start, name, LineDecorationKind::Underline);
        }

        if name == "left" {
            return self.parse_left_right(start);
        }

        if let Some(kern) = named_spacing_command(name) {
            self.consume_ascii_whitespace();
            return Node::Kern(kern);
        }

        if let Some(kind) = large_operator_kind(name) {
            return self.parse_scripts(Node::LargeOperator { kind });
        }

        if let Some(operator) = named_operator(name) {
            return self.parse_scripts(Node::Text(operator.to_owned()));
        }

        if let Some(kind) = accent_kind(name) {
            return self.parse_accent(start, name, kind);
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
        let Some(numerator) = self.parse_required_group(
            start,
            "frac",
            MathTextWarningReason::MissingFractionArgument,
        ) else {
            return Node::Text("\\frac".to_owned());
        };
        let Some(denominator) = self.parse_required_group(
            start,
            "frac",
            MathTextWarningReason::MissingFractionArgument,
        ) else {
            return Node::Text("\\frac".to_owned());
        };
        self.parse_scripts(Node::Fraction {
            numerator,
            denominator,
        })
    }

    fn parse_binomial(&mut self, start: usize) -> Node {
        let Some(upper) = self.parse_required_group(
            start,
            "binom",
            MathTextWarningReason::MissingCommandArgument,
        ) else {
            return Node::Text("\\binom".to_owned());
        };
        let Some(lower) = self.parse_required_group(
            start,
            "binom",
            MathTextWarningReason::MissingCommandArgument,
        ) else {
            return Node::Text("\\binom".to_owned());
        };
        self.parse_scripts(Node::Binomial { upper, lower })
    }

    fn parse_radical(&mut self, start: usize) -> Node {
        let index = self.parse_optional_bracket_group();
        let Some(body) =
            self.parse_required_group(start, "sqrt", MathTextWarningReason::MissingCommandArgument)
        else {
            return Node::Text("\\sqrt".to_owned());
        };
        self.parse_scripts(Node::Radical { index, body })
    }

    fn parse_optional_bracket_group(&mut self) -> Option<Row> {
        if self.peek_char() != Some('[') {
            return None;
        }

        self.pos += 1;
        Some(self.parse_row(Some(']')))
    }

    fn parse_text_command(&mut self, start: usize) -> Node {
        let Some(text) = self.parse_required_raw_group(
            start,
            "text",
            MathTextWarningReason::MissingCommandArgument,
        ) else {
            return Node::Text("\\text".to_owned());
        };
        self.parse_scripts(Node::Text(text))
    }

    fn parse_operatorname(&mut self, start: usize) -> Node {
        let Some(text) = self.parse_required_raw_group(
            start,
            "operatorname",
            MathTextWarningReason::MissingCommandArgument,
        ) else {
            return Node::Text("\\operatorname".to_owned());
        };
        self.parse_scripts(Node::Text(text))
    }

    fn parse_substack(&mut self, start: usize) -> Node {
        if self.peek_char() != Some('{') {
            self.warnings.push(MathTextWarning {
                range: Some(start..self.pos),
                reason: MathTextWarningReason::MissingCommandArgument,
                source: "\\substack".to_owned(),
            });
            return Node::Text("\\substack".to_owned());
        }

        self.pos += 1;
        let rows = self.parse_substack_rows();
        self.parse_scripts(Node::Substack { rows })
    }

    fn parse_math_style_command(&mut self, start: usize, command: &str, style: MathStyle) -> Node {
        let Some(body) = self.parse_required_group(
            start,
            command,
            MathTextWarningReason::MissingCommandArgument,
        ) else {
            return Node::Text(format!("\\{command}"));
        };
        self.parse_scripts(Node::Styled { style, body })
    }

    fn parse_line_decoration(
        &mut self,
        start: usize,
        command: &str,
        kind: LineDecorationKind,
    ) -> Node {
        let Some(body) = self.parse_required_group(
            start,
            command,
            MathTextWarningReason::MissingCommandArgument,
        ) else {
            return Node::Text(format!("\\{command}"));
        };
        self.parse_scripts(Node::LineDecoration { kind, body })
    }

    fn parse_left_right(&mut self, start: usize) -> Node {
        let Some(left) = self.parse_delimiter_token(start, "left") else {
            return Node::Text("\\left".to_owned());
        };
        let (body, right) = self.parse_row_until_right();
        let right = right.unwrap_or_else(|| {
            self.warnings.push(MathTextWarning {
                range: Some(self.source.len()..self.source.len()),
                reason: MathTextWarningReason::MissingRightDelimiter,
                source: "\\left".to_owned(),
            });
            DelimiterKind::None
        });
        self.parse_scripts(Node::Delimited { left, body, right })
    }

    fn parse_accent(&mut self, start: usize, command: &str, kind: AccentKind) -> Node {
        let Some(body) = self.parse_required_group(
            start,
            command,
            MathTextWarningReason::MissingCommandArgument,
        ) else {
            return Node::Text(format!("\\{command}"));
        };
        self.parse_scripts(Node::Accent { kind, body })
    }

    fn parse_required_group(
        &mut self,
        command_start: usize,
        command: &str,
        reason: MathTextWarningReason,
    ) -> Option<Row> {
        if self.peek_char() == Some('{') {
            self.pos += 1;
            Some(self.parse_group_from_open())
        } else {
            self.warnings.push(MathTextWarning {
                range: Some(command_start..self.pos),
                reason,
                source: format!("\\{command}"),
            });
            None
        }
    }

    fn parse_required_raw_group(
        &mut self,
        command_start: usize,
        command: &str,
        reason: MathTextWarningReason,
    ) -> Option<String> {
        if self.peek_char() != Some('{') {
            self.warnings.push(MathTextWarning {
                range: Some(command_start..self.pos),
                reason,
                source: format!("\\{command}"),
            });
            return None;
        }

        self.pos += 1;
        let mut depth = 1;
        let mut text = String::new();

        while self.pos < self.source.len() {
            let Some(ch) = self.peek_char() else {
                break;
            };

            match ch {
                '\\' => {
                    self.pos += 1;
                    if let Some(escaped) = self.peek_char() {
                        self.pos += escaped.len_utf8();
                        text.push(escaped);
                    } else {
                        text.push('\\');
                    }
                }
                '{' => {
                    self.pos += 1;
                    depth += 1;
                    text.push(ch);
                }
                '}' => {
                    self.pos += 1;
                    depth -= 1;
                    if depth == 0 {
                        return Some(text);
                    }
                    text.push(ch);
                }
                _ => {
                    self.pos += ch.len_utf8();
                    text.push(ch);
                }
            }
        }

        self.warnings.push(MathTextWarning {
            range: Some(self.source.len()..self.source.len()),
            reason: MathTextWarningReason::UnclosedGroup,
            source: String::new(),
        });
        Some(text)
    }

    fn parse_environment(&mut self, start: usize) -> Node {
        let Some(environment) = self.parse_required_raw_group(
            start,
            "begin",
            MathTextWarningReason::MissingCommandArgument,
        ) else {
            return Node::Text("\\begin".to_owned());
        };

        let (left, right) = match environment.as_str() {
            "matrix" => (DelimiterKind::None, DelimiterKind::None),
            "pmatrix" => (DelimiterKind::Paren, DelimiterKind::Paren),
            "bmatrix" => (DelimiterKind::Bracket, DelimiterKind::Bracket),
            "cases" => (DelimiterKind::Brace, DelimiterKind::None),
            _ => {
                let source = format!("\\begin{{{environment}}}");
                self.warnings.push(MathTextWarning {
                    range: Some(start..self.pos),
                    reason: MathTextWarningReason::UnsupportedCommand,
                    source: source.clone(),
                });
                return Node::Text(source);
            }
        };

        let rows = self.parse_matrix_rows(&environment);
        self.parse_scripts(Node::Matrix { rows, left, right })
    }

    fn parse_substack_rows(&mut self) -> Vec<Row> {
        let mut rows = Vec::new();
        let mut nodes = Vec::new();

        while self.pos < self.source.len() {
            if self.peek_char() == Some('}') {
                self.pos += 1;
                rows.push(Row { nodes });
                return rows;
            }

            if self.source[self.pos..].starts_with("\\\\") {
                self.pos += 2;
                rows.push(Row { nodes });
                nodes = Vec::new();
                continue;
            }

            match self.peek_char() {
                Some('}') => unreachable!("handled above"),
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

        self.warnings.push(MathTextWarning {
            range: Some(self.source.len()..self.source.len()),
            reason: MathTextWarningReason::UnclosedGroup,
            source: String::new(),
        });
        rows.push(Row { nodes });
        rows
    }

    fn parse_matrix_rows(&mut self, environment: &str) -> Vec<Vec<Row>> {
        let mut rows = Vec::new();
        let mut cells = Vec::new();
        let mut nodes = Vec::new();

        while self.pos < self.source.len() {
            if self.consume_environment_end(environment) {
                cells.push(Row { nodes });
                rows.push(cells);
                return rows;
            }

            if self.source[self.pos..].starts_with("\\\\") {
                self.pos += 2;
                cells.push(Row { nodes });
                rows.push(cells);
                cells = Vec::new();
                nodes = Vec::new();
                continue;
            }

            match self.peek_char() {
                Some('&') => {
                    self.pos += 1;
                    cells.push(Row { nodes });
                    nodes = Vec::new();
                }
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

        self.warnings.push(MathTextWarning {
            range: Some(self.source.len()..self.source.len()),
            reason: MathTextWarningReason::UnclosedGroup,
            source: format!("\\begin{{{environment}}}"),
        });
        cells.push(Row { nodes });
        rows.push(cells);
        rows
    }

    fn parse_row_until_right(&mut self) -> (Row, Option<DelimiterKind>) {
        let mut nodes = Vec::new();

        while self.pos < self.source.len() {
            if self.starts_command("right") {
                let start = self.pos;
                self.pos += "\\right".len();
                let right = self.parse_delimiter_token(start, "right");
                return (Row { nodes }, right);
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

        (Row { nodes }, None)
    }

    fn parse_delimiter_token(
        &mut self,
        command_start: usize,
        command: &str,
    ) -> Option<DelimiterKind> {
        let Some(ch) = self.peek_char() else {
            self.warnings.push(MathTextWarning {
                range: Some(command_start..self.pos),
                reason: MathTextWarningReason::MissingDelimiter,
                source: format!("\\{command}"),
            });
            return None;
        };

        if ch == '\\' {
            self.pos += 1;
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
                    return if ch == '|' {
                        Some(DelimiterKind::DoubleBar)
                    } else {
                        delimiter_symbol(ch)
                    };
                }
                return None;
            }
            return delimiter_command(name).or_else(|| {
                self.warnings.push(MathTextWarning {
                    range: Some(command_start..self.pos),
                    reason: MathTextWarningReason::MissingDelimiter,
                    source: format!("\\{command}"),
                });
                None
            });
        }

        self.pos += ch.len_utf8();
        delimiter_symbol(ch).or_else(|| {
            self.warnings.push(MathTextWarning {
                range: Some(command_start..self.pos),
                reason: MathTextWarningReason::MissingDelimiter,
                source: format!("\\{command}"),
            });
            None
        })
    }

    fn consume_environment_end(&mut self, environment: &str) -> bool {
        let marker = format!("\\end{{{environment}}}");
        if self.source[self.pos..].starts_with(&marker) {
            self.pos += marker.len();
            true
        } else {
            false
        }
    }

    fn starts_command(&self, command: &str) -> bool {
        let Some(rest) = self.source.get(self.pos..) else {
            return false;
        };
        let Some(rest) = rest.strip_prefix('\\') else {
            return false;
        };
        let Some(after_command) = rest.strip_prefix(command) else {
            return false;
        };
        after_command
            .chars()
            .next()
            .is_none_or(|ch| !ch.is_ascii_alphabetic())
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

    fn consume_ascii_whitespace(&mut self) {
        while let Some(ch) = self.peek_char() {
            if ch.is_ascii_whitespace() {
                self.pos += ch.len_utf8();
            } else {
                break;
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct LayoutBox {
    elements: Vec<MathElement>,
    width: f64,
    ascent: f64,
    descent: f64,
}

impl LayoutBox {
    fn height(&self) -> f64 {
        self.ascent + self.descent
    }
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
        Node::Kern(em) => LayoutBox {
            elements: Vec::new(),
            width: font_size_px * em,
            ascent: 0.0,
            descent: 0.0,
        },
        Node::Fraction {
            numerator,
            denominator,
        } => layout_fraction(numerator, denominator, font, font_size_px),
        Node::Binomial { upper, lower } => layout_binomial(upper, lower, font, font_size_px),
        Node::Radical { index, body } => layout_radical(index.as_ref(), body, font, font_size_px),
        Node::Matrix { rows, left, right } => {
            layout_matrix(rows, *left, *right, font, font_size_px)
        }
        Node::Substack { rows } => layout_substack(rows, font, font_size_px),
        Node::LineDecoration { kind, body } => {
            layout_line_decoration(*kind, body, font, font_size_px)
        }
        Node::Delimited { left, body, right } => {
            layout_delimited(*left, body, *right, font, font_size_px)
        }
        Node::Accent { kind, body } => layout_accent(*kind, body, font, font_size_px),
        Node::Styled { style, body } => {
            let resolved = resolve_styled_row(*style, body, font);
            layout_row(&resolved.nodes, font, font_size_px)
        }
        Node::LargeOperator { kind } => layout_large_operator(*kind, font, font_size_px),
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

    let mut children = Vec::new();
    children.extend(shift_elements(num.elements, num_x, num_baseline));
    children.push(MathElement::Rule {
        path: rect_path(0.0, -rule_thickness / 2.0, width, rule_thickness),
    });
    children.extend(shift_elements(den.elements, den_x, den_baseline));
    let path = combine_paths(
        &children
            .iter()
            .map(|element| element.path().clone())
            .collect::<Vec<_>>(),
    );
    let elements = vec![MathElement::Fraction { path }];

    LayoutBox {
        elements,
        width,
        ascent: num_baseline + num.ascent,
        descent: -den_baseline + den.descent,
    }
}

fn layout_binomial(upper: &Row, lower: &Row, font: &FontSource, font_size_px: f64) -> LayoutBox {
    let script_size = font_size_px * SCRIPT_SCALE;
    let upper = layout_row(&upper.nodes, font, script_size);
    let lower = layout_row(&lower.nodes, font, script_size);
    let pad = font_size_px * FRAC_PAD_EM;
    let gap = font_size_px * FRAC_GAP_EM;
    let stroke = (font_size_px * 0.045).max(1.0);
    let delimiter_width = delimiter_width(font_size_px, stroke);
    let inner_width = upper.width.max(lower.width) + 2.0 * pad;
    let upper_x = delimiter_width + (inner_width - upper.width) / 2.0;
    let lower_x = delimiter_width + (inner_width - lower.width) / 2.0;
    let upper_baseline = gap + upper.descent;
    let lower_baseline = -gap - lower.ascent;
    let ascent = upper_baseline + upper.ascent;
    let descent = -lower_baseline + lower.descent;
    let height = ascent + descent;
    let ymin = -descent;
    let right_x = delimiter_width + inner_width;

    let mut elements = Vec::new();
    elements.push(MathElement::Delimiter {
        kind: DelimiterKind::Paren,
        path: delimiter_path(
            DelimiterKind::Paren,
            0.0,
            ymin,
            delimiter_width,
            height,
            stroke,
            true,
        ),
    });
    elements.extend(shift_elements(upper.elements, upper_x, upper_baseline));
    elements.extend(shift_elements(lower.elements, lower_x, lower_baseline));
    elements.push(MathElement::Delimiter {
        kind: DelimiterKind::Paren,
        path: delimiter_path(
            DelimiterKind::Paren,
            right_x,
            ymin,
            delimiter_width,
            height,
            stroke,
            false,
        ),
    });

    LayoutBox {
        elements,
        width: right_x + delimiter_width,
        ascent,
        descent,
    }
}

fn layout_matrix(
    rows: &[Vec<Row>],
    left: DelimiterKind,
    right: DelimiterKind,
    font: &FontSource,
    font_size_px: f64,
) -> LayoutBox {
    if rows.is_empty() {
        return LayoutBox {
            elements: Vec::new(),
            width: 0.0,
            ascent: 0.0,
            descent: 0.0,
        };
    }

    let col_gap = font_size_px * MATRIX_COL_GAP_EM;
    let row_gap = font_size_px * MATRIX_ROW_GAP_EM;
    let pad = font_size_px * 0.08;
    let stroke = (font_size_px * 0.045).max(1.0);
    let delimiter_width = delimiter_width(font_size_px, stroke);
    let col_count = rows.iter().map(Vec::len).max().unwrap_or(0);

    let mut cells: Vec<Vec<LayoutBox>> = Vec::new();
    let mut col_widths = vec![0.0_f64; col_count];
    let mut row_ascents = Vec::with_capacity(rows.len());
    let mut row_descents = Vec::with_capacity(rows.len());

    for row in rows {
        let mut laid_out_row = Vec::with_capacity(row.len());
        let mut row_ascent: f64 = 0.0;
        let mut row_descent: f64 = 0.0;
        for (col, cell) in row.iter().enumerate() {
            let cell_box = layout_row(&cell.nodes, font, font_size_px);
            col_widths[col] = col_widths[col].max(cell_box.width);
            row_ascent = row_ascent.max(cell_box.ascent);
            row_descent = row_descent.max(cell_box.descent);
            laid_out_row.push(cell_box);
        }
        row_ascents.push(row_ascent);
        row_descents.push(row_descent);
        cells.push(laid_out_row);
    }

    let inner_width = col_widths.iter().sum::<f64>() + col_gap * col_count.saturating_sub(1) as f64;
    let total_height = row_ascents.iter().sum::<f64>()
        + row_descents.iter().sum::<f64>()
        + row_gap * rows.len().saturating_sub(1) as f64;
    let ascent = total_height / 2.0;
    let descent = total_height - ascent;

    let mut elements = Vec::new();
    let mut col_x = Vec::with_capacity(col_count);
    let mut x = if left != DelimiterKind::None {
        delimiter_width + pad
    } else {
        0.0
    };
    for (col, width) in col_widths.iter().copied().enumerate() {
        col_x.push(x);
        x += width;
        if col + 1 < col_count {
            x += col_gap;
        }
    }
    let inner_start_x = if left != DelimiterKind::None {
        delimiter_width + pad
    } else {
        0.0
    };
    let inner_end_x = inner_start_x + inner_width;

    let mut baseline = ascent - row_ascents[0];
    for (row_index, row_cells) in cells.into_iter().enumerate() {
        for (col, cell_box) in row_cells.into_iter().enumerate() {
            let cell_x = col_x[col] + (col_widths[col] - cell_box.width) / 2.0;
            elements.extend(shift_elements(cell_box.elements, cell_x, baseline));
        }
        if row_index + 1 < rows.len() {
            baseline -= row_descents[row_index] + row_gap + row_ascents[row_index + 1];
        }
    }

    if left != DelimiterKind::None {
        elements.insert(
            0,
            MathElement::Delimiter {
                kind: left,
                path: delimiter_path(
                    left,
                    0.0,
                    -descent,
                    delimiter_width,
                    total_height,
                    stroke,
                    true,
                ),
            },
        );
    }

    let mut width = inner_end_x;
    if right != DelimiterKind::None {
        width += pad;
        elements.push(MathElement::Delimiter {
            kind: right,
            path: delimiter_path(
                right,
                width,
                -descent,
                delimiter_width,
                total_height,
                stroke,
                false,
            ),
        });
        width += delimiter_width;
    }

    LayoutBox {
        elements,
        width,
        ascent,
        descent,
    }
}

fn layout_substack(rows: &[Row], font: &FontSource, font_size_px: f64) -> LayoutBox {
    if rows.is_empty() {
        return LayoutBox {
            elements: Vec::new(),
            width: 0.0,
            ascent: 0.0,
            descent: 0.0,
        };
    }

    let row_gap = font_size_px * 0.12;
    let laid_out: Vec<_> = rows
        .iter()
        .map(|row| layout_row(&row.nodes, font, font_size_px))
        .collect();
    let width = laid_out.iter().fold(0.0_f64, |acc, row| acc.max(row.width));
    let total_height = laid_out.iter().map(LayoutBox::height).sum::<f64>()
        + row_gap * rows.len().saturating_sub(1) as f64;
    let ascent = total_height / 2.0;
    let descent = total_height - ascent;

    let mut elements = Vec::new();
    let mut baseline = ascent - laid_out[0].ascent;
    for row_index in 0..laid_out.len() {
        let row = &laid_out[row_index];
        let x = (width - row.width) / 2.0;
        elements.extend(shift_elements(row.elements.clone(), x, baseline));
        if row_index + 1 < rows.len() {
            baseline -= row.descent + row_gap + laid_out[row_index + 1].ascent;
        }
    }

    LayoutBox {
        elements,
        width,
        ascent,
        descent,
    }
}

fn layout_accent(kind: AccentKind, body: &Row, font: &FontSource, font_size_px: f64) -> LayoutBox {
    let base = layout_row(&body.nodes, font, font_size_px);
    let mark_width = base.width.max(font_size_px * 0.45);
    let mark_height = (font_size_px * 0.12).max(1.0);
    let gap = font_size_px * 0.08;
    let mark_x = (base.width - mark_width) / 2.0;
    let mark_y = base.ascent + gap;
    let mark = accent_path(kind, mark_x, mark_y, mark_width, mark_height);
    let mut elements = base.elements;
    elements.push(MathElement::Accent { kind, path: mark });

    LayoutBox {
        elements,
        width: base.width,
        ascent: mark_y + mark_height,
        descent: base.descent,
    }
}

fn layout_line_decoration(
    kind: LineDecorationKind,
    body: &Row,
    font: &FontSource,
    font_size_px: f64,
) -> LayoutBox {
    let base = layout_row(&body.nodes, font, font_size_px);
    let rule_thickness = (font_size_px * LINE_DECORATION_RULE_EM).max(1.0);
    let gap = font_size_px * LINE_DECORATION_GAP_EM;
    let mut elements = base.elements;
    let (rule_y, ascent, descent) = match kind {
        LineDecorationKind::Overline => {
            let y = base.ascent + gap;
            (y, y + rule_thickness, base.descent)
        }
        LineDecorationKind::Underline => {
            let y = -base.descent - gap - rule_thickness;
            (y, base.ascent, -y)
        }
    };

    elements.push(MathElement::Rule {
        path: rect_path(0.0, rule_y, base.width, rule_thickness),
    });

    LayoutBox {
        elements,
        width: base.width,
        ascent,
        descent,
    }
}

fn layout_radical(
    index: Option<&Row>,
    body: &Row,
    font: &FontSource,
    font_size_px: f64,
) -> LayoutBox {
    let body_box = layout_row(&body.nodes, font, font_size_px);
    let index_box = index.map(|index| layout_row(&index.nodes, font, font_size_px * SCRIPT_SCALE));
    let gap = font_size_px * RADICAL_GAP_EM;
    let pad = font_size_px * RADICAL_PAD_EM;
    let rule_thickness = (font_size_px * RADICAL_RULE_EM).max(1.0);
    let sign_width = font_size_px * 0.45;
    let top_y = body_box.ascent + gap;
    let bottom_y = -body_box.descent;
    let total_height = (top_y - bottom_y).max(font_size_px);
    let index_reserved_width = index_box.as_ref().map_or(0.0, |index| index.width * 0.55);
    let sign_x = index_reserved_width;
    let body_x = sign_x + sign_width + pad;

    let mut elements = Vec::new();
    let mut ascent = top_y + rule_thickness;
    if let Some(index) = index_box {
        let index_baseline = top_y - index.descent - font_size_px * 0.12;
        ascent = ascent.max(index_baseline + index.ascent);
        elements.extend(shift_elements(index.elements, 0.0, index_baseline));
    }
    elements.push(MathElement::Radical {
        path: radical_path(sign_x, bottom_y, sign_width, total_height, rule_thickness),
    });
    elements.push(MathElement::Rule {
        path: rect_path(body_x, top_y, body_box.width + pad, rule_thickness),
    });
    elements.extend(shift_elements(body_box.elements, body_x, 0.0));

    LayoutBox {
        elements,
        width: body_x + body_box.width + pad,
        ascent,
        descent: body_box.descent,
    }
}

fn layout_delimited(
    left: DelimiterKind,
    body: &Row,
    right: DelimiterKind,
    font: &FontSource,
    font_size_px: f64,
) -> LayoutBox {
    let body_box = layout_row(&body.nodes, font, font_size_px);
    let pad = font_size_px * 0.08;
    let min_height = font_size_px * 1.2;
    let height = body_box.height().max(min_height);
    let ymin = -body_box.descent - (height - body_box.height()) / 2.0;
    let stroke = (font_size_px * 0.045).max(1.0);
    let delimiter_width = delimiter_width(font_size_px, stroke);

    let mut elements = Vec::new();
    let mut x = 0.0;
    if left != DelimiterKind::None {
        elements.push(MathElement::Delimiter {
            kind: left,
            path: delimiter_path(left, x, ymin, delimiter_width, height, stroke, true),
        });
        x += delimiter_width + pad;
    }

    elements.extend(shift_elements(body_box.elements, x, 0.0));
    x += body_box.width;

    if right != DelimiterKind::None {
        x += pad;
        elements.push(MathElement::Delimiter {
            kind: right,
            path: delimiter_path(right, x, ymin, delimiter_width, height, stroke, false),
        });
        x += delimiter_width;
    }

    LayoutBox {
        elements,
        width: x,
        ascent: (-ymin + height).max(body_box.ascent),
        descent: (-ymin).max(body_box.descent),
    }
}

fn layout_large_operator(
    kind: LargeOperatorKind,
    font: &FontSource,
    font_size_px: f64,
) -> LayoutBox {
    let text = large_operator_symbol(kind);
    let size = font_size_px * LARGE_OPERATOR_SCALE;
    let extent = font.measure(text, size);
    let path = font.text_to_path(text, size, [0.0, 0.0]);
    LayoutBox {
        elements: vec![MathElement::LargeOperator { kind, path }],
        width: extent.width,
        ascent: extent.ascent,
        descent: extent.descent,
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
        .map(|element| element.transformed(&transform))
        .collect()
}

fn accent_path(kind: AccentKind, x: f64, y: f64, width: f64, height: f64) -> Path {
    match kind {
        AccentKind::Hat => {
            Path::from_polyline(&[[x, y], [x + width * 0.5, y + height], [x + width, y]])
        }
        AccentKind::Bar => rect_path(x, y + height * 0.45, width, height * 0.18),
        AccentKind::Vec => {
            let shaft_y = y + height * 0.55;
            let head = height * 0.45;
            Path::from_polyline(&[
                [x, shaft_y],
                [x + width, shaft_y],
                [x + width - head, shaft_y + head],
                [x + width, shaft_y],
                [x + width - head, shaft_y - head],
            ])
        }
        AccentKind::Tilde => Path::from_polyline(&[
            [x, y + height * 0.45],
            [x + width * 0.25, y + height],
            [x + width * 0.5, y + height * 0.45],
            [x + width * 0.75, y],
            [x + width, y + height * 0.45],
        ]),
        AccentKind::Dot => rect_path(
            x + width * 0.5 - height * 0.25,
            y + height * 0.35,
            height * 0.5,
            height * 0.5,
        ),
        AccentKind::Ddot => {
            let dot = height * 0.45;
            let left = rect_path(x + width * 0.35 - dot * 0.5, y + height * 0.35, dot, dot);
            let right = rect_path(x + width * 0.65 - dot * 0.5, y + height * 0.35, dot, dot);
            combine_paths(&[left, right])
        }
    }
}

fn delimiter_width(font_size_px: f64, stroke: f64) -> f64 {
    (font_size_px * 0.32).max(stroke * 4.0)
}

fn delimiter_path(
    kind: DelimiterKind,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    stroke: f64,
    is_left: bool,
) -> Path {
    match kind {
        DelimiterKind::None => Path::new(Vec::new(), None),
        DelimiterKind::Paren => {
            let mid_y = y + height * 0.5;
            let inner_x = if is_left { x + width } else { x };
            let outer_x = if is_left {
                x + stroke
            } else {
                x + width - stroke
            };
            Path::from_polyline(&[
                [inner_x, y],
                [outer_x, y + height * 0.18],
                [outer_x, mid_y],
                [outer_x, y + height * 0.82],
                [inner_x, y + height],
            ])
        }
        DelimiterKind::Bracket => {
            let vertical_x = if is_left {
                x + stroke * 0.5
            } else {
                x + width - stroke * 0.5
            };
            let cap_x = if is_left { x + width } else { x };
            Path::from_polyline(&[
                [cap_x, y],
                [vertical_x, y],
                [vertical_x, y + height],
                [cap_x, y + height],
            ])
        }
        DelimiterKind::Brace => {
            let inner_x = if is_left { x + width } else { x };
            let outer_x = if is_left {
                x + stroke
            } else {
                x + width - stroke
            };
            let mid_y = y + height * 0.5;
            Path::from_polyline(&[
                [inner_x, y],
                [outer_x, y + height * 0.12],
                [outer_x, y + height * 0.35],
                [inner_x, mid_y],
                [outer_x, y + height * 0.65],
                [outer_x, y + height * 0.88],
                [inner_x, y + height],
            ])
        }
        DelimiterKind::Bar => rect_path(x + width * 0.5 - stroke * 0.5, y, stroke, height),
        DelimiterKind::DoubleBar => {
            let gap = stroke * 1.5;
            let first = rect_path(x + width * 0.5 - gap - stroke * 0.5, y, stroke, height);
            let second = rect_path(x + width * 0.5 + gap - stroke * 0.5, y, stroke, height);
            combine_paths(&[first, second])
        }
        DelimiterKind::Angle => {
            let inner_x = if is_left { x + width } else { x };
            let outer_x = if is_left {
                x + stroke
            } else {
                x + width - stroke
            };
            Path::from_polyline(&[
                [inner_x, y],
                [outer_x, y + height * 0.5],
                [inner_x, y + height],
            ])
        }
    }
}

fn radical_path(x: f64, bottom_y: f64, width: f64, height: f64, stroke: f64) -> Path {
    let y0 = bottom_y + height * 0.36;
    let y1 = bottom_y + height * 0.18;
    let y2 = bottom_y;
    let y3 = bottom_y + height;
    Path::from_polyline(&[
        [x, y0],
        [x + width * 0.22, y0],
        [x + width * 0.42, y1],
        [x + width * 0.62, y2],
        [x + width - stroke * 0.5, y3],
    ])
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
            Node::Kern(_) => {}
            Node::Fraction { .. } => out.push_str("\\frac"),
            Node::Binomial { .. } => out.push_str("\\binom"),
            Node::Radical { body, .. } => out.push_str(&flatten_row_text(body)),
            Node::Matrix { rows, .. } => out.push_str(&flatten_matrix_text(rows)),
            Node::Substack { rows } => out.push_str(&flatten_substack_text(rows)),
            Node::LineDecoration { body, .. } => out.push_str(&flatten_row_text(body)),
            Node::Delimited { body, .. } => out.push_str(&flatten_row_text(body)),
            Node::Accent { body, .. } => out.push_str(&flatten_row_text(body)),
            Node::Styled { body, .. } => out.push_str(&flatten_row_text(body)),
            Node::LargeOperator { kind } => out.push_str(large_operator_symbol(*kind)),
            Node::Script { base, .. } => out.push_str(&flatten_node_text(base)),
        }
    }
    out
}

fn flatten_node_text(node: &Node) -> String {
    match node {
        Node::Text(text) => text.clone(),
        Node::Space => " ".to_owned(),
        Node::Kern(_) => String::new(),
        Node::Fraction { .. } => "\\frac".to_owned(),
        Node::Binomial { .. } => "\\binom".to_owned(),
        Node::Radical { body, .. } => flatten_row_text(body),
        Node::Matrix { rows, .. } => flatten_matrix_text(rows),
        Node::Substack { rows } => flatten_substack_text(rows),
        Node::LineDecoration { body, .. } => flatten_row_text(body),
        Node::Delimited { body, .. } => flatten_row_text(body),
        Node::Accent { body, .. } => flatten_row_text(body),
        Node::Styled { body, .. } => flatten_row_text(body),
        Node::LargeOperator { kind } => large_operator_symbol(*kind).to_owned(),
        Node::Script { base, .. } => flatten_node_text(base),
    }
}

fn flatten_matrix_text(rows: &[Vec<Row>]) -> String {
    rows.iter()
        .flat_map(|row| row.iter())
        .map(flatten_row_text)
        .collect::<Vec<_>>()
        .join("")
}

fn flatten_substack_text(rows: &[Row]) -> String {
    rows.iter()
        .map(flatten_row_text)
        .collect::<Vec<_>>()
        .join("")
}

fn combine_paths(paths: &[Path]) -> Path {
    let mut vertices = Vec::new();
    let mut codes = Vec::new();
    for path in paths {
        vertices.extend_from_slice(path.vertices());
        if let Some(path_codes) = path.codes() {
            codes.extend_from_slice(path_codes);
        } else {
            for i in 0..path.vertices().len() {
                codes.push(if i == 0 {
                    crate::core::PathCode::MoveTo
                } else {
                    crate::core::PathCode::LineTo
                });
            }
        }
    }
    Path::new(vertices, Some(codes))
}

fn accent_kind(name: &str) -> Option<AccentKind> {
    match name {
        "hat" => Some(AccentKind::Hat),
        "bar" | "overline" => Some(AccentKind::Bar),
        "vec" => Some(AccentKind::Vec),
        "tilde" => Some(AccentKind::Tilde),
        "dot" => Some(AccentKind::Dot),
        "ddot" => Some(AccentKind::Ddot),
        _ => None,
    }
}

fn delimiter_symbol(ch: char) -> Option<DelimiterKind> {
    match ch {
        '(' | ')' => Some(DelimiterKind::Paren),
        '[' | ']' => Some(DelimiterKind::Bracket),
        '{' | '}' => Some(DelimiterKind::Brace),
        '|' => Some(DelimiterKind::Bar),
        '.' => Some(DelimiterKind::None),
        '<' | '>' => Some(DelimiterKind::Angle),
        _ => None,
    }
}

fn delimiter_command(name: &str) -> Option<DelimiterKind> {
    match name {
        "{" | "}" => Some(DelimiterKind::Brace),
        "|" | "Vert" | "lVert" | "rVert" => Some(DelimiterKind::DoubleBar),
        "langle" | "rangle" => Some(DelimiterKind::Angle),
        "lbrace" | "rbrace" => Some(DelimiterKind::Brace),
        "lbrack" | "rbrack" => Some(DelimiterKind::Bracket),
        "lparen" | "rparen" => Some(DelimiterKind::Paren),
        _ => None,
    }
}

fn large_operator_kind(name: &str) -> Option<LargeOperatorKind> {
    match name {
        "sum" => Some(LargeOperatorKind::Sum),
        "prod" => Some(LargeOperatorKind::Prod),
        "int" => Some(LargeOperatorKind::Integral),
        "oint" => Some(LargeOperatorKind::ContourIntegral),
        _ => None,
    }
}

fn large_operator_symbol(kind: LargeOperatorKind) -> &'static str {
    match kind {
        LargeOperatorKind::Sum => "∑",
        LargeOperatorKind::Prod => "∏",
        LargeOperatorKind::Integral => "∫",
        LargeOperatorKind::ContourIntegral => "∮",
    }
}

fn named_operator(name: &str) -> Option<&'static str> {
    match name {
        "arccos" => Some("arccos"),
        "arcsin" => Some("arcsin"),
        "arctan" => Some("arctan"),
        "arg" => Some("arg"),
        "cos" => Some("cos"),
        "cosh" => Some("cosh"),
        "cot" => Some("cot"),
        "coth" => Some("coth"),
        "csc" => Some("csc"),
        "deg" => Some("deg"),
        "det" => Some("det"),
        "dim" => Some("dim"),
        "exp" => Some("exp"),
        "gcd" => Some("gcd"),
        "hom" => Some("hom"),
        "inf" => Some("inf"),
        "ker" => Some("ker"),
        "lg" => Some("lg"),
        "lim" => Some("lim"),
        "liminf" => Some("liminf"),
        "limsup" => Some("limsup"),
        "ln" => Some("ln"),
        "log" => Some("log"),
        "max" => Some("max"),
        "min" => Some("min"),
        "Pr" => Some("Pr"),
        "sec" => Some("sec"),
        "sin" => Some("sin"),
        "sinh" => Some("sinh"),
        "sup" => Some("sup"),
        "tan" => Some("tan"),
        "tanh" => Some("tanh"),
        _ => None,
    }
}

fn single_char_spacing_command(ch: char) -> Option<f64> {
    match ch {
        ',' => Some(THIN_SPACE_EM),
        ':' => Some(MEDIUM_SPACE_EM),
        ';' => Some(THICK_SPACE_EM),
        '!' => Some(-THIN_SPACE_EM),
        _ => None,
    }
}

fn named_spacing_command(name: &str) -> Option<f64> {
    match name {
        "thinspace" => Some(THIN_SPACE_EM),
        "medspace" => Some(MEDIUM_SPACE_EM),
        "thickspace" => Some(THICK_SPACE_EM),
        "negthinspace" => Some(-THIN_SPACE_EM),
        "quad" => Some(QUAD_SPACE_EM),
        "qquad" => Some(2.0 * QUAD_SPACE_EM),
        _ => None,
    }
}

fn math_style_command(name: &str) -> Option<MathStyle> {
    match name {
        "mathbb" => Some(MathStyle::Blackboard),
        "mathcal" => Some(MathStyle::Calligraphic),
        "mathfrak" => Some(MathStyle::Fraktur),
        _ => None,
    }
}

/// Rewrites a styled group's body so each glyph uses the styled Unicode
/// codepoint when the font can render it, and the plain character otherwise.
///
/// The style substitution happens here, at layout time, because only then is the
/// [`FontSource`] available to check glyph coverage. Styling recurses into nested
/// text-bearing constructs so that, for example, a scripted styled atom keeps its
/// styling. Non-letters and codepoints the font lacks a glyph for degrade to the
/// plain character, which guarantees no styled character ever renders blank.
fn resolve_styled_row(style: MathStyle, row: &Row, font: &FontSource) -> Row {
    Row {
        nodes: row
            .nodes
            .iter()
            .map(|node| resolve_styled_node(style, node, font))
            .collect(),
    }
}

fn resolve_styled_node(style: MathStyle, node: &Node, font: &FontSource) -> Node {
    match node {
        Node::Text(text) => Node::Text(apply_math_style(style, text, font)),
        Node::Script { base, sup, sub } => Node::Script {
            base: Box::new(resolve_styled_node(style, base, font)),
            sup: sup.clone(),
            sub: sub.clone(),
        },
        Node::Styled { body, .. } => Node::Styled {
            style,
            body: body.clone(),
        },
        other => other.clone(),
    }
}

fn apply_math_style(style: MathStyle, text: &str, font: &FontSource) -> String {
    text.chars()
        .map(|ch| match math_style_char(style, ch) {
            Some(styled) if font.has_glyph(styled) => styled,
            _ => ch,
        })
        .collect()
}

fn math_style_char(style: MathStyle, ch: char) -> Option<char> {
    match style {
        MathStyle::Blackboard => blackboard_char(ch),
        MathStyle::Calligraphic => calligraphic_char(ch),
        MathStyle::Fraktur => fraktur_char(ch),
    }
}

fn char_from_base(base: u32, ch: char, start: char) -> Option<char> {
    let offset = u32::from(ch).checked_sub(u32::from(start))?;
    char::from_u32(base + offset)
}

fn blackboard_char(ch: char) -> Option<char> {
    match ch {
        'C' => Some('ℂ'),
        'H' => Some('ℍ'),
        'N' => Some('ℕ'),
        'P' => Some('ℙ'),
        'Q' => Some('ℚ'),
        'R' => Some('ℝ'),
        'Z' => Some('ℤ'),
        'A'..='Z' => char_from_base(0x1D538, ch, 'A'),
        'a'..='z' => char_from_base(0x1D552, ch, 'a'),
        '0'..='9' => char_from_base(0x1D7D8, ch, '0'),
        _ => None,
    }
}

fn calligraphic_char(ch: char) -> Option<char> {
    match ch {
        'B' => Some('ℬ'),
        'E' => Some('ℰ'),
        'F' => Some('ℱ'),
        'H' => Some('ℋ'),
        'I' => Some('ℐ'),
        'L' => Some('ℒ'),
        'M' => Some('ℳ'),
        'R' => Some('ℛ'),
        'A'..='Z' => char_from_base(0x1D49C, ch, 'A'),
        'a'..='z' => char_from_base(0x1D4B6, ch, 'a'),
        _ => None,
    }
}

fn fraktur_char(ch: char) -> Option<char> {
    match ch {
        'C' => Some('ℭ'),
        'H' => Some('ℌ'),
        'I' => Some('ℑ'),
        'R' => Some('ℜ'),
        'Z' => Some('ℨ'),
        'A'..='Z' => char_from_base(0x1D504, ch, 'A'),
        'a'..='z' => char_from_base(0x1D51E, ch, 'a'),
        _ => None,
    }
}

fn command_symbol(name: &str) -> Option<&'static str> {
    match name {
        "alpha" => Some("α"),
        "beta" => Some("β"),
        "gamma" => Some("γ"),
        "delta" => Some("δ"),
        "epsilon" => Some("ε"),
        "varepsilon" => Some("ε"),
        "zeta" => Some("ζ"),
        "eta" => Some("η"),
        "theta" => Some("θ"),
        "vartheta" => Some("ϑ"),
        "iota" => Some("ι"),
        "kappa" => Some("κ"),
        "lambda" => Some("λ"),
        "mu" => Some("μ"),
        "nu" => Some("ν"),
        "xi" => Some("ξ"),
        "omicron" => Some("ο"),
        "pi" => Some("π"),
        "varpi" => Some("ϖ"),
        "rho" => Some("ρ"),
        "varrho" => Some("ϱ"),
        "sigma" => Some("σ"),
        "varsigma" => Some("ς"),
        "tau" => Some("τ"),
        "upsilon" => Some("υ"),
        "phi" => Some("φ"),
        "varphi" => Some("ϕ"),
        "chi" => Some("χ"),
        "psi" => Some("ψ"),
        "omega" => Some("ω"),
        "Gamma" => Some("Γ"),
        "Delta" => Some("Δ"),
        "Theta" => Some("Θ"),
        "Lambda" => Some("Λ"),
        "Xi" => Some("Ξ"),
        "Pi" => Some("Π"),
        "Sigma" => Some("Σ"),
        "Upsilon" => Some("Υ"),
        "Phi" => Some("Φ"),
        "Psi" => Some("Ψ"),
        "Omega" => Some("Ω"),
        "times" => Some("×"),
        "div" => Some("÷"),
        "pm" => Some("±"),
        "mp" => Some("∓"),
        "ast" => Some("∗"),
        "star" => Some("⋆"),
        "dagger" => Some("†"),
        "ddagger" => Some("‡"),
        "oplus" => Some("⊕"),
        "ominus" => Some("⊖"),
        "otimes" => Some("⊗"),
        "oslash" => Some("⊘"),
        "odot" => Some("⊙"),
        "leq" => Some("≤"),
        "le" => Some("≤"),
        "geq" => Some("≥"),
        "ge" => Some("≥"),
        "neq" => Some("≠"),
        "ne" => Some("≠"),
        "lt" => Some("<"),
        "gt" => Some(">"),
        "ll" => Some("≪"),
        "gg" => Some("≫"),
        "approx" => Some("≈"),
        "sim" => Some("∼"),
        "simeq" => Some("≃"),
        "equiv" => Some("≡"),
        "propto" => Some("∝"),
        "cong" => Some("≅"),
        "asymp" => Some("≍"),
        "doteq" => Some("≐"),
        "models" => Some("⊨"),
        "perp" => Some("⊥"),
        "mid" => Some("∣"),
        "parallel" => Some("∥"),
        "infty" => Some("∞"),
        "partial" => Some("∂"),
        "nabla" => Some("∇"),
        "cdot" => Some("⋅"),
        "bullet" => Some("•"),
        "circ" => Some("∘"),
        "degree" => Some("°"),
        "prime" => Some("′"),
        "backprime" => Some("‵"),
        "ell" => Some("ℓ"),
        "hbar" => Some("ℏ"),
        "Re" => Some("ℜ"),
        "Im" => Some("ℑ"),
        "wp" => Some("℘"),
        "aleph" => Some("ℵ"),
        "rightarrow" | "to" => Some("→"),
        "leftarrow" | "gets" => Some("←"),
        "uparrow" => Some("↑"),
        "downarrow" => Some("↓"),
        "updownarrow" => Some("↕"),
        "mapsto" => Some("↦"),
        "longrightarrow" => Some("⟶"),
        "longleftarrow" => Some("⟵"),
        "longleftrightarrow" => Some("⟷"),
        "hookrightarrow" => Some("↪"),
        "hookleftarrow" => Some("↩"),
        "nearrow" => Some("↗"),
        "searrow" => Some("↘"),
        "swarrow" => Some("↙"),
        "nwarrow" => Some("↖"),
        "leftrightarrow" => Some("↔"),
        "Rightarrow" => Some("⇒"),
        "Leftarrow" => Some("⇐"),
        "Leftrightarrow" => Some("⇔"),
        "Uparrow" => Some("⇑"),
        "Downarrow" => Some("⇓"),
        "Updownarrow" => Some("⇕"),
        "in" => Some("∈"),
        "notin" => Some("∉"),
        "subset" => Some("⊂"),
        "subseteq" => Some("⊆"),
        "nsubseteq" => Some("⊈"),
        "supset" => Some("⊃"),
        "supseteq" => Some("⊇"),
        "nsupseteq" => Some("⊉"),
        "setminus" => Some("∖"),
        "cup" => Some("∪"),
        "cap" => Some("∩"),
        "emptyset" => Some("∅"),
        "varnothing" => Some("∅"),
        "forall" => Some("∀"),
        "exists" => Some("∃"),
        "nexists" => Some("∄"),
        "land" => Some("∧"),
        "wedge" => Some("∧"),
        "lor" => Some("∨"),
        "vee" => Some("∨"),
        "neg" => Some("¬"),
        "top" => Some("⊤"),
        "bot" => Some("⊥"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{MathMode, TextRun};

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
                _ => None,
            })
            .collect();
        assert_eq!(texts, ["α", "+", "β"]);
        assert!(layout.warnings.is_empty());
    }

    #[test]
    fn expanded_symbol_table_maps_common_commands() {
        let layout = layout_math(
            "\\leq\\approx\\nabla\\rightarrow\\subseteq\\oplus\\mapsto\\parallel\\aleph",
            &font(),
            20.0,
        );
        let text: String = layout
            .elements
            .iter()
            .filter_map(|element| match element {
                MathElement::Glyph { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(text, "≤≈∇→⊆⊕↦∥ℵ");
        assert!(layout.warnings.is_empty());
    }

    #[test]
    fn math_style_commands_map_covered_letters_to_unicode() {
        // ℝ, 𝟚 and ℱ all exist in DejaVu Sans, so they keep the styled codepoint;
        // fraktur `g` (𝔤) is absent and must fall back to the plain ASCII glyph.
        let layout = layout_math("\\mathbb{R2}+\\mathcal{F}+\\mathfrak{g}", &font(), 20.0);
        let text: String = layout
            .elements
            .iter()
            .filter_map(|element| match element {
                MathElement::Glyph { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(text, "ℝ𝟚+ℱ+g");
        assert!(layout.warnings.is_empty());
    }

    #[test]
    fn math_style_falls_back_to_plain_glyph_when_font_lacks_styled_codepoint() {
        // DejaVu Sans has no fraktur glyphs, so `\mathfrak{g}` must render the
        // plain "g" rather than a blank/notdef box.
        let layout = layout_math("\\mathfrak{g}", &font(), 20.0);
        let glyphs: Vec<&str> = layout
            .elements
            .iter()
            .filter_map(|element| match element {
                MathElement::Glyph { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(glyphs, vec!["g"]);
        assert!(!font().has_glyph('𝔤'));
        assert!(layout.warnings.is_empty());
    }

    #[test]
    fn math_style_keeps_styled_codepoint_when_font_has_glyph() {
        // ℝ (double-struck R) is present in DejaVu Sans, so it stays styled.
        assert!(font().has_glyph('ℝ'));
        let layout = layout_math("\\mathbb{R}", &font(), 20.0);
        let glyphs: Vec<&str> = layout
            .elements
            .iter()
            .filter_map(|element| match element {
                MathElement::Glyph { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(glyphs, vec!["ℝ"]);
        assert!(layout.warnings.is_empty());
    }

    #[test]
    fn math_style_commands_preserve_non_letters_and_take_scripts() {
        let styled = layout_math("\\mathbb{R}_0", &font(), 20.0);
        let plain = layout_math("\\mathbb{R}", &font(), 20.0);
        let text: String = styled
            .elements
            .iter()
            .filter_map(|element| match element {
                MathElement::Glyph { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(text, "ℝ0");
        assert!(styled.width > plain.width);
        assert!(styled.descent > plain.descent);
        assert!(styled.warnings.is_empty());
    }

    #[test]
    fn math_style_missing_argument_warns_and_preserves_command() {
        let layout = layout_math("\\mathcal+x", &font(), 20.0);

        assert_eq!(layout.warnings.len(), 1);
        assert_eq!(
            layout.warnings[0].reason,
            MathTextWarningReason::MissingCommandArgument
        );
        assert!(layout.elements.iter().any(
            |element| matches!(element, MathElement::Glyph { text, .. } if text == "\\mathcal")
        ));
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
    fn spacing_commands_adjust_width_without_glyphs_or_warnings() {
        let tight = layout_math("ab", &font(), 18.0);
        let thin = layout_math("a\\,b", &font(), 18.0);
        let med = layout_math("a\\:b", &font(), 18.0);
        let thick = layout_math("a\\;b", &font(), 18.0);
        let quad = layout_math("a\\quad b", &font(), 18.0);
        let qquad = layout_math("a\\qquad b", &font(), 18.0);
        let neg = layout_math("a\\!b", &font(), 18.0);

        assert!(neg.width < tight.width);
        assert!(thin.width > tight.width);
        assert!(med.width > thin.width);
        assert!(thick.width > med.width);
        assert!(quad.width > thick.width);
        assert!(qquad.width > quad.width);

        for layout in [&thin, &med, &thick, &quad, &qquad, &neg] {
            assert_eq!(
                layout
                    .elements
                    .iter()
                    .filter(|element| matches!(element, MathElement::Glyph { .. }))
                    .count(),
                2
            );
            assert!(layout.warnings.is_empty());
        }
    }

    #[test]
    fn named_spacing_aliases_match_symbol_spacing_commands() {
        let thin = layout_math("a\\,b", &font(), 18.0);
        let named_thin = layout_math("a\\thinspace b", &font(), 18.0);
        let neg = layout_math("a\\!b", &font(), 18.0);
        let named_neg = layout_math("a\\negthinspace b", &font(), 18.0);

        assert!((thin.width - named_thin.width).abs() < 1e-9);
        assert!((neg.width - named_neg.width).abs() < 1e-9);
        assert!(named_thin.warnings.is_empty());
        assert!(named_neg.warnings.is_empty());
    }

    #[test]
    fn fraction_emits_single_combined_element_and_centers_parts() {
        let layout = layout_math("\\frac{a}{b}", &font(), 24.0);
        assert_eq!(
            layout
                .elements
                .iter()
                .filter(|element| matches!(element, MathElement::Fraction { .. }))
                .count(),
            1
        );
        assert!(
            layout
                .elements
                .iter()
                .all(|element| !matches!(element, MathElement::Rule { .. }))
        );
        assert!(layout.elements[0].path().vertices().len() > 5);
        assert!(layout.ascent > 0.0);
        assert!(layout.descent > 0.0);
        assert!(layout.height() > 24.0);
    }

    #[test]
    fn fraction_is_taller_than_either_operand() {
        let numerator = layout_math("a", &font(), 24.0);
        let denominator = layout_math("b", &font(), 24.0);
        let fraction = layout_math("\\frac{a}{b}", &font(), 24.0);

        assert!(fraction.height() > numerator.height());
        assert!(fraction.height() > denominator.height());
        assert!(fraction.ascent > numerator.ascent);
        assert!(fraction.descent > denominator.descent);
    }

    #[test]
    fn nested_fraction_remains_single_rendered_subtree() {
        let layout = layout_math("\\frac{1}{\\frac{x}{y}}", &font(), 24.0);
        let fraction_count = layout
            .elements
            .iter()
            .filter(|element| matches!(element, MathElement::Fraction { .. }))
            .count();

        assert_eq!(fraction_count, 1);
        assert!(layout.height() > layout_math("\\frac{1}{x}", &font(), 24.0).height());
        assert!(layout.warnings.is_empty());
    }

    #[test]
    fn binomial_stacks_terms_inside_parentheses_without_rule() {
        let upper = layout_math("n", &font(), 24.0);
        let lower = layout_math("k", &font(), 24.0);
        let binom = layout_math("\\binom{n}{k}", &font(), 24.0);
        let texts: Vec<_> = binom
            .elements
            .iter()
            .filter_map(|element| match element {
                MathElement::Glyph { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        assert!(texts.contains(&"n"));
        assert!(texts.contains(&"k"));
        assert_eq!(
            binom
                .elements
                .iter()
                .filter(|element| matches!(
                    element,
                    MathElement::Delimiter {
                        kind: DelimiterKind::Paren,
                        ..
                    }
                ))
                .count(),
            2
        );
        assert!(
            !binom
                .elements
                .iter()
                .any(|element| matches!(element, MathElement::Rule { .. }))
        );
        assert!(binom.height() > upper.height());
        assert!(binom.height() > lower.height());
        assert!(binom.warnings.is_empty());
    }

    #[test]
    fn binomial_can_take_scripts() {
        let binom = layout_math("\\binom{n}{k}", &font(), 24.0);
        let scripted = layout_math("\\binom{n}{k}^2", &font(), 24.0);
        let texts: Vec<_> = scripted
            .elements
            .iter()
            .filter_map(|element| match element {
                MathElement::Glyph { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        assert!(texts.contains(&"2"));
        assert!(scripted.width > binom.width);
        assert!(scripted.ascent > binom.ascent);
        assert!(scripted.warnings.is_empty());
    }

    #[test]
    fn binomial_missing_argument_warns_and_preserves_command() {
        let layout = layout_math("\\binom{n}+x", &font(), 20.0);

        assert_eq!(layout.warnings.len(), 1);
        assert_eq!(
            layout.warnings[0].reason,
            MathTextWarningReason::MissingCommandArgument
        );
        assert!(layout.elements.iter().any(
            |element| matches!(element, MathElement::Glyph { text, .. } if text == "\\binom")
        ));
    }

    #[test]
    fn matrix_environment_lays_out_aligned_cells_without_delimiters() {
        let single_row = layout_math("ab", &font(), 24.0);
        let matrix = layout_math("\\begin{matrix}a&b\\\\c&d\\end{matrix}", &font(), 24.0);
        let texts: Vec<_> = matrix
            .elements
            .iter()
            .filter_map(|element| match element {
                MathElement::Glyph { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(texts, ["a", "b", "c", "d"]);
        assert!(
            !matrix
                .elements
                .iter()
                .any(|element| matches!(element, MathElement::Delimiter { .. }))
        );
        assert!(matrix.width > single_row.width);
        assert!(matrix.height() > single_row.height());
        assert!(matrix.warnings.is_empty());
    }

    #[test]
    fn pmatrix_and_bmatrix_add_matching_delimiters() {
        let plain = layout_math("\\begin{matrix}a&b\\\\c&d\\end{matrix}", &font(), 24.0);
        let pmatrix = layout_math("\\begin{pmatrix}a&b\\\\c&d\\end{pmatrix}", &font(), 24.0);
        let bmatrix = layout_math("\\begin{bmatrix}a&b\\\\c&d\\end{bmatrix}", &font(), 24.0);
        let parens = pmatrix
            .elements
            .iter()
            .filter(|element| {
                matches!(
                    element,
                    MathElement::Delimiter {
                        kind: DelimiterKind::Paren,
                        ..
                    }
                )
            })
            .count();
        let brackets = bmatrix
            .elements
            .iter()
            .filter(|element| {
                matches!(
                    element,
                    MathElement::Delimiter {
                        kind: DelimiterKind::Bracket,
                        ..
                    }
                )
            })
            .count();

        assert_eq!(parens, 2);
        assert_eq!(brackets, 2);
        assert!(pmatrix.width > plain.width);
        assert!(pmatrix.warnings.is_empty());
        assert!(bmatrix.warnings.is_empty());
    }

    #[test]
    fn cases_environment_adds_left_brace_only() {
        let plain = layout_math("\\begin{matrix}x&x>0\\\\-x&x<0\\end{matrix}", &font(), 24.0);
        let cases = layout_math("\\begin{cases}x&x>0\\\\-x&x<0\\end{cases}", &font(), 24.0);
        let brace_count = cases
            .elements
            .iter()
            .filter(|element| {
                matches!(
                    element,
                    MathElement::Delimiter {
                        kind: DelimiterKind::Brace,
                        ..
                    }
                )
            })
            .count();
        let texts: Vec<_> = cases
            .elements
            .iter()
            .filter_map(|element| match element {
                MathElement::Glyph { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(brace_count, 1);
        assert_eq!(texts, ["x", "x", ">", "0", "-", "x", "x", "<", "0"]);
        assert!(cases.width > plain.width);
        assert_eq!(cases.height(), plain.height());
        assert!(cases.warnings.is_empty());
    }

    #[test]
    fn cases_environment_can_take_scripts() {
        let cases = layout_math("\\begin{cases}x&x>0\\\\0&x=0\\end{cases}", &font(), 24.0);
        let scripted = layout_math("\\begin{cases}x&x>0\\\\0&x=0\\end{cases}_i", &font(), 24.0);

        assert!(scripted.width > cases.width);
        assert!(scripted.descent > cases.descent);
        assert!(scripted.warnings.is_empty());
    }

    #[test]
    fn substack_lays_out_centered_rows_without_delimiters() {
        let one_row = layout_math("i=0", &font(), 20.0);
        let substack = layout_math("\\substack{i=0\\\\j<n}", &font(), 20.0);
        let texts: Vec<_> = substack
            .elements
            .iter()
            .filter_map(|element| match element {
                MathElement::Glyph { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(texts.join(""), "i=0j<n");
        assert!(substack.width >= one_row.width);
        assert!(substack.height() > one_row.height());
        assert!(
            !substack
                .elements
                .iter()
                .any(|element| matches!(element, MathElement::Delimiter { .. }))
        );
        assert!(substack.warnings.is_empty());
    }

    #[test]
    fn substack_works_as_large_operator_script() {
        let plain = layout_math("\\sum", &font(), 24.0);
        let scripted = layout_math("\\sum_{\\substack{i=0\\\\j<n}} x", &font(), 24.0);
        let texts: String = scripted
            .elements
            .iter()
            .filter_map(|element| match element {
                MathElement::Glyph { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        assert!(texts.contains("i=0"));
        assert!(texts.contains("j<n"));
        assert!(scripted.width > plain.width);
        assert!(scripted.descent > plain.descent);
        assert!(scripted.warnings.is_empty());
    }

    #[test]
    fn substack_missing_argument_warns_and_preserves_command() {
        let layout = layout_math("\\substack+x", &font(), 20.0);

        assert_eq!(layout.warnings.len(), 1);
        assert_eq!(
            layout.warnings[0].reason,
            MathTextWarningReason::MissingCommandArgument
        );
        assert!(layout.elements.iter().any(
            |element| matches!(element, MathElement::Glyph { text, .. } if text == "\\substack")
        ));
    }

    #[test]
    fn matrix_environment_can_take_scripts() {
        let matrix = layout_math("\\begin{bmatrix}a&b\\\\c&d\\end{bmatrix}", &font(), 24.0);
        let scripted = layout_math(
            "\\begin{bmatrix}a&b\\\\c&d\\end{bmatrix}^{-1}",
            &font(),
            24.0,
        );
        let texts: Vec<_> = scripted
            .elements
            .iter()
            .filter_map(|element| match element {
                MathElement::Glyph { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        assert!(texts.contains(&"-"));
        assert!(texts.contains(&"1"));
        assert!(scripted.width > matrix.width);
        assert!(scripted.ascent > matrix.ascent);
        assert!(scripted.warnings.is_empty());
    }

    #[test]
    fn matrix_environment_recovers_from_bad_environment_boundaries() {
        let unsupported = layout_math("\\begin{array}a&b\\end{array}", &font(), 20.0);
        assert_eq!(unsupported.warnings.len(), 2);
        assert_eq!(
            unsupported.warnings[0].reason,
            MathTextWarningReason::UnsupportedCommand
        );
        assert!(unsupported.elements.iter().any(
            |element| matches!(element, MathElement::Glyph { text, .. } if text == "\\begin{array}")
        ));

        let unclosed = layout_math("\\begin{matrix}a&b", &font(), 20.0);
        assert_eq!(unclosed.warnings.len(), 1);
        assert_eq!(
            unclosed.warnings[0].reason,
            MathTextWarningReason::UnclosedGroup
        );
        assert!(
            unclosed
                .elements
                .iter()
                .any(|element| matches!(element, MathElement::Glyph { text, .. } if text == "a"))
        );
        assert!(
            unclosed
                .elements
                .iter()
                .any(|element| matches!(element, MathElement::Glyph { text, .. } if text == "b"))
        );
    }

    #[test]
    fn radical_emits_sign_and_overbar_rule() {
        let body = layout_math("x+1", &font(), 24.0);
        let radical = layout_math("\\sqrt{x+1}", &font(), 24.0);

        assert_eq!(
            radical
                .elements
                .iter()
                .filter(|element| matches!(element, MathElement::Radical { .. }))
                .count(),
            1
        );
        assert_eq!(
            radical
                .elements
                .iter()
                .filter(|element| matches!(element, MathElement::Rule { .. }))
                .count(),
            1
        );
        assert!(radical.width > body.width);
        assert!(radical.ascent > body.ascent);
        assert_eq!(radical.descent, body.descent);
        assert!(radical.warnings.is_empty());
    }

    #[test]
    fn nth_root_places_index_at_upper_left() {
        let square_root = layout_math("\\sqrt{x}", &font(), 24.0);
        let cube_root = layout_math("\\sqrt[3]{x}", &font(), 24.0);
        let texts: Vec<_> = cube_root
            .elements
            .iter()
            .filter_map(|element| match element {
                MathElement::Glyph { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        assert!(texts.contains(&"3"));
        assert!(texts.contains(&"x"));
        assert_eq!(
            cube_root
                .elements
                .iter()
                .filter(|element| matches!(element, MathElement::Radical { .. }))
                .count(),
            1
        );
        assert_eq!(
            cube_root
                .elements
                .iter()
                .filter(|element| matches!(element, MathElement::Rule { .. }))
                .count(),
            1
        );
        assert!(cube_root.width > square_root.width);
        assert!(cube_root.ascent >= square_root.ascent);
        assert_eq!(cube_root.descent, square_root.descent);
        assert!(cube_root.warnings.is_empty());
    }

    #[test]
    fn nth_root_accepts_scripts_on_whole_radical() {
        let layout = layout_math("\\sqrt[3]{x}^2", &font(), 24.0);
        let texts: Vec<_> = layout
            .elements
            .iter()
            .filter_map(|element| match element {
                MathElement::Glyph { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        assert!(texts.contains(&"3"));
        assert!(texts.contains(&"x"));
        assert!(texts.contains(&"2"));
        assert!(layout.ascent > layout_math("\\sqrt[3]{x}", &font(), 24.0).ascent);
        assert!(layout.warnings.is_empty());
    }

    #[test]
    fn radical_missing_argument_warns_and_preserves_command() {
        let layout = layout_math("\\sqrt+x", &font(), 20.0);

        assert_eq!(layout.warnings.len(), 1);
        assert_eq!(
            layout.warnings[0].reason,
            MathTextWarningReason::MissingCommandArgument
        );
        assert!(
            layout.elements.iter().any(
                |element| matches!(element, MathElement::Glyph { text, .. } if text == "\\sqrt")
            )
        );
    }

    #[test]
    fn nth_root_missing_body_warns_and_preserves_command() {
        let layout = layout_math("\\sqrt[3]+x", &font(), 20.0);

        assert_eq!(layout.warnings.len(), 1);
        assert_eq!(
            layout.warnings[0].reason,
            MathTextWarningReason::MissingCommandArgument
        );
        assert!(
            layout.elements.iter().any(
                |element| matches!(element, MathElement::Glyph { text, .. } if text == "\\sqrt")
            )
        );
    }

    #[test]
    fn text_command_preserves_literal_roman_text() {
        let layout = layout_math("x\\text{ if y_1 }", &font(), 20.0);
        let texts: Vec<_> = layout
            .elements
            .iter()
            .filter_map(|element| match element {
                MathElement::Glyph { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(texts, ["x", " if y_1 "]);
        assert!(layout.warnings.is_empty());
    }

    #[test]
    fn text_command_allows_nested_braces_and_escapes() {
        let layout = layout_math("\\text{set \\{A\\} and {B}}", &font(), 20.0);
        let text = layout.elements.iter().find_map(|element| match element {
            MathElement::Glyph { text, .. } => Some(text.as_str()),
            _ => None,
        });

        assert_eq!(text, Some("set {A} and {B}"));
        assert!(layout.warnings.is_empty());
    }

    #[test]
    fn operatorname_preserves_literal_text_and_takes_scripts() {
        let layout = layout_math("\\operatorname{Var}_x+1", &font(), 20.0);
        let plain = layout_math("\\operatorname{Var}", &font(), 20.0);
        let text: String = layout
            .elements
            .iter()
            .filter_map(|element| match element {
                MathElement::Glyph { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(text, "Varx+1");
        assert!(layout.width > plain.width);
        assert!(layout.descent > plain.descent);
        assert!(layout.warnings.is_empty());
    }

    #[test]
    fn named_operators_render_without_fallback_warnings() {
        let layout = layout_math("\\sin x+\\log y+\\lim_{n} a_n", &font(), 20.0);
        let text: String = layout
            .elements
            .iter()
            .filter_map(|element| match element {
                MathElement::Glyph { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        assert_eq!(text, "sinx+logy+limnan");
        assert!(layout.warnings.is_empty());
    }

    #[test]
    fn operatorname_missing_argument_warns_and_preserves_command() {
        let layout = layout_math("\\operatorname+x", &font(), 20.0);

        assert_eq!(layout.warnings.len(), 1);
        assert_eq!(
            layout.warnings[0].reason,
            MathTextWarningReason::MissingCommandArgument
        );
        assert!(layout.elements.iter().any(
            |element| matches!(element, MathElement::Glyph { text, .. } if text == "\\operatorname")
        ));
    }

    #[test]
    fn accents_emit_mark_paths_above_base() {
        let plain = layout_math("x", &font(), 24.0);
        let accented = layout_math("\\hat{x}", &font(), 24.0);
        let accent_path = accented.elements.iter().find_map(|element| match element {
            MathElement::Accent {
                kind: AccentKind::Hat,
                path,
            } => Some(path),
            _ => None,
        });
        let accent_path = accent_path.expect("hat accent should emit an accent path");

        assert!(accented.ascent > plain.ascent);
        assert!(accent_path.get_extents().ymin() >= plain.ascent);
        assert!(accented.warnings.is_empty());
    }

    #[test]
    fn vector_accent_can_take_script() {
        let layout = layout_math("\\vec{x}_i", &font(), 24.0);
        assert!(layout.elements.iter().any(|element| matches!(
            element,
            MathElement::Accent {
                kind: AccentKind::Vec,
                ..
            }
        )));
        assert!(layout.descent > layout_math("\\vec{x}", &font(), 24.0).descent);
    }

    #[test]
    fn overline_adds_rule_above_body() {
        let plain = layout_math("xy", &font(), 24.0);
        let overline = layout_math("\\overline{xy}", &font(), 24.0);

        assert_eq!(
            overline
                .elements
                .iter()
                .filter(|element| matches!(element, MathElement::Rule { .. }))
                .count(),
            1
        );
        assert!(overline.ascent > plain.ascent);
        assert_eq!(overline.descent, plain.descent);
        assert_eq!(overline.width, plain.width);
        assert!(overline.warnings.is_empty());
    }

    #[test]
    fn underline_adds_rule_below_body() {
        let plain = layout_math("xy", &font(), 24.0);
        let underline = layout_math("\\underline{xy}", &font(), 24.0);

        assert_eq!(
            underline
                .elements
                .iter()
                .filter(|element| matches!(element, MathElement::Rule { .. }))
                .count(),
            1
        );
        assert_eq!(underline.ascent, plain.ascent);
        assert!(underline.descent > plain.descent);
        assert_eq!(underline.width, plain.width);
        assert!(underline.warnings.is_empty());
    }

    #[test]
    fn line_decorations_can_take_scripts() {
        let decorated = layout_math("\\overline{x}^2", &font(), 24.0);
        let without_script = layout_math("\\overline{x}", &font(), 24.0);
        let texts: Vec<_> = decorated
            .elements
            .iter()
            .filter_map(|element| match element {
                MathElement::Glyph { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect();

        assert!(texts.contains(&"x"));
        assert!(texts.contains(&"2"));
        assert!(decorated.ascent > without_script.ascent);
        assert!(decorated.width > without_script.width);
        assert!(decorated.warnings.is_empty());
    }

    #[test]
    fn line_decoration_missing_argument_warns_and_preserves_command() {
        let layout = layout_math("\\overline+x", &font(), 20.0);

        assert_eq!(layout.warnings.len(), 1);
        assert_eq!(
            layout.warnings[0].reason,
            MathTextWarningReason::MissingCommandArgument
        );
        assert!(layout.elements.iter().any(
            |element| matches!(element, MathElement::Glyph { text, .. } if text == "\\overline")
        ));
    }

    #[test]
    fn accent_missing_argument_warns_and_preserves_command() {
        let layout = layout_math("\\hat", &font(), 20.0);
        assert_eq!(layout.warnings.len(), 1);
        assert_eq!(
            layout.warnings[0].reason,
            MathTextWarningReason::MissingCommandArgument
        );
        assert_eq!(layout.warnings[0].source, "\\hat");
        assert!(
            layout.elements.iter().any(
                |element| matches!(element, MathElement::Glyph { text, .. } if text == "\\hat")
            )
        );
    }

    #[test]
    fn element_path_accessor_covers_all_variants() {
        let layout = layout_math(
            "\\left(\\hat{\\sqrt{\\frac{x}{y}}}\\right)+\\sum",
            &font(),
            24.0,
        );
        assert!(
            layout
                .elements
                .iter()
                .any(|element| matches!(element, MathElement::Glyph { .. }))
        );
        assert!(
            layout
                .elements
                .iter()
                .any(|element| matches!(element, MathElement::Fraction { .. }))
        );
        assert!(
            layout
                .elements
                .iter()
                .any(|element| matches!(element, MathElement::Rule { .. }))
        );
        assert!(
            layout
                .elements
                .iter()
                .any(|element| matches!(element, MathElement::Accent { .. }))
        );
        assert!(
            layout
                .elements
                .iter()
                .any(|element| matches!(element, MathElement::Delimiter { .. }))
        );
        assert!(
            layout
                .elements
                .iter()
                .any(|element| matches!(element, MathElement::LargeOperator { .. }))
        );
        assert!(
            layout
                .elements
                .iter()
                .any(|element| matches!(element, MathElement::Radical { .. }))
        );

        for element in &layout.elements {
            assert!(!element.path().vertices().is_empty());
        }
    }

    #[test]
    fn left_right_emit_stretched_delimiters() {
        let body = layout_math("\\frac{a}{b}", &font(), 24.0);
        let delimited = layout_math("\\left(\\frac{a}{b}\\right)", &font(), 24.0);
        let delimiters: Vec<_> = delimited
            .elements
            .iter()
            .filter_map(|element| match element {
                MathElement::Delimiter { kind, path } => Some((*kind, path.get_extents())),
                _ => None,
            })
            .collect();

        assert_eq!(delimiters.len(), 2);
        assert_eq!(delimiters[0].0, DelimiterKind::Paren);
        assert_eq!(delimiters[1].0, DelimiterKind::Paren);
        assert!(delimited.width > body.width);
        assert!(delimiters[0].1.height() >= body.height());
        assert!(delimited.warnings.is_empty());
    }

    #[test]
    fn invisible_delimiter_suppresses_geometry() {
        let layout = layout_math("\\left. x \\right|", &font(), 20.0);
        let delimiter_count = layout
            .elements
            .iter()
            .filter(|element| matches!(element, MathElement::Delimiter { .. }))
            .count();

        assert_eq!(delimiter_count, 1);
        assert!(layout.warnings.is_empty());
    }

    #[test]
    fn missing_right_delimiter_warns_and_preserves_body() {
        let layout = layout_math("\\left(x+1", &font(), 20.0);

        assert!(layout.elements.iter().any(|element| {
            matches!(
                element,
                MathElement::Delimiter {
                    kind: DelimiterKind::Paren,
                    ..
                }
            )
        }));
        assert!(
            layout
                .warnings
                .iter()
                .any(|w| w.reason == MathTextWarningReason::MissingRightDelimiter)
        );
    }

    #[test]
    fn large_operators_use_larger_geometry_and_take_scripts() {
        let plain = layout_math("∑", &font(), 24.0);
        let sum = layout_math("\\sum_{i=0}^{n}", &font(), 24.0);
        let large = sum.elements.iter().find_map(|element| match element {
            MathElement::LargeOperator { kind, path } => Some((*kind, path.get_extents())),
            _ => None,
        });
        let large = large.expect("sum should emit large-operator geometry");

        assert_eq!(large.0, LargeOperatorKind::Sum);
        assert!(large.1.height() > plain.height());
        assert!(sum.width > plain.width);
        assert!(sum.ascent > plain.ascent);
        assert!(sum.descent > plain.descent);
        assert!(sum.warnings.is_empty());
    }

    #[test]
    fn translated_layout_moves_geometry_without_changing_metrics() {
        let layout = layout_math("\\frac{\\vec{x}}{y}", &font(), 24.0);
        let before = layout.elements[0].path().get_extents();
        let shifted = layout.translated(12.0, -3.0);
        let after = shifted.elements[0].path().get_extents();

        assert_eq!(shifted.width, layout.width);
        assert_eq!(shifted.ascent, layout.ascent);
        assert_eq!(shifted.descent, layout.descent);
        assert!((after.xmin() - (before.xmin() + 12.0)).abs() < 1e-9);
        assert!((after.ymin() - (before.ymin() - 3.0)).abs() < 1e-9);
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
