//! Runtime trace storage.
//!
//! [`TraceStore`] is the handoff point between the [`tracing`] subscriber layer and
//! TUI rendering. The subscriber writes complete structured records, and the
//! application reads snapshots later for display-time filtering.
//!
//! Event retention is bounded FIFO storage. When the event buffer reaches capacity,
//! the oldest retained event is evicted before the new event is retained. A capacity
//! of zero drops all events at insertion time while still allowing span metadata to
//! be recorded. These storage counters are independent of display-time filtering.
//!
//! # Common Workflow
//!
//! Create a store with [`TraceStore::default`] or [`TraceStore::with_capacity`],
//! install a [`crate::TraceLayer`] that writes to it, and read snapshots or render a
//! [`crate::TraceViewer`] from another clone.
//!
//! # Concurrency
//!
//! `TraceStore` is backed by a lock and is cheap to clone. Subscriber callbacks may
//! write while the UI reads snapshots. Snapshot creation clones retained records so
//! formatting and rendering do not hold the store lock.
//!
//! # Related Modules
//!
//! - [`crate::layer`] writes records into stores.
//! - [`crate::filter`] matches events in snapshots.
//! - [`crate::viewer`] renders a store and exposes view-specific status.

use std::collections::VecDeque;
use std::sync::Arc;

use indexmap::IndexMap;
use parking_lot::RwLock;

use crate::Timing;
use crate::record::{EventId, EventRecord, SpanId, SpanRecord};

const DEFAULT_EVENT_CAPACITY: usize = 10_000;

/// Shared in-memory trace buffer.
///
/// The store is cheap to clone and is safe to share between the tracing layer and
/// the TUI runtime. Events are retained in insertion order up to the configured
/// capacity. Span metadata is retained until the application explicitly clears the
/// store.
///
/// `TraceStore` does not perform I/O and does not spawn background work. Dropping
/// the final clone drops all retained records.
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
    ///
    /// ```
    /// use tui_tracing::TraceStore;
    ///
    /// let store = TraceStore::with_capacity(2);
    /// assert_eq!(store.event_capacity(), 2);
    /// assert!(store.status().is_empty());
    /// ```
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
    ///
    /// The snapshot is point-in-time data. Concurrent capture can make it stale
    /// immediately after it is returned, but the snapshot itself remains
    /// internally consistent and can be rendered without holding a store lock.
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
    ///
    /// This affects every clone of the same store. Events captured after `clear`
    /// start new counters from zero, but event ids continue from the previous
    /// sequence so ids remain monotonic for the lifetime of the store.
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
///
/// Snapshots are immutable point-in-time data. They are intended for rendering,
/// filtering, tests, and custom application views.
#[derive(Clone, Debug, Default)]
pub struct TraceSnapshot {
    /// Events retained by the store, in capture order.
    ///
    /// This vector contains at most [`TraceStoreStatus::event_capacity`] entries.
    /// It may be empty either because no events have been captured, because
    /// capacity is zero, or because the store was cleared.
    pub events: Vec<EventRecord>,

    /// Spans retained by the store, keyed by span identifier.
    ///
    /// Span records are retained for context even if no currently retained event
    /// references them. Use [`TraceStore::clear`] to remove retained spans.
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
    ///
    /// A value of zero means captured events are counted as dropped instead of
    /// retained.
    pub event_capacity: usize,

    /// Number of events currently retained by this store.
    ///
    /// This value is always less than or equal to [`Self::event_capacity`].
    pub retained_events: usize,

    /// Number of spans currently retained by this store.
    pub retained_spans: usize,

    /// Number of events passed to this store.
    ///
    /// This includes retained, evicted, and dropped events.
    pub captured_events: usize,

    /// Number of captured events accepted into storage.
    ///
    /// With a nonzero capacity, accepted events may later be evicted by FIFO
    /// retention.
    pub accepted_events: usize,

    /// Number of previously retained events removed because capacity was reached.
    pub evicted_events: usize,

    /// Number of captured events discarded before being retained.
    ///
    /// With the current retention policy, this only increases when capacity is
    /// zero.
    pub dropped_events: usize,
}

impl TraceStoreStatus {
    /// Return the total number of captured events that are no longer retained.
    pub fn lost_events(self) -> usize {
        self.evicted_events + self.dropped_events
    }

    /// Return the number of additional events this store can retain before eviction.
    ///
    /// This is a point-in-time storage summary. Concurrent capture can make the
    /// value stale immediately after it is read.
    pub fn remaining_event_capacity(self) -> usize {
        self.event_capacity.saturating_sub(self.retained_events)
    }

    /// Return `true` when no events are currently retained.
    ///
    /// An empty store may still have nonzero captured, evicted, or dropped counts.
    pub fn is_empty(self) -> bool {
        self.retained_events == 0
    }

    /// Return `true` when the retained event buffer has reached its configured capacity.
    ///
    /// This returns `true` for zero-capacity stores because retained events and
    /// capacity are both zero.
    pub fn is_at_event_capacity(self) -> bool {
        self.retained_events == self.event_capacity
    }

    /// Return `true` when any captured event is no longer retained.
    pub fn has_lost_events(self) -> bool {
        self.lost_events() > 0
    }

    /// Return `true` when at least one retained event was evicted by capacity pressure.
    pub fn has_evicted_events(self) -> bool {
        self.evicted_events > 0
    }

    /// Return `true` when at least one captured event was discarded before retention.
    pub fn has_dropped_events(self) -> bool {
        self.dropped_events > 0
    }
}
