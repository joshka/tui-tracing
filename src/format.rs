//! Formatting captured records into Ratatui text.
//!
//! This module owns the default `tracing_subscriber::fmt`-inspired presentation.
//! It is deliberately event-stream oriented; span hierarchy is rendered as compact
//! inline context.

use itertools::Itertools;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};

use crate::field::{FieldMap, FieldValue};
use crate::record::{EventRecord, Level, SpanRecord};
use crate::store::TraceSnapshot;

const OVERFLOW_MARKER: &str = "...";

/// Options for rendering captured trace records.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatOptions {
    /// Timestamp format used for each compact event row.
    pub timestamp_format: TimestampFormat,

    /// Whether to render compact span context after the event target.
    ///
    /// This is disabled by default so compact rows prioritize the event target,
    /// message, and fields. Selected-event detail still exposes the full span
    /// stack.
    pub show_span_context: bool,

    /// Whether to render event target before the event message.
    pub show_target: bool,

    /// Whether to render source file and line when present.
    pub show_location: bool,
}

impl Default for FormatOptions {
    fn default() -> Self {
        Self {
            timestamp_format: TimestampFormat::default(),
            show_span_context: false,
            show_target: true,
            show_location: false,
        }
    }
}

/// Timestamp format used for compact event rows.
///
/// Selected-event detail always uses a full RFC 3339 timestamp. This type only
/// controls the dense event stream where horizontal space is limited.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum TimestampFormat {
    /// Local wall-clock time with millisecond precision, such as `12:04:31.123`.
    #[default]
    ShortLocal,

    /// Full RFC 3339 timestamp with microsecond precision and local offset.
    Rfc3339,

    /// Custom [`chrono`] format string.
    ///
    /// Invalid format strings do not fail rendering; they are rendered according
    /// to `chrono`'s formatting behavior.
    Custom(String),
}

impl TimestampFormat {
    fn format(&self, timestamp: chrono::DateTime<chrono::Local>) -> String {
        match self {
            Self::ShortLocal => timestamp.format("%H:%M:%S%.3f").to_string(),
            Self::Rfc3339 => timestamp.format("%Y-%m-%dT%H:%M:%S%.6f%:z").to_string(),
            Self::Custom(format) => timestamp.format(format).to_string(),
        }
    }
}

pub(crate) fn event_line(
    event: &EventRecord,
    snapshot: &TraceSnapshot,
    options: &FormatOptions,
    max_width: u16,
) -> Line<'static> {
    let mut pieces = row_pieces(event, snapshot, options);
    fit_pieces(&mut pieces, usize::from(max_width));

    let mut spans = Vec::with_capacity(pieces.len());
    for piece in pieces {
        spans.push(Span::styled(piece.text, piece.style));
    }
    Line::from(spans)
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum RowPieceKind {
    Fixed,
    Target,
    Context,
    Location,
    Fields,
}

struct RowPiece {
    text: String,
    style: Style,
    kind: RowPieceKind,
}

impl RowPiece {
    fn raw(text: impl Into<String>, kind: RowPieceKind) -> Self {
        Self {
            text: text.into(),
            style: Style::default(),
            kind,
        }
    }

    fn styled(text: impl Into<String>, style: Style, kind: RowPieceKind) -> Self {
        Self {
            text: text.into(),
            style,
            kind,
        }
    }
}

fn row_pieces(
    event: &EventRecord,
    snapshot: &TraceSnapshot,
    options: &FormatOptions,
) -> Vec<RowPiece> {
    let mut pieces = vec![
        RowPiece::styled(
            options.timestamp_format.format(event.timestamp),
            Style::default().add_modifier(Modifier::DIM),
            RowPieceKind::Fixed,
        ),
        RowPiece::raw(" ", RowPieceKind::Fixed),
        RowPiece::styled(
            level_text(event.level),
            level_style(event.level),
            RowPieceKind::Fixed,
        ),
    ];

    if options.show_target {
        pieces.push(RowPiece::styled(
            format!("{}: ", event.target),
            Style::default().add_modifier(Modifier::DIM),
            RowPieceKind::Target,
        ));
    }

    if options.show_span_context {
        let context = event
            .span_stack
            .iter()
            .filter_map(|span_id| snapshot.spans.get(span_id))
            .map(format_span_context)
            .join(": ");
        if !context.is_empty() {
            pieces.push(RowPiece::styled(
                format!("{context}: "),
                Style::default().add_modifier(Modifier::BOLD),
                RowPieceKind::Context,
            ));
        }
    }

    if options.show_location {
        if let Some(location) = format_location(event) {
            pieces.push(RowPiece::styled(
                location,
                Style::default().add_modifier(Modifier::DIM),
                RowPieceKind::Location,
            ));
        }
    }

    let fields = format_event_fields(&event.fields);
    if !fields.is_empty() {
        pieces.push(RowPiece::raw(fields, RowPieceKind::Fields));
    }

    pieces
}

fn fit_pieces(pieces: &mut [RowPiece], max_width: usize) {
    if max_width == 0 {
        for piece in pieces {
            piece.text.clear();
        }
        return;
    }

    let width = pieces_width(pieces);
    if width <= max_width {
        return;
    }

    let fixed_width = pieces
        .iter()
        .filter(|piece| matches!(piece.kind, RowPieceKind::Fixed))
        .map(|piece| text_width(&piece.text))
        .sum::<usize>();
    if fixed_width >= max_width {
        let mut used = 0;
        for piece in pieces {
            let remaining = max_width.saturating_sub(used);
            piece.text = truncate_end(&piece.text, remaining);
            used += text_width(&piece.text);
            if used >= max_width {
                break;
            }
        }
        return;
    }

    for kind in [
        RowPieceKind::Fields,
        RowPieceKind::Target,
        RowPieceKind::Context,
        RowPieceKind::Location,
    ] {
        while pieces_width(pieces) > max_width {
            let excess = pieces_width(pieces).saturating_sub(max_width);
            let Some(piece) = pieces.iter_mut().rev().find(|piece| piece.kind == kind) else {
                break;
            };
            let width = text_width(&piece.text);
            let minimum_width = minimum_piece_width(piece.kind);
            if width <= minimum_width {
                break;
            }

            let target_width = width.saturating_sub(excess).max(minimum_width);
            piece.text = match piece.kind {
                RowPieceKind::Context => truncate_start(&piece.text, target_width),
                _ => truncate_end(&piece.text, target_width),
            };
        }
    }

    if pieces_width(pieces) > max_width {
        let line = pieces
            .iter()
            .map(|piece| piece.text.as_str())
            .collect::<String>();
        pieces[0].text = truncate_end(&line, max_width);
        for piece in &mut pieces[1..] {
            piece.text.clear();
        }
    }
}

fn pieces_width(pieces: &[RowPiece]) -> usize {
    pieces.iter().map(|piece| text_width(&piece.text)).sum()
}

fn minimum_piece_width(kind: RowPieceKind) -> usize {
    match kind {
        RowPieceKind::Fixed => 0,
        RowPieceKind::Target => 12,
        RowPieceKind::Context => 24,
        RowPieceKind::Location => 10,
        RowPieceKind::Fields => 18,
    }
}

fn format_event_fields(fields: &FieldMap) -> String {
    fields
        .iter()
        .map(|(name, value)| {
            if name == "message" {
                value.to_string()
            } else if let Some(raw_name) = name.strip_prefix("r#") {
                format!("{raw_name}={}", FmtFieldValue(value))
            } else {
                format!("{name}={}", FmtFieldValue(value))
            }
        })
        .join(" ")
}

fn format_location(event: &EventRecord) -> Option<String> {
    let file = event.file.as_deref()?;

    if let Some(line) = event.line {
        return Some(format!("{file}:{line}: "));
    }

    Some(format!("{file}: "))
}

fn format_span_context(span: &SpanRecord) -> String {
    if span.fields.is_empty() {
        return span.name.clone();
    }

    format!("{}{{{}}}", span.name, format_fields(&span.fields))
}

fn format_fields(fields: &FieldMap) -> String {
    fields
        .iter()
        .map(|(name, value)| {
            if let Some(raw_name) = name.strip_prefix("r#") {
                format!("{raw_name}={}", FmtFieldValue(value))
            } else {
                format!("{name}={}", FmtFieldValue(value))
            }
        })
        .join(" ")
}

struct FmtFieldValue<'a>(&'a FieldValue);

impl std::fmt::Display for FmtFieldValue<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            FieldValue::I64(value) => write!(f, "{value}"),
            FieldValue::U64(value) => write!(f, "{value}"),
            FieldValue::F64(value) => write!(f, "{value}"),
            FieldValue::Bool(value) => write!(f, "{value}"),
            FieldValue::Str(value) => write!(f, "{value:?}"),
            FieldValue::Error(value) | FieldValue::Debug(value) => f.write_str(value),
        }
    }
}

fn truncate_end(text: &str, max_width: usize) -> String {
    truncate(text, max_width, TruncateSide::End)
}

fn truncate_start(text: &str, max_width: usize) -> String {
    truncate(text, max_width, TruncateSide::Start)
}

enum TruncateSide {
    Start,
    End,
}

fn truncate(text: &str, max_width: usize, side: TruncateSide) -> String {
    if text_width(text) <= max_width {
        return text.to_owned();
    }

    if max_width <= OVERFLOW_MARKER.len() {
        return OVERFLOW_MARKER[..max_width].to_owned();
    }

    let keep = max_width - OVERFLOW_MARKER.len();
    match side {
        TruncateSide::Start => {
            let suffix = text
                .chars()
                .rev()
                .take(keep)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect::<String>();
            format!("{OVERFLOW_MARKER}{suffix}")
        }
        TruncateSide::End => {
            let prefix = text.chars().take(keep).collect::<String>();
            format!("{prefix}{OVERFLOW_MARKER}")
        }
    }
}

fn text_width(text: &str) -> usize {
    text.chars().count()
}

fn level_text(level: Level) -> String {
    format!("{:<5} ", level.0)
}

fn level_style(level: Level) -> Style {
    Style::default().fg(level_color(level))
}

fn level_color(level: Level) -> Color {
    match level.0 {
        tracing::Level::TRACE => Color::Magenta,
        tracing::Level::DEBUG => Color::Blue,
        tracing::Level::INFO => Color::Green,
        tracing::Level::WARN => Color::Yellow,
        tracing::Level::ERROR => Color::Red,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Local;

    use super::*;

    #[test]
    fn source_location_uses_file_and_line_when_present() {
        let event = event_with_location(Some("src/main.rs"), Some(42));

        assert_eq!(format_location(&event), Some("src/main.rs:42: ".to_owned()));
    }

    #[test]
    fn source_location_uses_file_without_line_when_line_is_missing() {
        let event = event_with_location(Some("src/main.rs"), None);

        assert_eq!(format_location(&event), Some("src/main.rs: ".to_owned()));
    }

    #[test]
    fn source_location_is_absent_without_file_metadata() {
        let event = event_with_location(None, Some(42));

        assert_eq!(format_location(&event), None);
    }

    fn event_with_location(file: Option<&str>, line: Option<u32>) -> EventRecord {
        EventRecord {
            id: 0,
            timestamp: Local::now(),
            level: Level(tracing::Level::INFO),
            target: "format_tests".to_owned(),
            module_path: None,
            file: file.map(str::to_owned),
            line,
            fields: FieldMap::default(),
            span_id: None,
            span_stack: Vec::new(),
        }
    }
}
