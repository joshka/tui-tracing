//! # tui-tracing
//!
//! `tui-tracing` provides a runtime store and [`ratatui`] widget for displaying
//! [`tracing`] events inside a [`ratatui`]-based application. It captures native
//! tracing spans and events, retains them in [`TraceStore`], and renders them as a
//! [`tracing_subscriber::fmt`](mod@tracing_subscriber::fmt)-style event stream with
//! [`TraceViewer`]. It is intentionally tracing-first: it does not adapt through the `log` crate,
//! and display-time filtering is separate from subscriber capture filtering.
//!
//! Use this crate when an application needs tracing diagnostics available inside
//! its own [`ratatui`] UI at runtime. The primary path is:
//! capture with [`TraceLayer`], retain in [`TraceStore`], filter with
//! [`TraceFilter`], and render with [`TraceViewer`].
//!
//! # Start Here
//!
//! Install [`TraceLayer`] in your subscriber, keep the returned [`TraceStore`] in
//! application state, and render a [`TraceViewer`] in your UI.
//!
//! ```
//! use ratatui::{backend::TestBackend, Terminal};
//! use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
//! use tui_tracing::{TraceLayer, TraceViewer};
//!
//! let (layer, store) = TraceLayer::new();
//! let _ = tracing_subscriber::registry().with(layer).try_init();
//!
//! tracing::info!(target: "demo", answer = 42, "ready");
//!
//! let mut viewer = TraceViewer::new(store);
//! let backend = TestBackend::new(80, 4);
//! let mut terminal = Terminal::new(backend).unwrap();
//! terminal.draw(|frame| frame.render_widget(&mut viewer, frame.area())).unwrap();
//! ```
//!
//! For a runnable version of this workflow, run `cargo run --example basic`. For
//! the interactive demo, run `cargo run --example demo`.
//!
//! # Core Concepts
//!
//! - [`TraceLayer`] captures structured tracing records.
//! - [`TraceStore`] retains those records for runtime inspection and exposes storage counters
//!   through [`TraceStore::status`].
//! - [`TraceFilter`] filters retained records at display time without losing data.
//! - [`FormatOptions`] controls compact event-row formatting.
//! - [`TraceViewer`] renders the event-stream view and owns scroll/filter state.
//! - [`TraceEventDetail`] renders full detail for one selected event.
//! - [`TimingLayer`] optionally records span busy/idle timing.
//!
//! # Module Map
//!
//! - [`layer`] owns [`tracing_subscriber`] integration.
//! - [`store`] owns retained runtime data and storage counters.
//! - [`viewer`] owns Ratatui rendering state.
//! - [`filter`] owns display-time matching.
//! - [`record`] owns captured event and span record shapes.
//! - [`field`] owns structured field values.
//!
//! Span trees, timing summaries, and aggregation are secondary views built on the
//! same stored records. The default view stays event-stream-first because recent
//! event output is the diagnostic surface users already know from
//! [`tracing_subscriber::fmt`](mod@tracing_subscriber::fmt).
//!
//! # Runtime And Lifecycle
//!
//! [`TraceLayer`] is a [`tracing_subscriber::Layer`]. It captures records only
//! while installed in the active subscriber. [`TraceStore`] is an in-memory,
//! cloneable handle backed by synchronization, so capture and rendering can run on
//! different threads.
//!
//! [`TraceViewer`] is application state. It intentionally implements
//! [`ratatui::widgets::Widget`] for `&mut TraceViewer` because scrollback and
//! follow-tail state depend on the render area and can only be clamped correctly
//! during rendering.
//!
//! This crate does not install a global subscriber by itself, does not spawn
//! background tasks, and does not manage terminal raw mode. Those lifecycle
//! concerns remain owned by the application.
//!
//! # Feature Flags
//!
//! This crate currently has no feature flags. All public APIs are available with
//! default dependencies.
//!
//! # Main Screen
//!
//! The default [`TraceViewer`] screen should answer one question first: what just
//! happened in the application, and what context is needed to decide where to look
//! next?
//!
//! The main screen is an event stream, not a span tree. Span nesting is important
//! runtime context, but it should appear as compact inline context first and as
//! full detail only when the user selects or expands an event. This keeps the
//! common diagnostic path close to [`tracing_subscriber::fmt`](mod@tracing_subscriber::fmt) output
//! while still preserving structured tracing data for richer views.
//!
//! The first visible surface should cover:
//!
//! - recent events, with timestamp, colored level, context, message, and fields;
//! - display-time filters, so retained data can be narrowed without changing capture;
//! - follow-tail and scrollback state, so live events do not disrupt inspection;
//! - selection state, so one event can expose full detail without changing the whole view;
//! - enough store status to explain what is visible and what may be hidden.
//!
//! Each event row should optimize for scanning:
//!
//! ```text
//! 12:04:31.123 INFO  app::net: connected peer=alpha latency=12ms
//! 12:04:32.018 WARN  app::sync: retrying attempt=2 error=timeout
//! ```
//!
//! Compact rows use [`TimestampFormat::ShortLocal`] by default because full
//! timestamps consume width needed for target, message, and fields. Inline span
//! context is available through
//! [`FormatOptions`] and [`TraceViewer::set_show_span_context`], but is hidden by
//! default so nested spans do not dominate the event stream. A row should remain
//! useful when an event has no `message` field because field-only events are valid
//! tracing output.
//! Long compact-row content is truncated before Ratatui clips the line, with an
//! inline `...` marker showing that complete data is available in detail.
//!
//! Selected-event detail is the full metadata surface. It keeps the complete
//! timestamp, level, target, module path, source location, promoted message, event
//! fields, full span stack, span fields, lifecycle state, and timing.
//! Detail styling uses restrained color and indentation to show ownership:
//! metadata and fields belong to the selected event, and span fields belong to
//! the span context around that event.
//!
//! The main screen should expose behavior that applications can bind to their own
//! input model:
//!
//! - set or clear the minimum visible level;
//! - filter by target, module, span name, text, field name, or field value;
//! - toggle source locations in compact event rows;
//! - scroll older or newer;
//! - move by rendered pages;
//! - jump to the oldest or newest visible event;
//! - select previous or next visible event;
//! - expand selected event details;
//! - toggle aggregation once grouped rows exist.
//!
//! Aggregation belongs on the main screen after the raw event stream is reliable.
//! Grouped rows should reduce repeated-event noise while preserving access to each
//! occurrence. A conservative grouping key should start with level, target,
//! message, span context shape, field names, and selected stable field values.
//!
//! The main screen should not own every tracing tool. Full span-tree navigation,
//! timing dashboards, task graphs, metrics summaries, capture filter editing, file
//! export, and long-form help should remain separate views or application-owned
//! features built from [`TraceStore`].
//!
//! The current implementation is an initial version of that shape. Missing pieces
//! that the public API should grow toward include compact filter/status summaries,
//! overflow handling for long context and fields, and grouped rows.

#![forbid(unsafe_code)]
#![deny(rustdoc::bare_urls)]
#![deny(rustdoc::broken_intra_doc_links)]
#![warn(missing_docs)]
#![warn(unreachable_pub)]

pub mod field;
pub mod filter;
pub mod layer;
pub mod record;
pub mod store;
pub mod viewer;

mod format;
mod timing_layer;

pub use field::{FieldMap, FieldValue};
pub use filter::TraceFilter;
pub use format::{FormatOptions, TimestampFormat};
pub use layer::TraceLayer;
pub use record::{EventId, EventRecord, Level, SpanId, SpanRecord};
pub use store::{TraceSnapshot, TraceStore, TraceStoreStatus};
pub use timing_layer::{Timing, TimingLayer, TimingState};
pub use viewer::{
    TraceEventDetail, TraceScrollMode, TraceSpanDetail, TraceViewStatus, TraceViewer,
};
