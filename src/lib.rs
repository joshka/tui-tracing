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
//! Install [`TraceLayer`] in your subscriber, keep a [`TraceViewer`] in application
//! state, and render it in the area where your application wants trace output.
//!
//! ```
//! use ratatui::DefaultTerminal;
//! use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
//! use tui_tracing::{TraceLayer, TraceViewer};
//!
//! let (layer, store) = TraceLayer::new();
//! let _ = tracing_subscriber::registry().with(layer).try_init();
//!
//! let mut viewer = TraceViewer::new(store);
//!
//! fn render(terminal: &mut DefaultTerminal, viewer: &mut TraceViewer) -> std::io::Result<()> {
//!     terminal.draw(|frame| frame.render_widget(viewer, frame.area()))?;
//!     Ok(())
//! }
//!
//! tracing::info!(target: "demo", answer = 42, "ready");
//! ```
//!
//! For a runnable application-style version of this workflow, run
//! `cargo run --example basic`. For the visual demo used by the README GIF, run
//! `cargo run --example demo`.
//!
//! # When To Use It
//!
//! Use this crate when your application already uses [`tracing`] or wants
//! structured runtime diagnostics in its own [`ratatui`] UI. `tui-tracing` captures
//! native spans, events, fields, targets, source locations, and span context through
//! [`tracing_subscriber`].
//!
//! This is not a `log` compatibility viewer and does not install a subscriber for
//! you. It can be installed beside a normal
//! [`tracing_subscriber::fmt`](mod@tracing_subscriber::fmt) layer when an
//! application wants both an in-app trace view and ordinary file or stdout logs.
//!
//! # Compatibility
//!
//! - MSRV: Rust 1.88.
//! - Ratatui: 0.30.
//! - Backend: the public API is a Ratatui widget. The examples use crossterm through Ratatui's
//!   `ratatui::run` helper.
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
//! # Viewer Surface
//!
//! [`TraceViewer`] renders the default event-stream surface. It is meant to
//! answer one question quickly: what just happened in the application?
//!
//! The viewer is not a span tree. Span nesting is retained as event context and
//! can be shown in compact rows when useful, but rows default to the same
//! event-first shape as [`tracing_subscriber::fmt`](mod@tracing_subscriber::fmt):
//! timestamp, level, target, message, and fields.
//!
//! Compact rows default to [`TimestampFormat::ShortLocal`] so recent events fit
//! inside an application pane:
//!
//! ```text
//! 12:04:31.123 INFO  app::net: connected peer=alpha latency_ms=12
//! 12:04:32.018 WARN  app::sync: retrying attempt=2 error=timeout
//! ```
//!
//! Display-time filtering is handled by [`TraceFilter`], so applications can
//! narrow the visible events without dropping retained records. The current
//! filter supports minimum level, target substring, span-name substring,
//! free-text matching, and exact field-name plus text-value matching.
//!
//! [`TraceViewer`] owns the interaction state needed to render this stream:
//! follow-tail versus scrollback mode, scroll position, selected event, source
//! location visibility, span-context visibility, and the active filter.
//! Applications bind their own input model to methods such as
//! [`TraceViewer::scroll_up`], [`TraceViewer::scroll_down`],
//! [`TraceViewer::page_up`], [`TraceViewer::page_down`],
//! [`TraceViewer::jump_to_oldest`], [`TraceViewer::jump_to_newest`],
//! [`TraceViewer::select_previous`], and [`TraceViewer::select_next`].
//!
//! [`TraceViewer::status`] exposes the current view state and underlying storage
//! counters for application-owned status bars. [`TraceViewer::selected_detail`]
//! returns the full detail surface for the selected event, including complete
//! timestamp, target, module path, source location, event fields, span stack,
//! span fields, lifecycle state, and timing when available.

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
