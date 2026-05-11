//! Captured [`tracing`] records.
//!
//! Records are immutable snapshots of tracing activity. The capture layer writes
//! them into [`crate::TraceStore`], while filters and widgets read cloned snapshots
//! without holding store locks during formatting or rendering.
//!
//! # Common Workflow
//!
//! Applications receive records through [`crate::TraceStore::snapshot`],
//! [`crate::TraceViewer::selected_event`], or [`crate::TraceViewer::selected_detail`].
//! Use these types directly when building custom views, exports, or tests.
//!
//! # Related Modules
//!
//! - [`crate::layer`] creates records from [`tracing`] callbacks.
//! - [`crate::store`] retains records and returns snapshots.
//! - [`crate::viewer`] renders records into [`ratatui`] widgets.

use chrono::{DateTime, Local};
use tracing::{Metadata, span};

use crate::Timing;
use crate::field::FieldMap;

/// Stable sequence number assigned when an event is captured.
///
/// Event ids are monotonically increasing within one [`crate::TraceStore`]. They
/// are not globally unique and reset when a store is created.
pub type EventId = u64;

/// Numeric [`tracing`] span identifier used inside the store.
///
/// Span ids come from [`tracing`]. They are useful for relating events to retained
/// spans inside a single snapshot.
pub type SpanId = u64;

/// A captured tracing event.
///
/// Events are the primary display unit. Span hierarchy is preserved as context, but
/// the default viewer renders an event stream rather than a tree.
///
/// Event records are cloned into [`crate::TraceSnapshot`] values. Mutating a cloned
/// record does not affect the store.
#[derive(Clone, Debug)]
pub struct EventRecord {
    /// Monotonic store-local event sequence.
    ///
    /// The id is assigned before retention is applied. Evicted or dropped events
    /// leave gaps in later snapshots.
    pub id: EventId,

    /// Wall-clock time at capture.
    pub timestamp: DateTime<Local>,

    /// Event level.
    pub level: Level,

    /// Event target from tracing metadata.
    ///
    /// This value comes from [`tracing`] metadata and is used by compact row
    /// formatting and [`crate::TraceFilter::with_target`].
    pub target: String,

    /// Module path reported by tracing metadata.
    pub module_path: Option<String>,

    /// Source file reported by tracing metadata.
    pub file: Option<String>,

    /// Source line reported by tracing metadata.
    pub line: Option<u32>,

    /// Structured event fields.
    ///
    /// The `message` field, when present, is promoted by viewer formatting but
    /// remains in this map.
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
///
/// Span records are retained independently from the event FIFO. They are cleared
/// only when [`crate::TraceStore::clear`] is called.
#[derive(Clone, Debug)]
pub struct SpanRecord {
    /// Numeric span identifier assigned by [`tracing`].
    pub id: SpanId,

    /// Parent span identifier, if this span was created inside another span.
    pub parent_id: Option<SpanId>,

    /// Wall-clock time at span creation.
    pub start_time: DateTime<Local>,

    /// Wall-clock time when the span closed.
    ///
    /// `None` means the span was still open at the time of the snapshot or the
    /// close event had not reached the store.
    pub close_time: Option<DateTime<Local>>,

    /// Latest timing data recorded for the span.
    ///
    /// This is present only when [`crate::TimingLayer`] is installed in the same
    /// subscriber stack before `TraceLayer` observes timing updates.
    pub timing: Option<Timing>,

    /// Span level.
    pub level: Level,

    /// Span name.
    pub name: String,

    /// Span target from tracing metadata.
    pub target: String,

    /// Module path reported by tracing metadata.
    pub module_path: Option<String>,

    /// Source file reported by tracing metadata.
    pub file: Option<String>,

    /// Source line reported by tracing metadata.
    pub line: Option<u32>,

    /// Structured span fields.
    ///
    /// These are fields recorded when the span was created. Later field updates
    /// are not currently captured as a separate public event stream.
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
///
/// This wrapper preserves [`tracing`] level ordering while allowing the crate to
/// derive traits and keep record fields simple. It converts to and from
/// [`tracing::Level`] and displays like the wrapped level.
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
