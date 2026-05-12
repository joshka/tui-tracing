# Tui-tracing

[![CI](https://github.com/joshka/tui-tracing/actions/workflows/ci.yml/badge.svg)](https://github.com/joshka/tui-tracing/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/tui-tracing.svg)](https://crates.io/crates/tui-tracing)
[![Documentation](https://docs.rs/tui-tracing/badge.svg)](https://docs.rs/tui-tracing)
[![License](https://img.shields.io/crates/l/tui-tracing.svg)](#license)

`tui-tracing` is a runtime store and widget for displaying [`tracing`] events inside
[`ratatui`]-based applications.

![tui-tracing demo](https://vhs.charm.sh/vhs-36dyKlTisqFpgEBDmaioC.gif)

Its subscriber layer captures spans, events, fields, timing, and span context so a running TUI can
inspect structured trace data without leaving the terminal UI.

Status: experimental pre-release API.

## Quick Start

Add the crate, install the tracing layer once, keep the viewer in app state, and render it like a
normal Ratatui widget:

```sh
cargo add tui-tracing ratatui tracing tracing-subscriber
```

```rust
use ratatui::{DefaultTerminal, Frame};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use tui_tracing::{TraceLayer, TraceViewer};

fn main() -> std::io::Result<()> {
    let (layer, store) = TraceLayer::new();
    tracing_subscriber::registry().with(layer).init();

    let traces = TraceViewer::new(store);
    let mut app = App { traces };
    ratatui::run(|terminal| app.run(terminal))
}

struct App {
    traces: TraceViewer,
}

impl App {
    fn run(&mut self, terminal: &mut DefaultTerminal) -> std::io::Result<()> {
        tracing::info!(target: "demo", peer = "alpha", "connected");
        terminal.draw(|frame| self.render(frame))?;
        Ok(())
    }

    fn render(&mut self, frame: &mut Frame) {
        frame.render_widget(&mut self.traces, frame.area());
    }
}
```

Run the small application-style example with:

```sh
cargo run --example basic
```

The `basic` example emits periodic tracing events and wires a few keys to filtering and scrollback.
The fuller `demo` example is more visual because it also feeds the README GIF.

## Relationship To tui-logger

`tui-tracing` is inspired by [`tui-logger`], which is a mature Ratatui log viewer with `log`,
`slog`, and [`tracing_subscriber::Layer`] support. `tui-logger` is the better fit when an
application wants an established logging console with target-level controls, capture-level
controls, environment-filter integration, file logging, custom formatters, and a target selector
widget.

This crate is narrower. It focuses on making native [`tracing`] output feel like the default
[`tracing_subscriber::fmt`] event stream inside a Ratatui application: timestamp, level, target,
message, structured fields, and optional span context.

Use `tui-tracing` when the important part is the structured tracing data and a compact fmt-like
event stream. Use `tui-logger` when the important part is a full logging widget with built-in
target and level management across logging backends.

## Compatibility

- MSRV: Rust 1.88.
- Ratatui: 0.30.
- Backend: the library API is a Ratatui widget. The examples use crossterm through Ratatui's
  `ratatui::run` helper.

## More Documentation

- [API documentation](https://docs.rs/tui-tracing/latest/tui_tracing/) explains the runtime model,
  viewer state, filtering, retention, event rows, and current limitations.
- [Architecture overview](docs/architecture.md) describes where behavior belongs in the crate.
- [Roadmap](https://github.com/joshka/tui-tracing/issues/22) tracks planned viewer affordances such
  as aggregation and secondary views.

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Maintainer Documentation

- [Design and quality standards](docs/standards.md)
- [Release checklist](docs/release-checklist.md)

## Local Checks

```sh
just ci
```

[`ratatui`]: https://docs.rs/ratatui/latest/ratatui/
[`tracing`]: https://docs.rs/tracing/latest/tracing/
[`tracing_subscriber::Layer`]: https://docs.rs/tracing-subscriber/latest/tracing_subscriber/layer/trait.Layer.html
[`tracing_subscriber::fmt`]: https://docs.rs/tracing-subscriber/latest/tracing_subscriber/fmt/
[`tui-logger`]: https://github.com/gin66/tui-logger
