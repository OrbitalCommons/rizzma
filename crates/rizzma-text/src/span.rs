//! Text span classification for plain text, math, and raw TeX.
//!
//! This module is deliberately independent of rendering. It preserves the exact
//! source slice for every math/raw-TeX span, exposes the inner content for later
//! layout engines, and defines a small fallback-warning contract that renderers
//! can use when they cannot honor a span natively. Delimiters describe math
//! mode, not render ownership: `$...$`, `$$...$$`, `\(...\)`, and `\[...\]`
//! all parse to [`TextSpanKind::Math`]. [`TextSpanKind::RawTex`] is reserved
//! for explicit TeX passthrough spans from higher-level APIs.

use std::ops::Range;

/// Whether a math-like span is inline or display-style.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MathMode {
    /// Inline math, such as `$x$` or `\(x\)`.
    Inline,
    /// Display math, such as `$$x$$` or `\[x\]`.
    Display,
}

/// The semantic class of a parsed text span.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextSpanKind {
    /// Ordinary text rendered with the normal text path.
    Plain,
    /// Math content that the portable mathtext engine should try to own.
    Math(MathMode),
    /// Raw TeX preserved exactly for frontend passthrough or optional TeX export.
    RawTex(MathMode),
}

/// A classified slice of a source text string.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextSpan {
    kind: TextSpanKind,
    source: String,
    content: String,
    source_range: Range<usize>,
}

impl TextSpan {
    fn new(kind: TextSpanKind, source: &str, content: &str, source_range: Range<usize>) -> Self {
        Self {
            kind,
            source: source.to_owned(),
            content: content.to_owned(),
            source_range,
        }
    }

    /// Returns this span's semantic class.
    #[must_use]
    pub fn kind(&self) -> TextSpanKind {
        self.kind
    }

    /// Returns the exact source text for this span, including delimiters.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the inner text for math/raw-TeX spans, or the plain text itself
    /// for [`TextSpanKind::Plain`].
    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Returns this span's byte range in the original source string.
    #[must_use]
    pub fn source_range(&self) -> Range<usize> {
        self.source_range.clone()
    }

    /// Returns `true` if this span requires mathtext or raw-TeX handling.
    #[must_use]
    pub fn is_special(&self) -> bool {
        !matches!(self.kind, TextSpanKind::Plain)
    }
}

/// A source string split into renderable spans.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextRun {
    source: String,
    spans: Vec<TextSpan>,
}

impl TextRun {
    /// Parses `source` into plain and math spans.
    ///
    /// Classification rules are intentionally small:
    ///
    /// - `$...$` and `\(...\)` become inline [`TextSpanKind::Math`].
    /// - `$$...$$` and `\[...\]` become display [`TextSpanKind::Math`].
    /// - escaped dollars (`\$`) and unclosed delimiters remain plain text.
    #[must_use]
    pub fn parse(source: &str) -> Self {
        let mut spans = Vec::new();
        let mut plain_start = 0;
        let mut i = 0;

        while i < source.len() {
            if starts_unescaped(source, i, "$$") {
                if let Some(close) = find_unescaped(source, i + 2, "$$") {
                    push_plain(&mut spans, source, plain_start, i);
                    let end = close + 2;
                    spans.push(TextSpan::new(
                        TextSpanKind::Math(MathMode::Display),
                        &source[i..end],
                        &source[i + 2..close],
                        i..end,
                    ));
                    i = end;
                    plain_start = i;
                    continue;
                }
                i += 2;
                continue;
            } else if starts_unescaped(source, i, "$") {
                if let Some(close) = find_unescaped(source, i + 1, "$") {
                    let end = close + 1;
                    push_plain(&mut spans, source, plain_start, i);
                    spans.push(TextSpan::new(
                        TextSpanKind::Math(MathMode::Inline),
                        &source[i..end],
                        &source[i + 1..close],
                        i..end,
                    ));
                    i = end;
                    plain_start = i;
                    continue;
                }
            } else if source[i..].starts_with("\\(") {
                if let Some(close) = source[i + 2..].find("\\)") {
                    let close = i + 2 + close;
                    let end = close + 2;
                    push_plain(&mut spans, source, plain_start, i);
                    spans.push(TextSpan::new(
                        TextSpanKind::Math(MathMode::Inline),
                        &source[i..end],
                        &source[i + 2..close],
                        i..end,
                    ));
                    i = end;
                    plain_start = i;
                    continue;
                }
            } else if source[i..].starts_with("\\[")
                && let Some(close) = source[i + 2..].find("\\]")
            {
                let close = i + 2 + close;
                let end = close + 2;
                push_plain(&mut spans, source, plain_start, i);
                spans.push(TextSpan::new(
                    TextSpanKind::Math(MathMode::Display),
                    &source[i..end],
                    &source[i + 2..close],
                    i..end,
                ));
                i = end;
                plain_start = i;
                continue;
            }

            i += next_char_len(source, i);
        }

        push_plain(&mut spans, source, plain_start, source.len());

        Self {
            source: source.to_owned(),
            spans,
        }
    }

    /// Returns the original source string.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the classified spans in source order.
    #[must_use]
    pub fn spans(&self) -> &[TextSpan] {
        &self.spans
    }

    /// Returns fallback warnings for spans unsupported by `capabilities`.
    #[must_use]
    pub fn fallback_warnings(
        &self,
        capabilities: TextRenderCapabilities,
    ) -> Vec<TextFallbackWarning> {
        self.spans
            .iter()
            .filter_map(|span| match span.kind() {
                TextSpanKind::Plain => None,
                TextSpanKind::Math(_) if !capabilities.mathtext => Some(TextFallbackWarning {
                    span: span.clone(),
                    reason: TextFallbackReason::MathtextUnsupported,
                    action: TextFallbackAction::DrawSourceAsPlainText,
                }),
                TextSpanKind::RawTex(_) if !capabilities.raw_tex => Some(TextFallbackWarning {
                    span: span.clone(),
                    reason: TextFallbackReason::RawTexUnsupported,
                    action: TextFallbackAction::DrawSourceAsPlainText,
                }),
                _ => None,
            })
            .collect()
    }
}

/// Span types that a renderer or export target can honor directly.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TextRenderCapabilities {
    /// Whether portable mathtext spans can be laid out deterministically.
    pub mathtext: bool,
    /// Whether raw TeX spans can be preserved or typeset by the target.
    pub raw_tex: bool,
}

impl TextRenderCapabilities {
    /// Capabilities of the current glyph-path text fallback.
    #[must_use]
    pub fn plain_text_only() -> Self {
        Self::default()
    }

    /// Capabilities for frontend targets that can pass raw TeX to the host.
    #[must_use]
    pub fn raw_tex_passthrough() -> Self {
        Self {
            raw_tex: true,
            ..Self::default()
        }
    }
}

/// Why a span was downgraded instead of rendered with its native semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextFallbackReason {
    /// No portable mathtext engine is available for a math span.
    MathtextUnsupported,
    /// The target cannot preserve or typeset a raw TeX span.
    RawTexUnsupported,
    /// An optional external TeX backend was requested but unavailable.
    ExternalTexUnavailable,
}

/// The concrete fallback chosen for an unsupported span.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextFallbackAction {
    /// Draw the exact source text, including delimiters, as ordinary text.
    DrawSourceAsPlainText,
    /// Preserve the raw source for host-side handling.
    PreserveSource,
}

/// A structured warning emitted when span semantics are downgraded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextFallbackWarning {
    /// The span whose native rendering was unavailable.
    pub span: TextSpan,
    /// Why the fallback was needed.
    pub reason: TextFallbackReason,
    /// Which fallback behavior should be used.
    pub action: TextFallbackAction,
}

fn push_plain(spans: &mut Vec<TextSpan>, source: &str, start: usize, end: usize) {
    if start < end {
        spans.push(TextSpan::new(
            TextSpanKind::Plain,
            &source[start..end],
            &source[start..end],
            start..end,
        ));
    }
}

fn starts_unescaped(source: &str, index: usize, needle: &str) -> bool {
    source[index..].starts_with(needle) && !is_escaped(source, index)
}

fn find_unescaped(source: &str, start: usize, needle: &str) -> Option<usize> {
    let mut search_start = start;
    while let Some(offset) = source[search_start..].find(needle) {
        let index = search_start + offset;
        if !is_escaped(source, index) {
            return Some(index);
        }
        search_start = index + needle.len();
    }
    None
}

fn is_escaped(source: &str, index: usize) -> bool {
    let bytes = source.as_bytes();
    let mut slash_count = 0;
    let mut i = index;
    while i > 0 && bytes[i - 1] == b'\\' {
        slash_count += 1;
        i -= 1;
    }
    slash_count % 2 == 1
}

fn next_char_len(source: &str, index: usize) -> usize {
    source[index..].chars().next().map_or(1, char::len_utf8)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_text_stays_plain() {
        let run = TextRun::parse("plain label");
        assert_eq!(run.source(), "plain label");
        assert_eq!(run.spans().len(), 1);
        assert_eq!(run.spans()[0].kind(), TextSpanKind::Plain);
        assert_eq!(run.spans()[0].content(), "plain label");
        assert_eq!(run.spans()[0].source_range(), 0..11);
    }

    #[test]
    fn dollar_math_is_inline_mathtext() {
        let run = TextRun::parse("speed $v^2$ now");
        let spans = run.spans();
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[0].content(), "speed ");
        assert_eq!(spans[1].kind(), TextSpanKind::Math(MathMode::Inline));
        assert_eq!(spans[1].source(), "$v^2$");
        assert_eq!(spans[1].content(), "v^2");
        assert_eq!(spans[2].content(), " now");
    }

    #[test]
    fn display_dollars_are_display_math() {
        let run = TextRun::parse("$$\\int x dx$$");
        let span = &run.spans()[0];
        assert_eq!(span.kind(), TextSpanKind::Math(MathMode::Display));
        assert_eq!(span.source(), "$$\\int x dx$$");
        assert_eq!(span.content(), "\\int x dx");
    }

    #[test]
    fn slash_delimiters_are_math() {
        let run = TextRun::parse("a \\(x+y\\) b \\[z\\]");
        let spans = run.spans();
        assert_eq!(spans.len(), 4);
        assert_eq!(spans[1].kind(), TextSpanKind::Math(MathMode::Inline));
        assert_eq!(spans[1].source(), "\\(x+y\\)");
        assert_eq!(spans[1].content(), "x+y");
        assert_eq!(spans[3].kind(), TextSpanKind::Math(MathMode::Display));
        assert_eq!(spans[3].source(), "\\[z\\]");
        assert_eq!(spans[3].content(), "z");
    }

    #[test]
    fn empty_display_math_is_preserved() {
        let run = TextRun::parse("before $$$$ after");
        let spans = run.spans();
        assert_eq!(spans.len(), 3);
        assert_eq!(spans[1].kind(), TextSpanKind::Math(MathMode::Display));
        assert_eq!(spans[1].source(), "$$$$");
        assert_eq!(spans[1].content(), "");
    }

    #[test]
    fn mixed_plain_and_math_delimiters_keep_order() {
        let run = TextRun::parse("plain $x$ plus \\(y\\) and \\[z\\]");
        let spans = run.spans();
        assert_eq!(spans.len(), 6);
        assert_eq!(spans[0].kind(), TextSpanKind::Plain);
        assert_eq!(spans[0].content(), "plain ");
        assert_eq!(spans[1].kind(), TextSpanKind::Math(MathMode::Inline));
        assert_eq!(spans[1].source(), "$x$");
        assert_eq!(spans[2].content(), " plus ");
        assert_eq!(spans[3].kind(), TextSpanKind::Math(MathMode::Inline));
        assert_eq!(spans[3].source(), "\\(y\\)");
        assert_eq!(spans[4].content(), " and ");
        assert_eq!(spans[5].kind(), TextSpanKind::Math(MathMode::Display));
        assert_eq!(spans[5].source(), "\\[z\\]");
    }

    #[test]
    fn escaped_and_unclosed_delimiters_remain_plain() {
        let run = TextRun::parse(r"cost \$5 and $unfinished");
        assert_eq!(run.spans().len(), 1);
        assert_eq!(run.spans()[0].kind(), TextSpanKind::Plain);
        assert_eq!(run.spans()[0].source(), r"cost \$5 and $unfinished");

        let run = TextRun::parse(r"cost $5 and \[unfinished");
        assert_eq!(run.spans().len(), 1);
        assert_eq!(run.spans()[0].kind(), TextSpanKind::Plain);
        assert_eq!(run.spans()[0].source(), r"cost $5 and \[unfinished");

        let run = TextRun::parse(r"$$unfinished $still plain");
        assert_eq!(run.spans().len(), 1);
        assert_eq!(run.spans()[0].kind(), TextSpanKind::Plain);
        assert_eq!(run.spans()[0].source(), r"$$unfinished $still plain");
    }

    #[test]
    fn unicode_source_ranges_are_byte_ranges() {
        let run = TextRun::parse("α $β$");
        let spans = run.spans();
        assert_eq!(spans[0].source_range(), 0..3);
        assert_eq!(spans[1].source_range(), 3..7);
        assert_eq!(spans[1].content(), "β");
    }

    #[test]
    fn fallback_warnings_respect_capabilities() {
        let run = TextRun::parse("a $x$ \\(y\\)");
        let warnings = run.fallback_warnings(TextRenderCapabilities::plain_text_only());
        assert_eq!(warnings.len(), 2);
        assert_eq!(warnings[0].reason, TextFallbackReason::MathtextUnsupported);
        assert_eq!(warnings[0].span.source(), "$x$");
        assert_eq!(warnings[1].reason, TextFallbackReason::MathtextUnsupported);
        assert_eq!(warnings[1].span.source(), "\\(y\\)");

        let warnings = run.fallback_warnings(TextRenderCapabilities::raw_tex_passthrough());
        assert_eq!(warnings.len(), 2);
        assert_eq!(warnings[0].span.source(), "$x$");
        assert_eq!(warnings[1].span.source(), "\\(y\\)");
    }

    #[test]
    fn raw_tex_fallback_warning_contract_is_distinct() {
        let run = TextRun {
            source: "\\input{external}".to_owned(),
            spans: vec![TextSpan::new(
                TextSpanKind::RawTex(MathMode::Display),
                "\\input{external}",
                "\\input{external}",
                0..16,
            )],
        };

        let warnings = run.fallback_warnings(TextRenderCapabilities::plain_text_only());
        assert_eq!(warnings.len(), 1);
        assert_eq!(warnings[0].reason, TextFallbackReason::RawTexUnsupported);
        assert_eq!(
            warnings[0].action,
            TextFallbackAction::DrawSourceAsPlainText
        );
        assert_eq!(warnings[0].span.source(), "\\input{external}");

        let warnings = run.fallback_warnings(TextRenderCapabilities::raw_tex_passthrough());
        assert!(warnings.is_empty());
    }
}
