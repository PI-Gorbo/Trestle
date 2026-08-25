//! Turning compiler errors into editor markers.
//!
//! Two jobs live here:
//!
//! 1. **Position translation.** `miette::SourceSpan` is a *byte* offset into the source.
//!    Monaco wants a 1-based line and a 1-based column counted in UTF-16 code units. Get
//!    this wrong and every marker after the first non-ASCII character (say, in a string
//!    literal) drifts.
//!
//! 2. **Generic diagnostic extraction.** Every leaf error in `trestle` derives
//!    `miette::Diagnostic`, so we read them through the trait — `labels()`, `code()`,
//!    `help()`, `severity()`, `Display` — rather than matching on variants. That means a
//!    new `TypeCheckError` variant shows up in the editor with no change here.
//!
//! 3. **Graphical rendering.** The structured labels above drive Monaco's squiggles, but they
//!    are a reduction of what `miette` can actually draw. `render_graphically` runs the real
//!    `GraphicalReportHandler` — the same one the CLI uses — so the playground can show the
//!    caret art and source excerpt verbatim rather than an approximation reassembled in
//!    TypeScript. This is why every diagnostic is wrapped in a `Report` with the source
//!    attached: the leaf errors deliberately carry no `#[source_code]` (see
//!    `parse::build_program::BuildError`), so the excerpt has to be supplied at this boundary,
//!    exactly as `trestle::parse` and the conformance corpus already do.

use miette::{GraphicalReportHandler, GraphicalTheme, LabeledSpan, NamedSource, Report};

use crate::dto::{Diagnostic, Label, Phase, Severity};

/// The name `miette` prints above the source excerpt. The playground has no filename, and
/// programs are `.trsl`, so this is the honest stand-in.
const SOURCE_NAME: &str = "playground.trsl";

/// Wider than a terminal because the panel scrolls horizontally and a narrow wrap point makes
/// long type names unreadable.
const RENDER_WIDTH: usize = 100;

/// Byte offset -> (line, column) lookup for one source string.
pub struct LineIndex<'a> {
    source: &'a str,
    /// Byte offset of the first character of each line. Always starts with 0.
    line_starts: Vec<usize>,
}

impl<'a> LineIndex<'a> {
    pub fn new(source: &'a str) -> Self {
        let mut line_starts = vec![0];
        line_starts.extend(source.match_indices('\n').map(|(index, _)| index + 1));
        Self {
            source,
            line_starts,
        }
    }

    /// The source this index was built over. Needed to attach an excerpt to a `Report`.
    pub fn source(&self) -> &'a str {
        self.source
    }

    /// 1-based line and 1-based UTF-16 column for a byte offset.
    ///
    /// Offsets are clamped into range and down to a char boundary rather than panicking: a
    /// span from a half-finished compiler pass should degrade to a slightly-off marker, not
    /// take down the whole worker.
    pub fn position(&self, offset: usize) -> (u32, u32) {
        let offset = self.clamp_to_char_boundary(offset);

        // `partition_point` gives the count of line starts <= offset; minus one is the index
        // of the line containing it. Never zero, because `line_starts[0]` is always 0.
        let line = self.line_starts.partition_point(|&start| start <= offset) - 1;
        let line_start = self.line_starts[line];
        let column = self.source[line_start..offset].encode_utf16().count();

        (line as u32 + 1, column as u32 + 1)
    }

    fn clamp_to_char_boundary(&self, offset: usize) -> usize {
        let mut offset = offset.min(self.source.len());
        while offset > 0 && !self.source.is_char_boundary(offset) {
            offset -= 1;
        }
        offset
    }

    /// A `Label` spanning `offset..offset + length`.
    pub fn label(&self, message: Option<String>, offset: usize, length: usize) -> Label {
        let (start_line, start_column) = self.position(offset);
        let (end_line, end_column) = self.position(offset + length);

        Label {
            message,
            start_line,
            start_column,
            end_line,
            end_column,
            offset: offset as u32,
            length: length as u32,
        }
    }
}

/// Read any `miette::Diagnostic` into the wire format.
///
/// Takes the error **by value** so it can be moved into a `Report` and given the source text.
/// That is what makes `render` possible; the structured fields are read back out through the
/// same `Report`, which delegates every accessor to the error it wraps.
pub fn from_miette<E>(error: E, phase: Phase, index: &LineIndex<'_>) -> Diagnostic
where
    E: miette::Diagnostic + Send + Sync + 'static,
{
    let report = Report::new(error)
        .with_source_code(NamedSource::new(SOURCE_NAME, index.source().to_owned()));
    let diagnostic: &dyn miette::Diagnostic = report.as_ref();

    let labels = diagnostic
        .labels()
        .map(|labels| labels.map(|label| label_from(&label, index)).collect())
        .unwrap_or_default();

    Diagnostic {
        phase,
        severity: severity_from(diagnostic.severity()),
        code: diagnostic.code().map(|code| code.to_string()),
        message: diagnostic.to_string(),
        help: diagnostic.help().map(|help| help.to_string()),
        labels,
        render: render_graphically(diagnostic),
    }
}

/// `miette`'s own rendering of a diagnostic, as plain text.
///
/// `unicode_nocolor` rather than a coloured theme: the box-drawing and caret layout are the
/// valuable part, and emitting ANSI escapes would mean parsing them back out in the browser.
/// The panel styles the block with CSS instead. Links are disabled explicitly — an OSC-8
/// hyperlink is meaningless in a `<pre>`.
fn render_graphically(diagnostic: &dyn miette::Diagnostic) -> String {
    let mut out = String::new();

    let handler = GraphicalReportHandler::new_themed(GraphicalTheme::unicode_nocolor())
        .with_width(RENDER_WIDTH)
        .with_wrap_lines(false)
        .with_links(false);

    // Rendering is pure string building into a `String`, whose `fmt::Write` is infallible.
    // A failure here must not cost the caller its diagnostic, so it degrades to no excerpt.
    let _ = handler.render_report(&mut out, diagnostic);

    out
}

fn label_from(label: &LabeledSpan, index: &LineIndex<'_>) -> Label {
    index.label(
        label.label().map(str::to_owned),
        label.offset(),
        // A zero-length span (an error pinned at a point rather than over a range) would be
        // invisible in the editor, so widen it to one character.
        label.len().max(1),
    )
}

fn severity_from(severity: Option<miette::Severity>) -> Severity {
    match severity {
        Some(miette::Severity::Warning) => Severity::Warning,
        Some(miette::Severity::Advice) => Severity::Advice,
        // `miette` treats "unset" as an error, and so do we.
        Some(miette::Severity::Error) | None => Severity::Error,
    }
}

#[cfg(test)]
mod tests {
    use super::LineIndex;

    #[test]
    fn first_character_is_line_one_column_one() {
        let index = LineIndex::new("let x = 1");
        assert_eq!(index.position(0), (1, 1));
    }

    #[test]
    fn offsets_resolve_to_the_right_line() {
        let source = "let a = 1\nlet b = 2\nb";
        let index = LineIndex::new(source);

        assert_eq!(index.position(source.find("let b").unwrap()), (2, 1));
        assert_eq!(index.position(source.rfind('b').unwrap()), (3, 1));
    }

    #[test]
    fn columns_count_utf16_units_not_bytes() {
        // The emoji is 4 UTF-8 bytes but 2 UTF-16 code units; a byte-based column would put
        // the marker two characters too far right.
        let source = "let s = \"🎈\"\nlet y = 2";
        let index = LineIndex::new(source);
        let offset = source.find("let y").unwrap();

        assert_eq!(index.position(offset), (2, 1));
        assert_eq!(index.position(source.find('🎈').unwrap()), (1, 10));
    }

    #[test]
    fn out_of_range_offsets_clamp_instead_of_panicking() {
        let index = LineIndex::new("let x = 1");
        assert_eq!(index.position(9_999), (1, 10));
    }

    #[test]
    fn offsets_inside_a_character_clamp_down_to_its_start() {
        let source = "\"🎈\"";
        let index = LineIndex::new(source);
        // Offset 2 is halfway through the emoji.
        assert_eq!(index.position(2), index.position(1));
    }
}
