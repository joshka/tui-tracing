use std::sync::Arc;

use chrono::{DateTime, Duration, Local};
use indexmap::IndexMap;
use parking_lot::RwLock;
use tracing::field::Visit;
use tracing_subscriber::{
    field::VisitOutput,
    registry::{LookupSpan, SpanRef},
};

use crate::Timing;

#[derive(Debug, Clone)]
pub struct TraceStore {
    pub(crate) spans: Arc<RwLock<IndexMap<u64, SpanRecord>>>,
}

impl Default for TraceStore {
    fn default() -> Self {
        let mut map = IndexMap::new();
        // Insert a root span to ensure there is always at least one span in the map.
        map.insert(
            0,
            SpanRecord {
                start_time: Local::now(),
                close_time: None,
                timing: Timing::default(),
                level: Level(tracing::Level::INFO),
                name: "root".to_owned(),
                target: "root".to_owned(),
                events: Vec::new(),
            },
        );
        Self {
            spans: Arc::new(RwLock::new(map)),
        }
    }
}

impl TraceStore {
    pub fn spans(&self) -> Vec<SpanRecord> {
        let spans = self.spans.read();
        spans.values().cloned().collect()
    }

    pub fn insert_span(&self, id: u64, span: SpanRecord) {
        let mut spans = self.spans.write();
        spans.insert(id, span);
    }

    pub fn insert_event(&self, span_id: u64, event: EventRecord) {
        let mut spans = self.spans.write();
        if let Some(span) = spans.get_mut(&span_id) {
            span.events.push(event);
        }
    }

    pub fn close_span(&self, id: u64) {
        self.spans.write().get_mut(&id).unwrap().close();
    }

    pub fn remove_expired(&self, threshold: Duration) {
        let mut spans = self.spans.write();
        spans.retain(|_, span| {
            !span.close_time.is_some_and(|close_time| {
                Local::now().signed_duration_since(close_time) > threshold
            })
        });
    }

    pub(crate) fn update_timing(&self, into_u64: u64, timing: &Timing) {
        let mut spans = self.spans.write();
        if let Some(span) = spans.get_mut(&into_u64) {
            span.timing = timing.clone();
        }
    }
}

#[derive(Debug, Clone)]
pub struct SpanRecord {
    pub start_time: DateTime<Local>,
    pub close_time: Option<DateTime<Local>>,
    pub timing: Timing,
    pub level: Level,
    pub name: String,
    pub target: String,
    pub events: Vec<EventRecord>,
}

impl SpanRecord {
    fn close(&mut self) {
        self.close_time = Some(Local::now());
    }
}

impl<'a, R: LookupSpan<'a>> From<SpanRef<'a, R>> for SpanRecord {
    fn from(span: SpanRef<'a, R>) -> Self {
        let timing = span
            .extensions()
            .get::<Timing>()
            .cloned()
            .unwrap_or_default();
        Self {
            start_time: Local::now(),
            close_time: None,
            timing,
            level: span.metadata().level().to_owned().into(),
            name: span.metadata().name().to_owned(),
            target: span.metadata().target().to_owned(),
            events: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct EventRecord {
    pub(crate) time: DateTime<Local>,
    pub(crate) level: Level,
    pub(crate) fields: FieldMap,
}

impl From<&tracing::Event<'_>> for EventRecord {
    fn from(event: &tracing::Event) -> Self {
        let visitor = FieldMapVisitor::default();
        let fields = visitor.visit(&event);
        let metadata = event.metadata();
        EventRecord {
            time: Local::now(),
            level: metadata.level().to_owned().into(),
            fields,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Level(pub tracing::Level);

impl From<tracing::Level> for Level {
    fn from(level: tracing::Level) -> Self {
        Self(level)
    }
}

pub(crate) type FieldMap = IndexMap<String, String>;

#[derive(Debug, Default)]
pub struct FieldMapVisitor {
    fields: FieldMap,
}

impl Visit for FieldMapVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        self.fields
            .insert(field.name().to_owned(), format!("{:?}", value));
    }
}

impl VisitOutput<FieldMap> for FieldMapVisitor {
    fn finish(self) -> FieldMap {
        self.fields
    }
}
