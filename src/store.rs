//! Runtime trace storage.
//!
//! [`TraceStore`] is the handoff point between the tracing subscriber layer and TUI
//! rendering. The subscriber writes complete structured records, and the application
//! reads snapshots later for display-time filtering.
//!
//! Event retention is bounded FIFO storage. When the event buffer reaches capacity,
//! the oldest retained event is evicted before the new event is retained. A capacity
//! of zero drops all events at insertion time while still allowing span metadata to
//! be recorded. These storage counters are independent of display-time filtering.

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
                captured_events: 0,
                accepted_events: 0,
                evicted_events: 0,
                dropped_events: 0,
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
            status: inner.status(),
        }
    }

    /// Return cheap storage counters without cloning retained records.
    ///
    /// These counters describe what reached the store before display-time filtering.
    /// A hidden event still contributes to the counters if it was captured and
    /// retained, evicted, or dropped by storage capacity.
    pub fn status(&self) -> TraceStoreStatus {
        self.inner.read().status()
    }

    /// Remove all retained events and spans and reset storage counters.
    pub fn clear(&self) {
        let mut inner = self.inner.write();
        inner.events.clear();
        inner.spans.clear();
        inner.captured_events = 0;
        inner.accepted_events = 0;
        inner.evicted_events = 0;
        inner.dropped_events = 0;
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
    captured_events: usize,
    accepted_events: usize,
    evicted_events: usize,
    dropped_events: usize,
    events: VecDeque<EventRecord>,
    spans: IndexMap<SpanId, SpanRecord>,
}

impl TraceStoreInner {
    fn push_event(&mut self, event: EventRecord) {
        self.captured_events += 1;

        if self.event_capacity == 0 {
            self.dropped_events += 1;
            return;
        }

        self.accepted_events += 1;

        if self.events.len() == self.event_capacity {
            self.events.pop_front();
            self.evicted_events += 1;
        }

        self.events.push_back(event);
    }

    fn status(&self) -> TraceStoreStatus {
        TraceStoreStatus {
            event_capacity: self.event_capacity,
            retained_events: self.events.len(),
            retained_spans: self.spans.len(),
            captured_events: self.captured_events,
            accepted_events: self.accepted_events,
            evicted_events: self.evicted_events,
            dropped_events: self.dropped_events,
        }
    }
}

/// Cloned view of retained trace records.
#[derive(Clone, Debug, Default)]
pub struct TraceSnapshot {
    /// Events retained by the store, in capture order.
    pub events: Vec<EventRecord>,

    /// Spans retained by the store, keyed by span identifier.
    pub spans: IndexMap<SpanId, SpanRecord>,

    /// Storage counters captured at the same time as the retained records.
    pub status: TraceStoreStatus,
}

/// Cheap summary of retained trace storage and storage-level event loss.
///
/// Counts are measured since store creation or the last [`TraceStore::clear`].
/// Display-time filtering does not change these values. `dropped_events` counts
/// events that reached the store but could not be retained at insertion time; with
/// the current FIFO policy, that only happens when event capacity is zero.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TraceStoreStatus {
    /// Maximum number of events retained by this store.
    pub event_capacity: usize,

    /// Number of events currently retained by this store.
    pub retained_events: usize,

    /// Number of spans currently retained by this store.
    pub retained_spans: usize,

    /// Number of events passed to this store.
    pub captured_events: usize,

    /// Number of captured events accepted into storage.
    pub accepted_events: usize,

    /// Number of previously retained events removed because capacity was reached.
    pub evicted_events: usize,

    /// Number of captured events discarded before being retained.
    pub dropped_events: usize,
}

impl TraceStoreStatus {
    /// Return the total number of captured events that are no longer retained.
    pub fn lost_events(self) -> usize {
        self.evicted_events + self.dropped_events
    }

    /// Return `true` when no events are currently retained.
    pub fn is_empty(self) -> bool {
        self.retained_events == 0
    }
}
