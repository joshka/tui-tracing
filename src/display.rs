use std::iter;

use itertools::{Itertools, Position};
use ratatui::{
    style::{Color, Modifier},
    text::{Line, Text, ToLine, ToSpan, ToText},
};
use ratatui_macros::{line, span};

use crate::storage::{EventRecord, Level, SpanRecord};

impl ToLine for SpanRecord {
    fn to_line(&self) -> Line {
        let timing = self.timing;
        let busy_percentage = timing
            .busy_duration()
            .as_nanos()
            .checked_div(timing.total_duration().as_nanos())
            .unwrap_or_default();
        line![
            span!(Modifier::DIM; "{} ", self.start_time.format("%H:%M:%S")),
            self.level.to_span(),
            span!(" "),
            span!(Modifier::DIM; "{}::{}", self.target, self.name),
            span!(Modifier::DIM; " [Busy:"),
            span!(Modifier::DIM | Modifier::BOLD; "{:>8.2?}", timing.busy_duration()),
            span!(Modifier::DIM; "("),
            span!(Modifier::DIM | Modifier::BOLD; "{busy_percentage:.2}"),
            span!(Modifier::DIM; "%), Idle:"),
            span!(Modifier::DIM | Modifier::BOLD; "{:>8.2?}",  timing.idle_duration()),
            span!(Modifier::DIM; ", Total:"),
            span!(Modifier::DIM | Modifier::BOLD; "{:>8.2?}", timing.total_duration()),
        ]
    }
}

impl ToText for SpanRecord {
    fn to_text(&self) -> Text {
        let span_line = self.to_line();
        let event_lines = self
            .events
            .iter()
            .rev()
            .take(4) // todo: make this configurable and based on some sort of retention policy instead of a fixed number
            .rev()
            .map(ToLine::to_line)
            .with_position()
            .map(|(pos, mut line)| {
                let symbol = if matches!(pos, Position::Last | Position::Only) {
                    " └─"
                } else {
                    " ├─"
                };
                line.spans.insert(3, symbol.into());
                line
            });
        Text::from_iter(iter::once(span_line).chain(event_lines))
    }
}

impl ToLine for EventRecord {
    fn to_line(&self) -> Line {
        let message = self.fields["message"].clone();
        let fields = self
            .fields
            .iter()
            .filter(|(k, _)| *k != "message")
            .map(|(k, v)| format!("{}: {}", k, v))
            .join(", ");
        let mut line = line![
            span!(Modifier::DIM; "{}", self.time.format("%H:%M:%S")),
            " ",
            self.level.to_span(),
            span!(" {message}")
        ];
        if !fields.is_empty() {
            line.push_span(span!(Modifier::DIM | Modifier::ITALIC; " {fields}"));
        }
        line
    }
}

impl ToSpan for Level {
    fn to_span(&self) -> ratatui::text::Span {
        span!(self.color(); "{:5}", self.0)
    }
}

impl Level {
    fn color(&self) -> Color {
        match self.0 {
            tracing::Level::TRACE => Color::Magenta,
            tracing::Level::DEBUG => Color::Blue,
            tracing::Level::INFO => Color::Green,
            tracing::Level::WARN => Color::Yellow,
            tracing::Level::ERROR => Color::Red,
        }
    }
}
