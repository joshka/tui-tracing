# Tui-tracing

`tui-tracing` is a native `tracing` viewer for Ratatui applications.

It is inspired by `tui-logger`, but it does not treat tracing as log compatibility glue. The
crate captures spans, events, fields, timing, and span context directly from `tracing-subscriber`
so a running TUI can inspect more information than it currently displays.

Status: experimental pre-release API.

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Design Direction

The default viewer is event-stream-first. It should feel familiar to users of
`tracing_subscriber::fmt`: colored levels, readable timestamps, messages, structured fields, and
compact span context.

Span trees, timing summaries, and aggregation are secondary views built from the same retained
records. Nesting is important data, but it should not dominate the first diagnostic view.

## Runtime Model

`tui-tracing` separates capture from display:

1. `TraceLayer` captures structured tracing records into `TraceStore`.
1. The application keeps the store in runtime state.
1. `TraceViewer` renders retained records in a Ratatui widget.
1. `TraceFilter` changes what is shown without losing captured data.

Capture-time filtering is still useful for cost control. Display-time filtering is the main
interaction model for diagnostics inside a running TUI.

`TraceStore::status()` exposes cheap storage counters for status bars and diagnostics: configured
capacity, retained events, retained spans, captured events, accepted events, evicted events, and
dropped events. These counters describe storage behavior before display-time filtering, so hiding
an event with `TraceFilter` does not change the captured, evicted, or dropped totals. The returned
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

`TraceViewer` intentionally implements `Widget` for `&mut TraceViewer`. The viewer must update
scroll state during rendering because the tail position depends on the rendered height and the
current number of visible rows. That value is only known once the host application chooses a
layout area for the widget.

The practical effect is that applications can keep `TraceViewer` directly in app state, mutate it
through methods such as `set_filter`, `scroll_up`, `scroll_down`, and `follow_tail`, and then
render `&mut viewer` in the area they assign to trace output. This avoids `StatefulWidget` while
still preserving correct follow-tail and scrollback behavior.

## Current Public Path

- `TraceLayer`: subscriber layer for capture.
- `TraceStore`: shared runtime buffer of retained records and storage counters.
- `TraceViewer`: Ratatui event-stream viewer.
- `TraceFilter`: display-time event filter.
- `TimingLayer`: optional span busy/idle timing layer.

The demo in `examples/demo.rs` is intentionally small and should stay aligned with the public API
path above.

## Known Limitations

- Aggregation of repeated events is not implemented yet.
- The default viewer is intentionally simple and does not yet expose secondary span-tree or timing
  views.
- The timing layer remains local to this crate. The related upstream tracing PR did not appear to
  land as a stable `tracing-subscriber` API.
- Selection, expanded details, grouped rows, page movement, and higher-level viewer status
  summaries are planned follow-up work rather than part of the initial viewer surface.

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
