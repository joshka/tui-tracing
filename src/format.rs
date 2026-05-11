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

/// Options for rendering captured trace records.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormatOptions {
    /// Timestamp format used for each compact event row.
    pub timestamp_format: TimestampFormat,

    /// Whether to render compact span context after the event target.
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
            show_span_context: true,
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
) -> Line<'static> {
    let mut spans = vec![
        Span::styled(
            options.timestamp_format.format(event.timestamp),
            Style::default().add_modifier(Modifier::DIM),
        ),
        Span::raw(" "),
        level_span(event.level),
    ];

    push_target(&mut spans, event, options);
    push_context(&mut spans, event, snapshot, options);

    if options.show_location {
        push_location(&mut spans, event);
    }

    push_message_and_fields(&mut spans, event);

    Line::from(spans)
}

fn push_context(
    spans: &mut Vec<Span<'static>>,
    event: &EventRecord,
    snapshot: &TraceSnapshot,
    options: &FormatOptions,
) {
    if options.show_span_context && !event.span_stack.is_empty() {
        let mut pushed = false;
        for span in event
            .span_stack
            .iter()
            .filter_map(|span_id| snapshot.spans.get(span_id))
        {
            spans.push(Span::styled(
                format_span_context(span),
                Style::default().add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::styled(
                ":",
                Style::default().add_modifier(Modifier::DIM),
            ));
            pushed = true;
        }

        if pushed {
            spans.push(Span::raw(" "));
        }
    }
}

fn push_target(spans: &mut Vec<Span<'static>>, event: &EventRecord, options: &FormatOptions) {
    if options.show_target {
        spans.push(Span::styled(
            event.target.clone(),
            Style::default().add_modifier(Modifier::DIM),
        ));
        spans.push(Span::styled(
            ":",
            Style::default().add_modifier(Modifier::DIM),
        ));
        spans.push(Span::raw(" "));
    }
}

fn push_message_and_fields(spans: &mut Vec<Span<'static>>, event: &EventRecord) {
    let fields = event
        .fields
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
        .join(" ");

    if !fields.is_empty() {
        spans.push(Span::raw(fields));
    }
}

fn push_location(spans: &mut Vec<Span<'static>>, event: &EventRecord) {
    let Some(file) = event.file.as_deref() else {
        return;
    };

    spans.push(Span::styled(
        file.to_owned(),
        Style::default().add_modifier(Modifier::DIM),
    ));
    spans.push(Span::styled(
        ":",
        Style::default().add_modifier(Modifier::DIM),
    ));

    if let Some(line) = event.line {
        spans.push(Span::styled(
            line.to_string(),
            Style::default().add_modifier(Modifier::DIM),
        ));
        spans.push(Span::styled(
            ":",
            Style::default().add_modifier(Modifier::DIM),
        ));
    }

    spans.push(Span::raw(" "));
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

fn level_span(level: Level) -> Span<'static> {
    Span::styled(
        format!("{:<5} ", level.0),
        Style::default().fg(level_color(level)),
    )
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
