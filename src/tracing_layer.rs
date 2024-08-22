use tracing::{span, Subscriber};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

use crate::{storage::TraceStore, Timing};

#[derive(Debug, Default)]
pub struct TracingLayer {
    records: TraceStore,
}

impl TracingLayer {
    pub fn new() -> (Self, TraceStore) {
        let records = TraceStore::default();
        (
            Self {
                records: records.clone(),
            },
            records.clone(),
        )
    }

    fn update_timing<S>(&self, ctx: Context<S>, id: &span::Id)
    where
        S: Subscriber + for<'lookup> LookupSpan<'lookup>,
    {
        let span = ctx.span(id).expect("span not found");
        let extensions = span.extensions();
        let timing = extensions.get::<Timing>().expect("timing not found");
        self.records.update_timing(id.into_u64(), timing);
    }
}

impl<S> Layer<S> for TracingLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, _attrs: &span::Attributes<'_>, id: &span::Id, ctx: Context<'_, S>) {
        let span = ctx.span(id).expect("span not found");
        self.records.insert_span(id.into_u64(), span.into());
    }
    fn on_enter(&self, id: &span::Id, ctx: Context<'_, S>) {
        self.update_timing(ctx, id);
    }

    fn on_exit(&self, id: &span::Id, ctx: Context<'_, S>) {
        self.update_timing(ctx, id);
    }

    fn on_close(&self, id: span::Id, ctx: Context<'_, S>) {
        self.update_timing(ctx, &id);
        self.records.close_span(id.into_u64());
    }

    fn on_event(&self, event: &tracing::Event<'_>, ctx: Context<'_, S>) {
        let id = ctx.event_span(event).map_or(0, |span| span.id().into_u64());
        self.records.insert_event(id, event.into());
    }
}
