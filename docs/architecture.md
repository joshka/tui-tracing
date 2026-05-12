# Architecture Overview

This guide explains where behavior belongs in `tui-tracing`. It is maintainer-facing
documentation for the current crate shape, not a roadmap. Public API docs and Rustdoc remain the
source of truth for exact signatures and behavior. GitHub issues describe planned work and open
tradeoffs; when an issue conflicts with this document, treat the issue as a proposal until the code
and docs change together.

The crate has one primary runtime path:

1. `TraceLayer` captures `tracing` records.
1. `TraceStore` retains captured records and storage counters.
1. `TraceFilter` decides which retained events a view shows.
1. `TraceViewer` owns interaction state and renders rows in Ratatui.
1. `FormatOptions` controls fmt-like compact row formatting.

Keep new behavior near the concept that owns the data or decision. Avoid moving application
lifecycle, terminal setup, demo controls, or roadmap-only views into library code before they have a
clear reusable API.

## Capture

`TraceLayer` is the subscriber integration boundary. It observes spans and events while installed
in a `tracing_subscriber` registry and writes structured records into `TraceStore`.

Put behavior in `TraceLayer` when it answers a capture-time question:

- How should a `tracing` callback become an `EventRecord` or `SpanRecord`?
- Which span metadata, event metadata, fields, and span stack should be captured?
- How should optional span timing be refreshed from subscriber extensions?
- How should an existing `TraceStore` be connected to a subscriber layer?

Do not put display choices in `TraceLayer`. It should not decide which retained records are visible,
how rows are formatted, how input works, or how terminal state is managed. Capture-time filtering
belongs to ordinary `tracing_subscriber` filters around the layer; display-time filtering belongs to
`TraceFilter`.

## Storage

`TraceStore` is the in-memory handoff between subscriber callbacks and UI rendering. It owns
retained events, retained spans, monotonic event ids, bounded FIFO event capacity, and storage
counters.

Put behavior in `TraceStore` when it changes retained data or storage accounting:

- Event insertion, eviction, dropping, and clearing.
- Span insertion, closing, timing updates, and retained span metadata.
- Snapshot creation for readers that must not hold the store lock while rendering.
- Cheap status counters such as captured, accepted, evicted, dropped, and retained records.

Storage counters describe what reached the store before display-time filtering. Hiding an event in
`TraceFilter` must not change captured, evicted, dropped, or retained totals. If a feature needs a
new durable fact about captured records, add it to the record/store layer before teaching the viewer
to display it.

## Filtering

`TraceFilter` owns display-time matching. It narrows a retained `TraceSnapshot` without mutating the
store or losing captured records.

Put behavior in `TraceFilter` when it answers whether a retained event is visible:

- Minimum level matching.
- Target, text, span-name, and field matching.
- Matching rules that custom application views should be able to reuse.
- Filter state that belongs to the display, not the subscriber.

Do not use `TraceFilter` for cost control. If an application needs to avoid capturing data, it
should configure `tracing_subscriber` filters before records reach `TraceLayer`.

## Formatting

`FormatOptions` and the internal `format` module own compact row text. The row shape is deliberately
close to `tracing_subscriber::fmt`: timestamp, level, target, optional span context, message, and
fields.

Put behavior in formatting when it affects compact event rows:

- Timestamp format selection.
- Inline span context visibility and truncation policy.
- Row text composition, field ordering, and overflow markers.
- Source-location text in compact rows.

Formatting should not own selection, scrolling, filtering, or detail layout. Selected-event detail
may use the same captured data, but it is a separate rendering surface because it is complete rather
than width-constrained.

## Viewing

`TraceViewer` is application state for the default Ratatui event stream. It owns the active
`TraceFilter`, `FormatOptions`, follow-tail state, scroll position, selected event id, and the
latest render-derived viewport facts.

Put behavior in `TraceViewer` when it affects default viewer interaction or rendering:

- Scrollback, follow-tail, paging, and jump-to-oldest or jump-to-newest behavior.
- Selection movement and revealing the selected event.
- View status derived from the active filter and latest store snapshot.
- Rendering compact rows and selected-row styling.
- Producing selected-event detail data for caller-owned detail panes.

`TraceViewer` intentionally implements `Widget` for `&mut TraceViewer` because scroll bounds depend
on the final render area. Keep terminal raw mode, event loops, key maps, panes, borders, and status
bar composition in the host application or demo unless the library has a reusable primitive to
expose.

## Demo And Application Concerns

`examples/demo.rs` should show the public path and exercise expected application wiring. It can own
controls, panes, fake workload generation, local status text, and demo pacing.

Keep behavior demo-owned when it is about presenting or driving the example:

- Keyboard bindings and command help.
- Demo data generation and timing.
- Layout choices that combine viewer, detail, and status panes.
- Terminal setup, teardown, and application event loops.

Move behavior from the demo into the library only when it is reusable outside the demo and has a
stable concept in the core API. A useful test is whether another Ratatui app would want the behavior
without also copying the demo's layout or input model.

## Planned Layers

The store already retains richer data than the default compact stream displays. Planned secondary
surfaces should build on the same captured records instead of changing the primary event-stream
contract.

Aggregation belongs above storage and filtering. It should summarize retained events for display
without changing the underlying event records or storage counters.

Detail surfaces belong beside the viewer, not inside capture. Selected-event detail should stay
complete even when compact rows hide span context, source location, or overflowing fields.

Selection belongs to the viewer layer because it is display state interpreted through the active
filter. Selection should use retained event ids rather than row indexes so filtering and retention
changes can be handled deliberately.

Span trees and timing summaries are secondary views over retained spans, event span stacks, and
optional `TimingLayer` data. They should not make the default event stream behave like a span tree.

## Change Placement

Use this checklist when placing a change:

- New captured metadata: `record`, `field`, `layer`, then `store`.
- New retention or loss accounting: `store`.
- New display-time match rule: `filter`, with viewer integration only for active filter state.
- New compact row text or truncation rule: `format` and `FormatOptions`.
- New scroll, selection, or follow-tail behavior: `viewer`.
- New full selected-event content: `viewer` detail types, backed by stored records.
- New demo keybinding, pane, generated event, or visual walkthrough: `examples/demo.rs`.
- New maintainer process, design rule, or release step: `docs/`.

When a feature crosses boundaries, make the data owner explicit first. For example, source locations
are captured as event metadata, retained in records, formatted in compact rows only when enabled,
and always available in detail. That flow keeps capture, storage, formatting, and viewing decisions
separate.
