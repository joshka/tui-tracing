//! Display-time filtering.
//!
//! Capture-time filters decide what reaches [`crate::TraceStore`]. Display filters
//! decide what a viewer shows from the retained records at render time.
//!
//! # Common Workflow
//!
//! Build a [`TraceFilter`] with the `with_*` methods, then pass it to
//! [`crate::TraceViewer::set_filter`]. The filter changes which retained events are
//! visible without changing store contents or storage counters.
//!
//! # Related Modules
//!
//! - [`crate::layer`] and [`tracing_subscriber`] filters own capture-time filtering.
//! - [`crate::store`] retains records before display-time filtering.
//! - [`crate::viewer`] owns the active filter for an on-screen view.

use crate::record::{EventRecord, Level, SpanRecord};
use crate::store::TraceSnapshot;

/// Display-time event filter.
///
/// Filters are intentionally independent from [`tracing_subscriber`] filters. They do
/// not affect capture, and changing them never loses retained events.
///
/// Matching is substring-based for targets, text, span names, and field values.
/// Level matching keeps events whose level is at least as severe as the configured
/// minimum according to [`tracing`] level ordering.
///
/// ```
/// use tracing::subscriber;
/// use tracing_subscriber::{layer::SubscriberExt, Registry};
/// use tui_tracing::{TraceFilter, TraceLayer, TraceViewer};
///
/// let (layer, store) = TraceLayer::new();
/// let subscriber = Registry::default().with(layer);
///
/// subscriber::with_default(subscriber, || {
///     tracing::debug!(target: "app::cache", "warming cache");
///     tracing::warn!(target: "app::network", status = 503, "retrying request");
/// });
///
/// let mut viewer = TraceViewer::new(store);
/// viewer.set_filter(TraceFilter::all().with_min_level(tracing::Level::INFO));
///
/// let status = viewer.status();
/// assert_eq!(status.visible_events, 1);
/// assert_eq!(status.hidden_events, 1);
/// ```
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct TraceFilter {
    min_level: Option<Level>,
    target: Option<String>,
    text: Option<String>,
    field: Option<FieldFilter>,
    span_name: Option<String>,
}

impl TraceFilter {
    /// Create a filter that accepts every retained event.
    pub fn all() -> Self {
        Self::default()
    }

    /// Set the minimum visible level.
    ///
    /// For example, `INFO` shows `INFO`, `WARN`, and `ERROR`, but hides `DEBUG`
    /// and `TRACE`.
    pub fn with_min_level(mut self, level: tracing::Level) -> Self {
        self.min_level = Some(level.into());
        self
    }

    /// Restrict visible events to targets containing `target`.
    pub fn with_target(mut self, target: impl Into<String>) -> Self {
        self.target = Some(target.into());
        self
    }

    /// Restrict visible events to records containing `text`.
    ///
    /// Text matching searches event targets, event field names and values, span
    /// names, span targets, and span field names and values.
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Restrict visible events to records with a field containing `value`.
    ///
    /// The field name must match exactly. The field value is matched as text.
    pub fn with_field(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.field = Some(FieldFilter {
            name: name.into(),
            value: value.into(),
        });
        self
    }

    /// Restrict visible events to those captured inside a span with `name`.
    pub fn with_span_name(mut self, name: impl Into<String>) -> Self {
        self.span_name = Some(name.into());
        self
    }

    /// Return true when an event should be shown for the snapshot.
    ///
    /// This method is public so applications can reuse the same display-time
    /// matching policy for custom views. It does not mutate the filter, event, or
    /// snapshot.
    pub fn matches_event(&self, event: &EventRecord, snapshot: &TraceSnapshot) -> bool {
        self.matches_level(event)
            && self.matches_target(event)
            && self.matches_field(event)
            && self.matches_span_name(event, snapshot)
            && self.matches_text(event, snapshot)
    }

    fn matches_level(&self, event: &EventRecord) -> bool {
        self.min_level.is_none_or(|level| event.level <= level)
    }

    fn matches_target(&self, event: &EventRecord) -> bool {
        self.target
            .as_deref()
            .is_none_or(|target| event.target.contains(target))
    }

    fn matches_field(&self, event: &EventRecord) -> bool {
        self.field.as_ref().is_none_or(|field| {
            event
                .fields
                .get(&field.name)
                .is_some_and(|value| value.matches_text(&field.value))
        })
    }

    fn matches_span_name(&self, event: &EventRecord, snapshot: &TraceSnapshot) -> bool {
        self.span_name.as_deref().is_none_or(|name| {
            event.span_stack.iter().any(|span_id| {
                snapshot
                    .spans
                    .get(span_id)
                    .is_some_and(|span| span.name.contains(name))
            })
        })
    }

    fn matches_text(&self, event: &EventRecord, snapshot: &TraceSnapshot) -> bool {
        let Some(text) = self.text.as_deref() else {
            return true;
        };

        event.target.contains(text)
            || event
                .fields
                .iter()
                .any(|(name, value)| name.contains(text) || value.matches_text(text))
            || event.span_stack.iter().any(|span_id| {
                snapshot
                    .spans
                    .get(span_id)
                    .is_some_and(|span| span_matches_text(span, text))
            })
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct FieldFilter {
    name: String,
    value: String,
}

fn span_matches_text(span: &SpanRecord, text: &str) -> bool {
    span.name.contains(text)
        || span.target.contains(text)
        || span
            .fields
            .iter()
            .any(|(name, value)| name.contains(text) || value.matches_text(text))
}
