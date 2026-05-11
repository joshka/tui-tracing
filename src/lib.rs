//! Runtime tracing viewer primitives for Ratatui applications.
//!
//! `tui-tracing` captures native [`tracing`] spans and events into a runtime store
//! and renders that store as a `tracing_subscriber::fmt`-style event stream inside
//! a TUI. It is intentionally tracing-first: it does not adapt through the `log`
//! crate, and display-time filtering is separate from subscriber capture filtering.
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
//! # Core Concepts
//!
//! - [`TraceLayer`] captures structured tracing records.
//! - [`TraceStore`] retains those records for runtime inspection and exposes storage counters
//!   through [`TraceStore::status`].
//! - [`TraceFilter`] filters retained records at display time without losing data.
//! - [`TraceViewer`] renders the event-stream view and owns scroll/filter state.
//! - [`TimingLayer`] optionally records span busy/idle timing.
//!
//! Span trees, timing summaries, and aggregation are secondary views built on the
//! same stored records. The default view stays event-stream-first because that is
//! the most common diagnostic path while an application is running.
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
//! common diagnostic path close to `tracing_subscriber::fmt` output while still
//! preserving structured tracing data for richer views.
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
//! 12:04:32.018 WARN  sync{peer=alpha}: retrying attempt=2 error=timeout
//! ```
//!
//! A row should remain useful when an event has no `message` field. Field-only
//! events are valid tracing output. Complete fields, source location, full span
//! stack, span fields, lifecycle state, and timing belong in a detail view for the
//! selected event.
//!
//! The main screen should expose behavior that applications can bind to their own
//! input model:
//!
//! - set or clear the minimum visible level;
//! - filter by target, module, span name, text, field name, or field value;
//! - scroll older or newer;
//! - jump back to the newest visible event;
//! - select previous or next visible event;
//! - expand selected event details;
//! - toggle source locations;
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
//! that the public API should grow toward include selected-row state, detail
//! rendering, compact filter/status summaries, overflow handling for long context
//! and fields, grouped rows, and tests for follow-tail and selection behavior.

#![forbid(unsafe_code)]
#![deny(rustdoc::bare_urls)]
#![deny(rustdoc::broken_intra_doc_links)]
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
pub use format::FormatOptions;
pub use layer::{TraceLayer, TracingLayer};
pub use record::{EventId, EventRecord, Level, SpanId, SpanRecord};
pub use store::{TraceSnapshot, TraceStore, TraceStoreStatus};
pub use timing_layer::{Timing, TimingLayer};
pub use viewer::TraceViewer;
