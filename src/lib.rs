//! # tui-tracing
//!
//! `tui-tracing` provides a runtime store and [`ratatui`] widget for showing
//! [`tracing`] output inside a TUI application.
//!
//! The default viewer is an event stream shaped like
//! [`tracing_subscriber::fmt`](mod@tracing_subscriber::fmt): timestamp, level,
//! target, message, and structured fields. The store keeps native tracing records
//! behind that compact view so applications can inspect span context, source
//! locations, timing data, and selected-event detail without leaving the terminal UI.
//!
//! # Start Here
//!
//! Install [`TraceLayer`] in your subscriber, keep [`TraceViewer`] in application
//! state, and render it in the part of your layout that should show trace output.
//!
//! ```no_run
//! use ratatui::{DefaultTerminal, Frame};
//! use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
//! use tui_tracing::{TraceLayer, TraceViewer};
//!
//! # fn main() -> std::io::Result<()> {
//! let (layer, store) = TraceLayer::new();
//! tracing_subscriber::registry().with(layer).init();
//!
//! let traces = TraceViewer::new(store);
//! let mut app = App { traces };
//! ratatui::run(|terminal| app.run(terminal))
//! # }
//!
//! struct App {
//!     traces: TraceViewer,
//! }
//!
//! impl App {
//!     fn run(&mut self, terminal: &mut DefaultTerminal) -> std::io::Result<()> {
//!         tracing::info!(target: "demo", peer = "alpha", "connected");
//!         terminal.draw(|frame| self.render(frame))?;
//!         Ok(())
//!     }
//!
//!     fn render(&mut self, frame: &mut Frame) {
//!         frame.render_widget(&mut self.traces, frame.area());
//!     }
//! }
//! ```
//!
//! For a runnable application-style example, run `cargo run --example basic`.
//! For a more visual demo, run `cargo run --example demo`.
//!
//! # When To Use It
//!
//! Use this crate when a Ratatui application already emits [`tracing`] events and
//! should expose those diagnostics inside the UI. It is useful for developer tools,
//! demos, long-running terminal applications, and debug panes where switching to a
//! separate log file or terminal stream would break the workflow.
//!
//! `tui-tracing` does not install a subscriber for you, does not manage terminal
//! raw mode, and does not replace normal file or stdout logging. Install
//! [`TraceLayer`] beside a regular
//! [`tracing_subscriber::fmt`](mod@tracing_subscriber::fmt) layer when the
//! application wants both in-app diagnostics and durable logs.
//!
//! # How The Pieces Fit
//!
//! The crate separates capture, storage, filtering, and rendering:
//!
//! - [`TraceLayer`] is the [`tracing_subscriber::Layer`] that receives tracing callbacks and writes
//!   records into a [`TraceStore`].
//! - [`TraceStore`] is a cloneable, synchronized buffer of retained events and spans. It is the
//!   handoff between subscriber callbacks and UI rendering.
//! - [`TraceViewer`] owns one Ratatui view of a store: scrollback, follow-tail mode, selection, row
//!   options, and the active [`TraceFilter`].
//! - [`TraceFilter`] hides or shows retained events at display time. Changing it does not remove
//!   records from the store.
//!
//! This split gives applications two levels of filtering. Use ordinary
//! [`tracing_subscriber`] filters when noisy records should never reach the store.
//! Use [`TraceFilter`] when the user is exploring records that have already been
//! captured.
//!
//! ```no_run
//! use tracing_subscriber::Layer;
//! use tracing_subscriber::filter::LevelFilter;
//! use tracing_subscriber::layer::SubscriberExt;
//! use tui_tracing::{TraceFilter, TraceLayer, TraceViewer};
//!
//! let (layer, store) = TraceLayer::new();
//! let subscriber = tracing_subscriber::registry()
//!     // Capture-time filter: DEBUG never reaches TraceStore.
//!     .with(layer.with_filter(LevelFilter::INFO));
//!
//! let mut viewer = TraceViewer::new(store);
//! // Display-time filter: retained INFO events can be hidden or shown later.
//! viewer.set_filter(TraceFilter::all().with_min_level(tracing::Level::WARN));
//! # let _ = subscriber;
//! ```
//!
//! # Driving The Viewer
//!
//! [`TraceViewer`] is application state and implements
//! [`ratatui::widgets::Widget`] for `&mut TraceViewer`. Rendering updates the
//! scroll bounds because follow-tail and scrollback depend on the final layout
//! area.
//!
//! Applications decide their own key map and call viewer methods from input
//! handlers:
//!
//! ```
//! use tui_tracing::{TraceStore, TraceViewer};
//!
//! let store = TraceStore::default();
//! let mut viewer = TraceViewer::new(store);
//!
//! viewer.scroll_up(1);
//! viewer.page_down();
//! viewer.jump_to_newest();
//!
//! viewer.select_next();
//! viewer.reveal_selection();
//! ```
//!
//! Use [`TraceViewer::status`] for status bars and [`TraceViewer::selected_detail`]
//! for a caller-owned detail pane:
//!
//! ```
//! use tracing::subscriber;
//! use tracing_subscriber::Registry;
//! use tracing_subscriber::layer::SubscriberExt;
//! use tui_tracing::{TraceLayer, TraceViewer};
//!
//! let (layer, store) = TraceLayer::new();
//! let subscriber = Registry::default().with(layer);
//!
//! subscriber::with_default(subscriber, || {
//!     tracing::warn!(status = 503, "retrying request");
//! });
//!
//! let mut viewer = TraceViewer::new(store);
//! viewer.select_first();
//!
//! let status = viewer.status();
//! assert_eq!(status.visible_events, 1);
//!
//! let detail = viewer.selected_detail().expect("selected event");
//! assert!(detail.text().to_string().contains("retrying request"));
//! ```
//!
//! # Event Rows And Detail
//!
//! The default event row uses a short local timestamp so recent events fit inside
//! an application pane:
//!
//! <pre><code><span style="color:#7f8490">12:04:31.123</span> <span
//! style="color:#8bd5ca;font-weight:700">INFO </span> <span style="color:#7f8490">app::net:</span>
//! connected peer=alpha latency_ms=12 <span style="color:#7f8490">12:04:32.018</span> <span
//! style="color:#eed49f;font-weight:700">WARN </span> <span style="color:#7f8490">app::sync:</span>
//! retrying attempt=2 error=timeout</code></pre>
//!
//! Span context and source locations are captured even when compact rows hide
//! them. Applications can show those fields inline when the extra width is useful:
//!
//! ```
//! use tui_tracing::{TraceStore, TraceViewer};
//!
//! let store = TraceStore::default();
//! let mut viewer = TraceViewer::new(store);
//!
//! viewer.set_show_span_context(true);
//! viewer.set_show_source_locations(true);
//! ```
//!
//! Selected-event detail is where complete metadata, fields, span stack, span
//! fields, lifecycle state, and timing live. Applications can render
//! [`TraceEventDetail`] directly or use [`TraceEventDetail::text`] when they want
//! their own borders, layout, or scroll state.
//!
//! # Retention And Status
//!
//! Event retention is bounded. [`TraceStore::default`] keeps the newest 10,000
//! events, and [`TraceStore::with_capacity`] can choose a different event
//! capacity. When the event buffer is full, the oldest retained event is evicted
//! before the new event is inserted. A zero-capacity store counts captured events
//! as dropped instead of retaining them.
//!
//! Span records are retained for context until the store is cleared. This keeps
//! selected-event detail useful even after spans close, but it also means
//! long-running applications should choose capacity and clearing behavior
//! deliberately.
//!
//! [`TraceStore::status`] exposes storage counters before display-time filtering:
//! configured capacity, retained events, retained spans, captured events, accepted
//! events, evicted events, and dropped events. Hiding an event with
//! [`TraceFilter`] does not change those counters. [`TraceViewer::status`] adds
//! view-specific state such as follow-tail versus scrollback mode, scroll offsets,
//! visible and hidden event counts, selection, and the active filter.
//!
//! # Optional Timing
//!
//! [`TimingLayer`] can record span busy and idle timing. Install it alongside
//! [`TraceLayer`] when span timing matters for detail views or custom diagnostics:
//!
//! ```no_run
//! use tracing_subscriber::layer::SubscriberExt;
//! use tracing_subscriber::util::SubscriberInitExt;
//! use tui_tracing::{TimingLayer, TraceLayer};
//!
//! let (trace_layer, _store) = TraceLayer::new();
//!
//! tracing_subscriber::registry()
//!     .with(TimingLayer)
//!     .with(trace_layer)
//!     .init();
//! ```
//!
//! # What To Read Next
//!
//! - [`layer`] explains subscriber integration.
//! - [`store`] documents retention, snapshots, clearing, and storage counters.
//! - [`viewer`] documents scrolling, selection, status, detail rendering, and row controls.
//! - [`filter`] documents display-time matching rules.
//! - [`record`] and [`field`] define the retained record data available to custom views.
//!
//! # Current Scope
//!
//! The default viewer is event-stream-first. It is not a span tree, and
//! aggregation of repeated events is not implemented yet. Future span-tree,
//! timing-summary, and aggregation views should build on the same retained records
//! without changing the compact event stream into the primary nesting view.
//!
//! # Compatibility
//!
//! - MSRV: Rust 1.88.
//! - Ratatui: 0.30.
//! - Backend: the public API is a Ratatui widget. The examples use crossterm through Ratatui's
//!   `ratatui::run` helper.
//!
//! # Feature Flags
//!
//! This crate currently has no feature flags. All public APIs are available with
//! default dependencies.

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
