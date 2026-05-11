//! Runtime trace storage.
//!
//! [`TraceStore`] is the handoff point between the tracing subscriber layer and TUI
//! rendering. The subscriber writes complete structured records, and the application
//! reads snapshots later for display-time filtering.

use std::collections::VecDeque;
use std::sync::Arc;

use indexmap::IndexMap;
use parking_lot::RwLock;

use crate::record::{EventId, EventRecord, SpanId, SpanRecord};
use crate::Timing;

const DEFAULT_EVENT_CAPACITY: usize = 10_000;

/// Shared in-memory trace buffer.
///
/// The store is cheap to clone and is safe to share between the tracing layer and
/// the TUI runtime. Events are retained in insertion order up to the configured
/// capacity. Spans are retained while referenced by retained events or until the
/// application explicitly clears the store.
#[derive(Clone, Debug)]
pub struct TraceStore {
    inner: Arc<RwLock<TraceStoreInner>>,
}

impl Default for TraceStore {
    fn default() -> Self {
        Self::with_capacity(DEFAULT_EVENT_CAPACITY)
    }
}

impl TraceStore {
    /// Create a store retaining at most `event_capacity` events.
    ///
    /// A capacity of zero is accepted and keeps only span metadata.
    pub fn with_capacity(event_capacity: usize) -> Self {
        Self {
            inner: Arc::new(RwLock::new(TraceStoreInner {
                event_capacity,
                next_event_id: 0,
                events: VecDeque::with_capacity(event_capacity),
                spans: IndexMap::new(),
            })),
        }
    }

    /// Return a cloned snapshot of the currently retained records.
    pub fn snapshot(&self) -> TraceSnapshot {
        let inner = self.inner.read();
        TraceSnapshot {
            events: inner.events.iter().cloned().collect(),
            spans: inner.spans.clone(),
        }
    }

    /// Remove all retained events and spans.
    pub fn clear(&self) {
        let mut inner = self.inner.write();
        inner.events.clear();
        inner.spans.clear();
    }

    /// Return the configured event capacity.
    pub fn event_capacity(&self) -> usize {
        self.inner.read().event_capacity
    }

    pub(crate) fn insert_span(&self, span: SpanRecord) {
        self.inner.write().spans.insert(span.id, span);
    }

    pub(crate) fn insert_event(
        &self,
        metadata: &tracing::Metadata<'_>,
        fields: crate::field::FieldMap,
        span_stack: Vec<SpanId>,
    ) {
        let mut inner = self.inner.write();
        let id = inner.next_event_id;
        inner.next_event_id += 1;
        let event = EventRecord::new(id, metadata, fields, span_stack);
        inner.push_event(event);
    }

    pub(crate) fn close_span(&self, id: SpanId) {
        if let Some(span) = self.inner.write().spans.get_mut(&id) {
            span.close();
        }
    }

    pub(crate) fn update_timing(&self, id: SpanId, timing: &Timing) {
        if let Some(span) = self.inner.write().spans.get_mut(&id) {
            span.timing = Some(*timing);
        }
    }
}

#[derive(Debug)]
struct TraceStoreInner {
    event_capacity: usize,
    next_event_id: EventId,
    events: VecDeque<EventRecord>,
    spans: IndexMap<SpanId, SpanRecord>,
}

impl TraceStoreInner {
    fn push_event(&mut self, event: EventRecord) {
        if self.event_capacity == 0 {
            return;
        }
        if self.events.len() == self.event_capacity {
            self.events.pop_front();
        }
        self.events.push_back(event);
    }
}

/// Cloned view of retained trace records.
#[derive(Clone, Debug, Default)]
pub struct TraceSnapshot {
    /// Events retained by the store, in capture order.
    pub events: Vec<EventRecord>,

    /// Spans retained by the store, keyed by span identifier.
    pub spans: IndexMap<SpanId, SpanRecord>,
}
