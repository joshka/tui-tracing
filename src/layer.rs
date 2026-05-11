//! `tracing-subscriber` capture layer.
//!
//! [`TraceLayer`] records tracing spans and events into [`crate::TraceStore`]. It
//! intentionally does not decide what the TUI shows. Applications can combine it
//! with ordinary `tracing_subscriber` capture filters and then apply
//! [`crate::TraceFilter`] at display time.

use tracing::{span, Subscriber};
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;
use tracing_subscriber::Layer;

use crate::field::FieldVisitor;
use crate::record::{SpanId, SpanRecord};
use crate::store::TraceStore;
use crate::Timing;

/// Subscriber layer that captures structured tracing records for a TUI.
#[derive(Debug, Default)]
pub struct TraceLayer {
    store: TraceStore,
}

impl TraceLayer {
    /// Create a layer and the store it writes into.
    ///
    /// Install the layer in a `tracing_subscriber` registry and keep the returned
    /// store in application state for rendering.
    pub fn new() -> (Self, TraceStore) {
        let store = TraceStore::default();
        (Self::from_store(store.clone()), store)
    }

    /// Create a layer that writes into an existing store.
    pub fn from_store(store: TraceStore) -> Self {
        Self { store }
    }

    /// Return the store this layer writes into.
    pub fn store(&self) -> TraceStore {
        self.store.clone()
    }

    fn update_timing<S>(&self, ctx: Context<'_, S>, id: &span::Id)
    where
        S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let extensions = span.extensions();
        if let Some(timing) = extensions.get::<Timing>() {
            self.store.update_timing(id.into_u64(), timing);
        }
    }
}

impl<S> Layer<S> for TraceLayer
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    fn on_new_span(&self, attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        let Some(span) = ctx.span(id) else {
            return;
        };

        let fields = FieldVisitor::default().record_with(|visitor| attrs.record(visitor));
        let parent_id = span.parent().map(|parent| parent.id().into_u64());
        let timing = span.extensions().get::<Timing>().copied();
        let record = SpanRecord::new(id, parent_id, span.metadata(), fields, timing);
        self.store.insert_span(record);
    }

    fn on_enter(&self, id: &span::Id, ctx: Context<'_, S>) {
        self.update_timing(ctx, id);
    }

    fn on_exit(&self, id: &span::Id, ctx: Context<'_, S>) {
        self.update_timing(ctx, id);
    }

    fn on_close(&self, id: span::Id, ctx: Context<'_, S>) {
        self.update_timing(ctx, &id);
        self.store.close_span(id.into_u64());
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        let fields = FieldVisitor::default().record_with(|visitor| event.record(visitor));
        let span_stack = event_span_stack(event, ctx);
        self.store
            .insert_event(event.metadata(), fields, span_stack);
    }
}

fn event_span_stack<S>(event: &tracing::Event<'_>, ctx: Context<'_, S>) -> Vec<SpanId>
where
    S: Subscriber + for<'lookup> LookupSpan<'lookup>,
{
    ctx.event_scope(event)
        .map(|scope| scope.from_root().map(|span| span.id().into_u64()).collect())
        .unwrap_or_default()
}

/// Backwards-compatible alias for the original development-era name.
pub type TracingLayer = TraceLayer;
