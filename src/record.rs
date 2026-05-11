//! Captured tracing records.
//!
//! Records are immutable snapshots of tracing activity. The capture layer writes
//! them into [`crate::TraceStore`], while filters and widgets read cloned snapshots
//! without holding store locks during formatting or rendering.

use chrono::{DateTime, Local};
use tracing::{Metadata, span};

use crate::Timing;
use crate::field::FieldMap;

/// Stable sequence number assigned when an event is captured.
pub type EventId = u64;

/// Numeric tracing span identifier used inside the store.
pub type SpanId = u64;

/// A captured tracing event.
///
/// Events are the primary display unit. Span hierarchy is preserved as context, but
/// the default viewer renders an event stream rather than a tree.
#[derive(Clone, Debug)]
pub struct EventRecord {
    /// Monotonic store-local event sequence.
    pub id: EventId,

    /// Wall-clock time at capture.
    pub timestamp: DateTime<Local>,

    /// Event level.
    pub level: Level,

    /// Event target, usually the Rust module path.
    pub target: String,

    /// Module path reported by tracing metadata.
    pub module_path: Option<String>,

    /// Source file reported by tracing metadata.
    pub file: Option<String>,

    /// Source line reported by tracing metadata.
    pub line: Option<u32>,

    /// Structured event fields.
    pub fields: FieldMap,

    /// Innermost event span, if any.
    pub span_id: Option<SpanId>,

    /// Span stack from root to innermost span at event capture time.
    pub span_stack: Vec<SpanId>,
}

impl EventRecord {
    pub(crate) fn new(
        id: EventId,
        metadata: &Metadata<'_>,
        fields: FieldMap,
        span_stack: Vec<SpanId>,
    ) -> Self {
        Self {
            id,
            timestamp: Local::now(),
            level: (*metadata.level()).into(),
            target: metadata.target().to_owned(),
            module_path: metadata.module_path().map(str::to_owned),
            file: metadata.file().map(str::to_owned),
            line: metadata.line(),
            fields,
            span_id: span_stack.last().copied(),
            span_stack,
        }
    }
}

/// A captured tracing span.
///
/// Spans are stored for display context, filtering, lifecycle state, and optional
/// timing. The default widget shows them inline with events and leaves tree-oriented
/// presentation to future secondary views.
#[derive(Clone, Debug)]
pub struct SpanRecord {
    /// Numeric span identifier assigned by `tracing`.
    pub id: SpanId,

    /// Parent span identifier, if this span was created inside another span.
    pub parent_id: Option<SpanId>,

    /// Wall-clock time at span creation.
    pub start_time: DateTime<Local>,

    /// Wall-clock time when the span closed.
    pub close_time: Option<DateTime<Local>>,

    /// Latest timing data recorded for the span.
    pub timing: Option<Timing>,

    /// Span level.
    pub level: Level,

    /// Span name.
    pub name: String,

    /// Span target, usually the Rust module path.
    pub target: String,

    /// Module path reported by tracing metadata.
    pub module_path: Option<String>,

    /// Source file reported by tracing metadata.
    pub file: Option<String>,

    /// Source line reported by tracing metadata.
    pub line: Option<u32>,

    /// Structured span fields.
    pub fields: FieldMap,
}

impl SpanRecord {
    pub(crate) fn new(
        id: &span::Id,
        parent_id: Option<SpanId>,
        metadata: &Metadata<'_>,
        fields: FieldMap,
        timing: Option<Timing>,
    ) -> Self {
        Self {
            id: id.into_u64(),
            parent_id,
            start_time: Local::now(),
            close_time: None,
            timing,
            level: (*metadata.level()).into(),
            name: metadata.name().to_owned(),
            target: metadata.target().to_owned(),
            module_path: metadata.module_path().map(str::to_owned),
            file: metadata.file().map(str::to_owned),
            line: metadata.line(),
            fields,
        }
    }

    pub(crate) fn close(&mut self) {
        self.close_time = Some(Local::now());
    }
}

/// Tracing level captured as a small value type.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Ord, PartialOrd)]
pub struct Level(pub tracing::Level);

impl From<tracing::Level> for Level {
    fn from(level: tracing::Level) -> Self {
        Self(level)
    }
}

impl From<Level> for tracing::Level {
    fn from(level: Level) -> Self {
        level.0
    }
}

impl std::fmt::Display for Level {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
