# Tui-tracing

`tui-tracing` is a runtime store and widget for displaying [`tracing`] events inside
[`ratatui`]-based applications.

![tui-tracing demo](https://github.com/joshka/tui-tracing/releases/latest/download/tui-tracing-demo.gif)

It is inspired by `tui-logger`, but it does not treat tracing as log compatibility glue. The
crate captures spans, events, fields, timing, and span context directly from [`tracing-subscriber`]
so a running TUI can inspect more information than it currently displays.

Status: experimental pre-release API.

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Design Direction

The default viewer is event-stream-first. It should feel familiar to users of the
[`tracing_subscriber::fmt`] formatter: colored levels, readable timestamps, messages, structured
fields, and optional compact span context.

Compact rows default to a short local timestamp so they fit inside an application pane:

```text
12:04:31.123 INFO  app::net: connected latency_ms=12
```

Compact rows hide span context by default so target, message, and fields stay readable.
Applications can enable span context with `TraceViewer::set_show_span_context` or `FormatOptions`
when inline span names are worth the width. When target, optional span context, message, or fields
exceed the rendered width, compact rows use `...` to mark overflow. Span context truncates from the
left so the innermost span remains visible when possible. The selected-event detail remains
complete.

Selected-event detail keeps the complete timestamp, target, module path, source location, promoted
message, event fields, span stack, span fields, lifecycle state, and timing data. Its layout and
styling should communicate ownership: event fields are nested under the event, span fields are
nested under their span, and the span stack is context for the selected event.

Span trees, timing summaries, and aggregation are secondary views built from the same retained
records. Nesting is important data, but it should not dominate the first diagnostic view.

## Runtime Model

`tui-tracing` separates capture from display:

1. [`TraceLayer`] captures structured tracing records into [`TraceStore`].
1. The application keeps the store in runtime state.
1. [`TraceViewer`] renders retained records in a [`ratatui`] widget.
1. [`TraceFilter`] changes what is shown without losing captured data.

Capture-time filtering is still useful for cost control. Display-time filtering is the main
interaction model for diagnostics inside a running TUI.

[`TraceStore::status`] exposes cheap storage counters for status bars and diagnostics: configured
capacity, retained events, retained spans, captured events, accepted events, evicted events, and
dropped events. These counters describe storage behavior before display-time filtering, so hiding
an event with [`TraceFilter`] does not change the captured, evicted, or dropped totals. The returned
status also has helpers for common status-bar decisions such as empty state, remaining event
capacity, capacity pressure, and whether any captured event has been lost.

## Basic Usage

```rust
use ratatui::{backend::TestBackend, Terminal};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use tui_tracing::{TraceFilter, TraceLayer, TraceViewer};

let (layer, store) = TraceLayer::new();
tracing_subscriber::registry().with(layer).init();

let mut viewer = TraceViewer::new(store);
viewer.set_filter(TraceFilter::all().with_min_level(tracing::Level::INFO));

tracing::info!(target: "demo", peer = "alpha", "connected");

let backend = TestBackend::new(80, 4);
let mut terminal = Terminal::new(backend).unwrap();
terminal
    .draw(|frame| frame.render_widget(&mut viewer, frame.area()))
    .unwrap();
```

[`TraceViewer`] intentionally implements [`Widget`] for `&mut TraceViewer`. The viewer must update
scroll state during rendering because the tail position depends on the rendered height and the
current number of visible rows. That value is only known once the host application chooses a
layout area for the widget.

Applications can keep [`TraceViewer`] directly in app state, mutate it through methods such as
`set_filter`, `scroll_up`, `scroll_down`, `page_up`, `page_down`, `jump_to_oldest`, and
`jump_to_newest`, and render `&mut viewer` in the area assigned to trace output. The API avoids
[`StatefulWidget`] while preserving follow-tail and scrollback behavior.

Applications can call [`TraceViewer::status`] to build their own status bars without duplicating
viewer logic. The returned status includes follow-tail versus scrollback mode, the last computed
scroll offsets, visible and hidden event counts after display filtering, selected visible row
state, the active filter, and the underlying `TraceStoreStatus`.

Selection is tracked by retained event id and interpreted through the active display filter.
Applications can move selection with `select_next`, `select_previous`, `select_first`, and
`select_last`, then read `selected_event` or [`TraceViewer::status`] for app-owned detail views.
For rendering full selected-event context, call [`TraceViewer::selected_detail`] and render the returned
[`TraceEventDetail`] into a caller-owned pane. Compact row rendering remains fmt-like and optimized
for scanning: short timestamp, level, target, message, and fields.
[`FormatOptions`] can switch compact rows to full RFC 3339 timestamps or a custom [`chrono`] format.
[`TraceViewer::set_show_span_context`] enables inline span context for applications that want
fmt-style span names in compact rows.
[`TraceViewer::set_show_source_locations`] enables source file and line display in compact rows
without rebuilding the viewer.
Detail rendering is where full fields, source location, and span-stack context live. Its styling
uses a small palette and indentation to show how metadata, fields, and span context relate without
turning the pane into a color legend.
[`TraceEventDetail::text`] exposes the formatted detail text for applications that need their
own scroll state, borders, titles, or layout chrome around the detail pane.

## Current Public Path

- [`TraceLayer`]: subscriber layer for capture.
- [`TraceStore`]: shared runtime buffer of retained records and storage counters.
- [`TraceViewer`]: [`ratatui`] event-stream viewer.
- [`TraceEventDetail`]: renderable selected-event detail.
- [`TraceFilter`]: display-time event filter.
- [`FormatOptions`]: compact row formatting options.
- [`TimingLayer`]: optional span busy/idle timing layer.
- [`TimingState`]: span timing lifecycle state.

The demo in `examples/demo.rs` is intentionally small and should stay aligned with the public API
path above.

## Known Limitations

- Aggregation of repeated events is not implemented yet.
- The default viewer is intentionally simple and does not yet expose secondary span-tree or timing
  views.
- The timing layer remains local to this crate. The related upstream [`tracing`] PR did not appear
  to land as a stable [`tracing-subscriber`] API.
- Grouped rows are planned follow-up work rather than part of the initial viewer surface.

The follow-up work is tracked in the
[main trace viewer roadmap](https://github.com/joshka/tui-tracing/issues/22).

## Maintainer Documentation

- [Design and quality standards](docs/standards.md)
- [Release checklist](docs/release-checklist.md)

## Local Checks

```sh
just ci
```

The `rustfmt.toml` matches the formatting posture used by `../async-tty` and requires nightly
rustfmt for the configured unstable formatting options.

[`formatoptions`]: https://docs.rs/tui-tracing/latest/tui_tracing/struct.FormatOptions.html
[`chrono`]: https://docs.rs/chrono/latest/chrono/
[`ratatui`]: https://docs.rs/ratatui/latest/ratatui/
[`statefulwidget`]: https://docs.rs/ratatui/latest/ratatui/widgets/trait.StatefulWidget.html
[`timinglayer`]: https://docs.rs/tui-tracing/latest/tui_tracing/struct.TimingLayer.html
[`timingstate`]: https://docs.rs/tui-tracing/latest/tui_tracing/enum.TimingState.html
[`traceeventdetail`]: https://docs.rs/tui-tracing/latest/tui_tracing/struct.TraceEventDetail.html
[`traceeventdetail::text`]: https://docs.rs/tui-tracing/latest/tui_tracing/struct.TraceEventDetail.html#method.text
[`tracefilter`]: https://docs.rs/tui-tracing/latest/tui_tracing/struct.TraceFilter.html
[`tracelayer`]: https://docs.rs/tui-tracing/latest/tui_tracing/struct.TraceLayer.html
[`tracestore`]: https://docs.rs/tui-tracing/latest/tui_tracing/struct.TraceStore.html
[`tracestore::status`]: https://docs.rs/tui-tracing/latest/tui_tracing/struct.TraceStore.html#method.status
[`traceviewer`]: https://docs.rs/tui-tracing/latest/tui_tracing/struct.TraceViewer.html
[`traceviewer::selected_detail`]: https://docs.rs/tui-tracing/latest/tui_tracing/struct.TraceViewer.html#method.selected_detail
[`traceviewer::set_show_source_locations`]: https://docs.rs/tui-tracing/latest/tui_tracing/struct.TraceViewer.html#method.set_show_source_locations
[`traceviewer::set_show_span_context`]: https://docs.rs/tui-tracing/latest/tui_tracing/struct.TraceViewer.html#method.set_show_span_context
[`traceviewer::status`]: https://docs.rs/tui-tracing/latest/tui_tracing/struct.TraceViewer.html#method.status
[`tracing`]: https://docs.rs/tracing/latest/tracing/
[`tracing-subscriber`]: https://docs.rs/tracing-subscriber/latest/tracing_subscriber/
[`tracing_subscriber::fmt`]: https://docs.rs/tracing-subscriber/latest/tracing_subscriber/fmt/
[`widget`]: https://docs.rs/ratatui/latest/ratatui/widgets/trait.Widget.html
