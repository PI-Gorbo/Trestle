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

use miette::LabeledSpan;

use crate::dto::{Diagnostic, Label, Phase, Severity};

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
pub fn from_miette(
    diagnostic: &dyn miette::Diagnostic,
    phase: Phase,
    index: &LineIndex<'_>,
) -> Diagnostic {
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
    }
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

/// A diagnostic we raise ourselves rather than one the compiler produced — currently only
/// used for the `EvalError` arm, which is uninhabited today.
pub fn synthetic(phase: Phase, code: &str, message: String, label: Label) -> Diagnostic {
    Diagnostic {
        phase,
        severity: Severity::Error,
        code: Some(code.to_owned()),
        message,
        help: None,
        labels: vec![label],
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
