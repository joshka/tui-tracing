use ratatui::Terminal;
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};
use tracing::subscriber;
use tracing_subscriber::Registry;
use tracing_subscriber::layer::SubscriberExt;
use tui_tracing::{
    EventRecord, FieldMap, FieldValue, Level, TimestampFormat, TraceEventDetail, TraceFilter,
    TraceLayer, TraceScrollMode, TraceSpanDetail, TraceStore, TraceViewer,
};

#[test]
fn store_status_tracks_empty_capacity_as_dropped_events() {
    let store = TraceStore::with_capacity(0);
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        tracing::info!("not retained");
        tracing::warn!("also not retained");
    });

    let status = store.status();
    assert_eq!(status.event_capacity, 0);
    assert_eq!(status.captured_events, 2);
    assert_eq!(status.accepted_events, 0);
    assert_eq!(status.retained_events, 0);
    assert_eq!(status.evicted_events, 0);
    assert_eq!(status.dropped_events, 2);
    assert_eq!(status.lost_events(), 2);
    assert_eq!(status.remaining_event_capacity(), 0);
    assert!(status.is_empty());
    assert!(status.is_at_event_capacity());
    assert!(status.has_lost_events());
    assert!(!status.has_evicted_events());
    assert!(status.has_dropped_events());
}

#[test]
fn store_status_tracks_retained_events_without_eviction() {
    let store = TraceStore::with_capacity(3);
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("request");
        let _guard = span.enter();
        tracing::info!("one");
        tracing::info!("two");
    });

    let status = store.status();
    assert_eq!(status.event_capacity, 3);
    assert_eq!(status.captured_events, 2);
    assert_eq!(status.accepted_events, 2);
    assert_eq!(status.retained_events, 2);
    assert_eq!(status.retained_spans, 1);
    assert_eq!(status.evicted_events, 0);
    assert_eq!(status.dropped_events, 0);
    assert_eq!(status.lost_events(), 0);
    assert_eq!(status.remaining_event_capacity(), 1);
    assert!(!status.is_empty());
    assert!(!status.is_at_event_capacity());
    assert!(!status.has_lost_events());

    let snapshot = store.snapshot();
    assert_eq!(snapshot.status, status);
}

#[test]
fn store_status_tracks_fifo_eviction_and_clear_reset() {
    let store = TraceStore::with_capacity(2);
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        tracing::info!("one");
        tracing::info!("two");
        tracing::info!("three");
    });

    let status = store.status();
    assert_eq!(status.captured_events, 3);
    assert_eq!(status.accepted_events, 3);
    assert_eq!(status.retained_events, 2);
    assert_eq!(status.evicted_events, 1);
    assert_eq!(status.dropped_events, 0);
    assert_eq!(status.lost_events(), 1);
    assert_eq!(status.remaining_event_capacity(), 0);
    assert!(!status.is_empty());
    assert!(status.is_at_event_capacity());
    assert!(status.has_lost_events());
    assert!(status.has_evicted_events());
    assert!(!status.has_dropped_events());

    let snapshot = store.snapshot();
    assert_eq!(snapshot.events.len(), 2);
    assert_eq!(
        snapshot.events[0].fields.get("message"),
        Some(&FieldValue::Debug("two".to_owned()))
    );
    assert_eq!(
        snapshot.events[1].fields.get("message"),
        Some(&FieldValue::Debug("three".to_owned()))
    );

    store.clear();
    assert_eq!(
        store.status(),
        tui_tracing::TraceStoreStatus {
            event_capacity: 2,
            ..Default::default()
        }
    );
    assert_eq!(store.status().remaining_event_capacity(), 2);
    assert!(store.status().is_empty());
    assert!(!store.status().is_at_event_capacity());
    assert!(store.snapshot().events.is_empty());
}

#[test]
fn capture_preserves_span_context_and_field_only_events() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("request", user = "alice");
        let _guard = span.enter();
        tracing::info!(answer = 42);
    });

    let snapshot = store.snapshot();
    assert_eq!(snapshot.events.len(), 1);

    let event = &snapshot.events[0];
    assert_eq!(event.fields.get("answer"), Some(&FieldValue::I64(42)));
    assert_eq!(event.span_stack.len(), 1);

    let span = snapshot.spans.get(&event.span_stack[0]).unwrap();
    assert_eq!(span.name, "request");
    assert_eq!(
        span.fields.get("user"),
        Some(&FieldValue::Str("alice".to_owned()))
    );
}

#[test]
fn viewer_renders_field_only_events() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        tracing::warn!(retries = 3);
    });

    let mut viewer = TraceViewer::new(store);
    let backend = TestBackend::new(80, 4);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| frame.render_widget(&mut viewer, frame.area()))
        .unwrap();

    let rendered = buffer_text(terminal.backend().buffer());
    assert!(rendered.contains("WARN"));
    assert!(rendered.contains("retries=3"));
}

#[test]
fn viewer_uses_compact_default_ordering_without_span_context() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("request", user = "alice");
        let _guard = span.enter();
        tracing::info!(answer = 42, "connected");
    });

    let expected_timestamp = store.snapshot().events[0]
        .timestamp
        .format("%H:%M:%S%.3f")
        .to_string();
    let mut viewer = TraceViewer::new(store);
    let rendered = render_viewer(&mut viewer, 140, 3);
    let row = first_non_empty_row(&rendered);

    assert!(row.starts_with(&expected_timestamp));
    assert!(row.contains("INFO  public_workflow: connected answer=42"));
    assert!(!row.contains("request{user=\"alice\"}:"));
    assert_ordered(row, &["INFO", "public_workflow:", "connected", "answer=42"]);
}

#[test]
fn viewer_can_enable_compact_span_context_without_rebuilding() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("request", user = "alice");
        let _guard = span.enter();
        tracing::info!(answer = 42, "connected");
    });

    let mut viewer = TraceViewer::new(store);
    assert!(!viewer.show_span_context());
    let hidden = render_viewer(&mut viewer, 140, 3);
    assert!(!hidden.contains("request{user=\"alice\"}:"));

    viewer.set_show_span_context(true);
    assert!(viewer.show_span_context());
    let visible = render_viewer(&mut viewer, 140, 3);

    assert!(visible.contains("INFO  public_workflow: request{user=\"alice\"}:"));
    assert!(visible.contains(": connected answer=42"));
}

#[test]
fn viewer_toggles_compact_span_context_for_keybindings() {
    let store = TraceStore::default();
    let mut viewer = TraceViewer::new(store);

    assert!(!viewer.show_span_context());
    assert!(viewer.toggle_span_context());
    assert!(viewer.show_span_context());
    assert!(!viewer.toggle_span_context());
    assert!(!viewer.show_span_context());
}

#[test]
fn viewer_supports_configured_timestamp_formats() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        tracing::info!("configured timestamp");
    });

    let event = store.snapshot().events[0].clone();

    let mut viewer = TraceViewer::new(store.clone());
    let mut format = viewer.format_options().clone();
    format.timestamp_format = TimestampFormat::Rfc3339;
    viewer.set_format_options(format);
    let rendered = render_viewer(&mut viewer, 140, 3);
    assert!(
        first_non_empty_row(&rendered).starts_with(
            &event
                .timestamp
                .format("%Y-%m-%dT%H:%M:%S%.6f%:z")
                .to_string()
        )
    );

    let mut viewer = TraceViewer::new(store);
    let mut format = viewer.format_options().clone();
    format.timestamp_format = TimestampFormat::Custom("[%H:%M]".to_owned());
    viewer.set_format_options(format);
    let rendered = render_viewer(&mut viewer, 140, 3);
    assert!(
        first_non_empty_row(&rendered).starts_with(&event.timestamp.format("[%H:%M]").to_string())
    );
}

#[test]
fn viewer_hides_source_locations_by_default() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        tracing::info!("source hidden");
    });

    let mut viewer = TraceViewer::new(store);
    assert!(!viewer.show_source_locations());

    let rendered = render_viewer(&mut viewer, 140, 3);

    assert!(rendered.contains("source hidden"));
    assert!(!rendered.contains("tests/public_workflow.rs:"));
}

#[test]
fn viewer_can_enable_source_locations_without_rebuilding() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        tracing::warn!("source visible");
    });

    let mut viewer = TraceViewer::new(store);
    let hidden = render_viewer(&mut viewer, 140, 3);
    assert!(!hidden.contains("tests/public_workflow.rs:"));

    viewer.set_show_source_locations(true);
    assert!(viewer.show_source_locations());
    let visible = render_viewer(&mut viewer, 140, 3);

    assert!(visible.contains("tests/public_workflow.rs:"));
    assert!(visible.contains("source visible"));
}

#[test]
fn viewer_toggles_source_locations_for_keybindings() {
    let store = TraceStore::default();
    let mut viewer = TraceViewer::new(store);

    assert!(!viewer.show_source_locations());
    assert!(viewer.toggle_source_locations());
    assert!(viewer.show_source_locations());
    assert!(!viewer.toggle_source_locations());
    assert!(!viewer.show_source_locations());
}

#[test]
fn viewer_marks_overflow_for_long_targets() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        tracing::info!(
            target: "public_workflow::very_long_component_name::deeply_nested_module_name",
            "connected"
        );
    });

    let mut viewer = TraceViewer::new(store);
    let rendered = render_viewer(&mut viewer, 64, 3);
    let row = first_non_empty_row(&rendered);

    assert!(row.contains("..."));
    assert!(row.contains("connected"));
    assert!(!row.contains("deeply_nested_module_name"));
}

#[test]
fn viewer_marks_overflow_for_long_span_context_and_keeps_innermost_span() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        let outer = tracing::info_span!("outer_very_long_span_name", request_id = "req-123456789");
        let _outer = outer.enter();
        let middle = tracing::info_span!("middle_very_long_span_name", shard = "checkout-west");
        let _middle = middle.enter();
        let inner = tracing::info_span!("inner_important_span", peer = "alpha");
        let _inner = inner.enter();
        tracing::warn!("retrying");
    });

    let mut viewer = TraceViewer::new(store);
    viewer.set_show_span_context(true);
    let rendered = render_viewer(&mut viewer, 96, 3);
    let row = first_non_empty_row(&rendered);

    assert!(row.contains("..."));
    assert!(row.contains("inner_important_span"));
    assert!(row.contains("retrying"));
}

#[test]
fn viewer_marks_overflow_for_long_messages() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        tracing::error!(
            target: "demo",
            "failed to persist snapshot after repeated retry attempts for local session"
        );
    });

    let mut viewer = TraceViewer::new(store);
    let rendered = render_viewer(&mut viewer, 72, 3);
    let row = first_non_empty_row(&rendered);

    assert!(row.contains("failed to persist snapshot"));
    assert!(row.contains("..."));
    assert!(!row.contains("local session"));
}

#[test]
fn viewer_marks_overflow_for_long_fields_but_detail_remains_complete() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));
    let value = "field-value-that-is-long-enough-to-overflow-the-compact-row";

    subscriber::with_default(subscriber, || {
        tracing::info!(target: "demo", payload = value, "stored");
    });

    let mut viewer = TraceViewer::new(store);
    let rendered = render_viewer(&mut viewer, 70, 3);
    let row = first_non_empty_row(&rendered);

    assert!(row.contains("stored"));
    assert!(row.contains("payload="));
    assert!(row.contains("..."));
    assert!(!row.contains("compact-row"));

    viewer.select_first();
    let detail = viewer
        .selected_detail()
        .expect("selected visible event has detail");
    let rendered = render_detail(&detail, 140, 16);

    assert!(rendered.contains("message: stored"));
    assert!(
        rendered.contains("payload: field-value-that-is-long-enough-to-overflow-the-compact-row")
    );
}

#[test]
fn display_filter_does_not_drop_captured_events() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        tracing::debug!(target: "demo::detail", "hidden by display filter");
        tracing::error!(target: "demo::important", "visible");
    });

    let mut viewer = TraceViewer::new(store.clone());
    viewer.set_filter(TraceFilter::all().with_min_level(tracing::Level::INFO));

    assert_eq!(store.snapshot().events.len(), 2);

    let backend = TestBackend::new(100, 4);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| frame.render_widget(&mut viewer, frame.area()))
        .unwrap();

    let rendered = buffer_text(terminal.backend().buffer());
    assert!(rendered.contains("visible"));
    assert!(!rendered.contains("hidden by display filter"));
}

#[test]
fn viewer_status_reports_follow_mode_and_visible_counts_without_rendering() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        tracing::debug!("hidden");
        tracing::info!("visible");
        tracing::warn!("also visible");
    });

    let mut viewer = TraceViewer::new(store);
    viewer.set_filter(TraceFilter::all().with_min_level(tracing::Level::INFO));

    let status = viewer.status();
    assert_eq!(status.scroll_mode, TraceScrollMode::FollowTail);
    assert_eq!(status.scroll_top, 0);
    assert_eq!(status.tail_scroll, 0);
    assert_eq!(status.visible_events, 2);
    assert_eq!(status.hidden_events, 1);
    assert_eq!(status.store.retained_events, 3);
    assert_eq!(
        status.filter,
        TraceFilter::all().with_min_level(tracing::Level::INFO)
    );
}

#[test]
fn viewer_status_reports_scrollback_after_scroll_changes() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        for index in 0..6 {
            tracing::info!(index, "event-{index}");
        }
    });

    let mut viewer = TraceViewer::new(store);
    render_viewer(&mut viewer, 100, 3);

    let status = viewer.status();
    assert_eq!(status.scroll_mode, TraceScrollMode::FollowTail);
    assert_eq!(status.tail_scroll, 3);
    assert_eq!(status.scroll_top, 3);
    assert_eq!(status.visible_events, 6);

    viewer.scroll_up(2);
    let status = viewer.status();
    assert_eq!(status.scroll_mode, TraceScrollMode::Scrollback);
    assert_eq!(status.tail_scroll, 3);
    assert_eq!(status.scroll_top, 1);

    viewer.follow_tail();
    let status = viewer.status();
    assert_eq!(status.scroll_mode, TraceScrollMode::FollowTail);
    assert_eq!(status.scroll_top, status.tail_scroll);
}

#[test]
fn viewer_selection_is_empty_without_visible_events() {
    let store = TraceStore::default();
    let mut viewer = TraceViewer::new(store);

    assert_eq!(viewer.status().selected_event_id, None);
    assert!(viewer.selected_event().is_none());

    viewer.select_next();
    assert_eq!(viewer.status().selected_event_id, None);

    viewer.select_previous();
    assert_eq!(viewer.status().selected_event_id, None);
}

#[test]
fn viewer_selection_moves_through_visible_events() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        tracing::info!("only");
    });

    let mut viewer = TraceViewer::new(store);

    viewer.select_next();
    assert_eq!(viewer.status().selected_visible_index, Some(0));
    assert_eq!(
        selected_message(&viewer),
        Some(FieldValue::Debug("only".to_owned()))
    );

    viewer.select_next();
    assert_eq!(viewer.status().selected_visible_index, Some(0));

    viewer.clear_selection();
    assert_eq!(viewer.status().selected_event_id, None);

    viewer.select_previous();
    assert_eq!(viewer.status().selected_visible_index, Some(0));
}

#[test]
fn viewer_selection_uses_filtered_visible_rows() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        tracing::debug!("hidden");
        tracing::info!("visible-info");
        tracing::warn!("visible-warn");
    });

    let mut viewer = TraceViewer::new(store);
    viewer.set_filter(TraceFilter::all().with_min_level(tracing::Level::INFO));

    viewer.select_next();
    assert_eq!(viewer.status().selected_visible_index, Some(0));
    assert_eq!(
        selected_message(&viewer),
        Some(FieldValue::Debug("visible-info".to_owned()))
    );

    viewer.select_next();
    assert_eq!(viewer.status().selected_visible_index, Some(1));
    assert_eq!(
        selected_message(&viewer),
        Some(FieldValue::Debug("visible-warn".to_owned()))
    );

    viewer.set_filter(TraceFilter::all().with_min_level(tracing::Level::ERROR));
    assert_eq!(viewer.status().selected_event_id, None);
    assert!(viewer.selected_event().is_none());
}

#[test]
fn viewer_can_reveal_selection_below_viewport() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        for index in 0..7 {
            tracing::info!("event-{index}");
        }
    });

    let mut viewer = TraceViewer::new(store);
    render_viewer(&mut viewer, 100, 3);
    viewer.jump_to_oldest();
    assert_eq!(
        render_viewer_event_rows(&mut viewer, 100, 3),
        ["event-0", "event-1", "event-2"]
    );

    for _ in 0..5 {
        viewer.select_next();
    }
    viewer.reveal_selection();

    let status = viewer.status();
    assert_eq!(status.scroll_mode, TraceScrollMode::Scrollback);
    assert_eq!(status.selected_visible_index, Some(4));
    assert_eq!(status.scroll_top, 2);
    assert_eq!(
        render_viewer_event_rows(&mut viewer, 100, 3),
        ["event-2", "event-3", "event-4"]
    );
}

#[test]
fn viewer_can_reveal_selection_above_viewport() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        for index in 0..7 {
            tracing::info!("event-{index}");
        }
    });

    let mut viewer = TraceViewer::new(store);
    render_viewer(&mut viewer, 100, 3);
    viewer.select_first();
    viewer.reveal_selection();

    let status = viewer.status();
    assert_eq!(status.scroll_mode, TraceScrollMode::Scrollback);
    assert_eq!(status.selected_visible_index, Some(0));
    assert_eq!(status.scroll_top, 0);
    assert_eq!(
        render_viewer_event_rows(&mut viewer, 100, 3),
        ["event-0", "event-1", "event-2"]
    );
}

#[test]
fn viewer_selection_is_stable_when_new_events_arrive() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        for index in 0..4 {
            tracing::info!("event-{index}");
        }

        let mut viewer = TraceViewer::new(store.clone());
        viewer.select_next();
        viewer.select_next();
        viewer.select_next();
        let selected_event_id = viewer.status().selected_event_id;
        assert_eq!(
            selected_message(&viewer),
            Some(FieldValue::Debug("event-2".to_owned()))
        );

        viewer.scroll_up(1);

        for index in 4..7 {
            tracing::info!("event-{index}");
        }

        let status = viewer.status();
        assert_eq!(status.selected_event_id, selected_event_id);
        assert_eq!(status.selected_visible_index, Some(2));
        assert_eq!(
            selected_message(&viewer),
            Some(FieldValue::Debug("event-2".to_owned()))
        );
    });
}

#[test]
fn viewer_selection_clears_when_retention_evicts_selected_event() {
    let store = TraceStore::with_capacity(2);
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        tracing::info!("one");
        tracing::info!("two");

        let mut viewer = TraceViewer::new(store.clone());
        viewer.select_first();
        assert_eq!(
            selected_message(&viewer),
            Some(FieldValue::Debug("one".to_owned()))
        );

        tracing::info!("three");
        assert_eq!(viewer.status().selected_event_id, None);
        assert!(viewer.selected_event().is_none());

        viewer.select_next();
        assert_eq!(
            selected_message(&viewer),
            Some(FieldValue::Debug("two".to_owned()))
        );
    });
}

#[test]
fn selected_detail_renders_event_fields_span_fields_and_source_location() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("request", user = "alice", route = "/checkout");
        let _guard = span.enter();
        tracing::warn!(answer = 42, "payment declined");
    });

    let mut viewer = TraceViewer::new(store);
    viewer.select_first();

    let rendered = render_detail(
        &viewer
            .selected_detail()
            .expect("selected visible event has renderable detail"),
        120,
        24,
    );

    assert!(rendered.contains("Event"));
    assert!(rendered.contains("level: WARN"));
    assert!(rendered.contains("target: public_workflow"));
    assert!(rendered.contains("location: tests/public_workflow.rs:"));
    assert!(rendered.contains("message: payment declined"));
    assert!(rendered.contains("answer: 42"));
    assert!(rendered.contains("Span stack"));
    assert!(rendered.contains("0. request"));
    assert!(rendered.contains("user: alice"));
    assert!(rendered.contains("route: /checkout"));
    assert!(rendered.contains("lifecycle: closed"));
    assert_eq!(rendered.matches("message: payment declined").count(), 1);
    assert_ordered(
        &rendered,
        &[
            "Event",
            "time:",
            "level:",
            "target:",
            "module:",
            "location:",
            "message:",
            "fields",
            "answer:",
            "Span stack",
            "0. request",
            "lifecycle:",
            "fields",
            "user:",
        ],
    );
}

#[test]
fn selected_detail_styles_event_and_span_hierarchy() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("request", user = "alice");
        let _guard = span.enter();
        tracing::warn!(answer = 42, "payment declined");
    });

    let mut viewer = TraceViewer::new(store);
    viewer.select_first();
    let detail = viewer
        .selected_detail()
        .expect("selected visible event has renderable detail");
    let buffer = render_detail_buffer(&detail, 120, 24);

    assert_cell_style(&buffer, "Event", Color::Cyan, Modifier::BOLD);
    assert_cell_style(&buffer, "message:", Color::Gray, Modifier::empty());
    assert_cell_style(&buffer, "WARN", Color::Reset, Modifier::empty());
    assert_cell_style(&buffer, "fields", Color::Gray, Modifier::BOLD);
    assert_cell_style(&buffer, "answer:", Color::Gray, Modifier::empty());
    assert_cell_style(&buffer, "Span stack", Color::Cyan, Modifier::BOLD);
    assert_cell_style_after(&buffer, "Span stack", "0.", Color::DarkGray, Modifier::DIM);
    assert_cell_style(&buffer, "request", Color::Cyan, Modifier::BOLD);
    assert_cell_style(&buffer, "user:", Color::Gray, Modifier::empty());
}

#[test]
fn selected_detail_renders_field_only_events() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        tracing::info!(answer = 42, ready = true);
    });

    let mut viewer = TraceViewer::new(store);
    viewer.select_first();

    let rendered = render_detail(
        &viewer
            .selected_detail()
            .expect("field-only selected event has detail"),
        100,
        14,
    );

    assert!(rendered.contains("message: <none>"));
    assert!(rendered.contains("answer: 42"));
    assert!(rendered.contains("ready: true"));
    assert!(rendered.contains("Span stack"));
    assert!(rendered.contains("<none>"));
}

#[test]
fn event_detail_renders_missing_source_location_and_missing_span_records() {
    let mut fields = FieldMap::default();
    fields.insert(
        "message".to_owned(),
        FieldValue::Debug("synthetic".to_owned()),
    );
    fields.insert("code".to_owned(), FieldValue::U64(503));

    let event = EventRecord {
        id: 7,
        timestamp: chrono::Local::now(),
        level: Level(tracing::Level::ERROR),
        target: "synthetic::target".to_owned(),
        module_path: None,
        file: None,
        line: None,
        fields,
        span_id: Some(99),
        span_stack: vec![99],
    };
    let detail = TraceEventDetail::new(event, vec![TraceSpanDetail::missing(99)]);

    let rendered = render_detail(&detail, 100, 16);

    assert!(rendered.contains("module: <unknown>"));
    assert!(rendered.contains("location: <unknown>"));
    assert!(rendered.contains("message: synthetic"));
    assert!(rendered.contains("code: 503"));
    assert!(rendered.contains("<missing span 99>"));
}

#[test]
fn selected_detail_styles_missing_values() {
    let event = EventRecord {
        id: 7,
        timestamp: chrono::Local::now(),
        level: Level(tracing::Level::ERROR),
        target: "synthetic::target".to_owned(),
        module_path: None,
        file: None,
        line: None,
        fields: FieldMap::default(),
        span_id: Some(99),
        span_stack: vec![99],
    };
    let detail = TraceEventDetail::new(event, vec![TraceSpanDetail::missing(99)]);
    let buffer = render_detail_buffer(&detail, 100, 16);

    assert_cell_style(&buffer, "<unknown>", Color::DarkGray, Modifier::DIM);
    assert_cell_style(&buffer, "<none>", Color::DarkGray, Modifier::DIM);
    assert_cell_style(&buffer, "<missing span 99>", Color::DarkGray, Modifier::DIM);
}

#[test]
fn viewer_follows_tail_by_default() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        for index in 0..6 {
            tracing::info!(index, "event-{index}");
        }
    });

    let mut viewer = TraceViewer::new(store);
    let rendered = render_viewer(&mut viewer, 100, 3);

    assert!(!rendered.contains("event-0"));
    assert!(rendered.contains("event-5"));
}

#[test]
fn viewer_stays_in_scrollback_until_returning_to_tail() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        for index in 0..6 {
            tracing::info!(index, "event-{index}");
        }

        let mut viewer = TraceViewer::new(store);
        let rendered = render_viewer(&mut viewer, 100, 5);
        assert!(rendered.contains("event-5"));

        viewer.scroll_up(2);

        let rendered = render_viewer(&mut viewer, 100, 5);
        assert!(rendered.contains("event-1"));
        assert!(!rendered.contains("event-5"));

        for index in 6..9 {
            tracing::info!(index, "event-{index}");
        }

        let rendered = render_viewer(&mut viewer, 100, 5);
        assert!(rendered.contains("event-1"));
        assert!(!rendered.contains("event-8"));

        viewer.scroll_down(20);
        let rendered = render_viewer(&mut viewer, 100, 5);
        assert!(!rendered.contains("event-0"));
        assert!(rendered.contains("event-8"));
    });
}

#[test]
fn viewer_page_controls_do_not_move_when_events_fit_viewport() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        for index in 0..2 {
            tracing::info!("event-{index}");
        }
    });

    let mut viewer = TraceViewer::new(store);
    assert_eq!(
        render_viewer_event_rows(&mut viewer, 100, 3),
        ["event-0", "event-1"]
    );

    viewer.page_up();
    assert_eq!(
        render_viewer_event_rows(&mut viewer, 100, 3),
        ["event-0", "event-1"]
    );

    viewer.page_down();
    assert_eq!(
        render_viewer_event_rows(&mut viewer, 100, 3),
        ["event-0", "event-1"]
    );
}

#[test]
fn viewer_page_controls_do_not_move_when_events_equal_viewport() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        for index in 0..3 {
            tracing::info!("event-{index}");
        }
    });

    let mut viewer = TraceViewer::new(store);
    assert_eq!(
        render_viewer_event_rows(&mut viewer, 100, 3),
        ["event-0", "event-1", "event-2"]
    );

    viewer.page_up();
    assert_eq!(
        render_viewer_event_rows(&mut viewer, 100, 3),
        ["event-0", "event-1", "event-2"]
    );

    viewer.page_down();
    assert_eq!(
        render_viewer_event_rows(&mut viewer, 100, 3),
        ["event-0", "event-1", "event-2"]
    );
}

#[test]
fn viewer_page_controls_move_by_rendered_viewport_height() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        for index in 0..7 {
            tracing::info!("event-{index}");
        }
    });

    let mut viewer = TraceViewer::new(store);
    assert_eq!(
        render_viewer_event_rows(&mut viewer, 100, 3),
        ["event-4", "event-5", "event-6"]
    );

    viewer.page_up();
    assert_eq!(
        render_viewer_event_rows(&mut viewer, 100, 3),
        ["event-1", "event-2", "event-3"]
    );

    viewer.page_down();
    assert_eq!(
        render_viewer_event_rows(&mut viewer, 100, 3),
        ["event-4", "event-5", "event-6"]
    );
}

#[test]
fn viewer_jump_controls_move_to_oldest_and_newest_visible_rows() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        for index in 0..7 {
            tracing::info!("event-{index}");
        }
    });

    let mut viewer = TraceViewer::new(store);
    render_viewer(&mut viewer, 100, 3);

    viewer.jump_to_oldest();
    assert_eq!(
        render_viewer_event_rows(&mut viewer, 100, 3),
        ["event-0", "event-1", "event-2"]
    );

    viewer.jump_to_newest();
    assert_eq!(
        render_viewer_event_rows(&mut viewer, 100, 3),
        ["event-4", "event-5", "event-6"]
    );
}

#[test]
fn viewer_page_and_jump_controls_are_sensible_before_first_render() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        for index in 0..7 {
            tracing::info!("event-{index}");
        }
    });

    let mut viewer = TraceViewer::new(store);
    viewer.page_up();

    let status = viewer.status();
    assert_eq!(status.scroll_mode, TraceScrollMode::Scrollback);
    assert_eq!(status.scroll_top, 0);

    viewer.jump_to_oldest();
    assert_eq!(
        render_viewer_event_rows(&mut viewer, 100, 3),
        ["event-0", "event-1", "event-2"]
    );
}

#[test]
fn viewer_jump_oldest_does_not_move_when_new_events_arrive() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        for index in 0..7 {
            tracing::info!("event-{index}");
        }

        let mut viewer = TraceViewer::new(store);
        render_viewer(&mut viewer, 100, 3);
        viewer.jump_to_oldest();
        assert_eq!(
            render_viewer_event_rows(&mut viewer, 100, 3),
            ["event-0", "event-1", "event-2"]
        );

        for index in 7..10 {
            tracing::info!("event-{index}");
        }

        assert_eq!(
            render_viewer_event_rows(&mut viewer, 100, 3),
            ["event-0", "event-1", "event-2"]
        );
    });
}

fn buffer_text(buffer: &ratatui::buffer::Buffer) -> String {
    buffer
        .content()
        .iter()
        .map(|cell| cell.symbol())
        .collect::<Vec<_>>()
        .join("")
}

fn render_viewer(viewer: &mut TraceViewer, width: u16, height: u16) -> String {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| frame.render_widget(viewer, frame.area()))
        .unwrap();
    buffer_text(terminal.backend().buffer())
}

fn render_viewer_event_rows(viewer: &mut TraceViewer, width: u16, height: u16) -> Vec<String> {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| frame.render_widget(viewer, frame.area()))
        .unwrap();

    buffer_rows(terminal.backend().buffer())
        .into_iter()
        .filter_map(|row| event_label(&row))
        .collect()
}

fn render_detail(detail: &TraceEventDetail, width: u16, height: u16) -> String {
    buffer_rows(&render_detail_buffer(detail, width, height)).join("\n")
}

fn render_detail_buffer(detail: &TraceEventDetail, width: u16, height: u16) -> Buffer {
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| frame.render_widget(detail, frame.area()))
        .unwrap();
    terminal.backend().buffer().clone()
}

fn first_non_empty_row(rendered: &str) -> &str {
    rendered
        .lines()
        .find(|line| !line.trim().is_empty())
        .expect("rendered output contains a non-empty row")
}

fn assert_ordered(rendered: &str, needles: &[&str]) {
    let mut start = 0;
    for needle in needles {
        let offset = rendered[start..]
            .find(needle)
            .unwrap_or_else(|| panic!("expected {needle:?} after byte {start} in:\n{rendered}"));
        start += offset + needle.len();
    }
}

fn assert_cell_style(buffer: &Buffer, text: &str, fg: Color, modifier: Modifier) {
    let (x, y) = find_text(buffer, text);
    assert_cell_at(buffer, text, x, y, fg, modifier);
}

fn assert_cell_style_after(
    buffer: &Buffer,
    after: &str,
    text: &str,
    fg: Color,
    modifier: Modifier,
) {
    let (x, y) = find_text_after(buffer, after, text);
    assert_cell_at(buffer, text, x, y, fg, modifier);
}

fn assert_cell_at(buffer: &Buffer, text: &str, x: u16, y: u16, fg: Color, modifier: Modifier) {
    let cell = buffer
        .cell((x, y))
        .expect("text coordinate points inside the buffer");
    assert_eq!(cell.fg, fg, "foreground for {text:?}");
    assert!(
        cell.modifier.contains(modifier),
        "modifier for {text:?}: expected {:?} in {:?}",
        modifier,
        cell.modifier
    );
}

fn find_text(buffer: &Buffer, text: &str) -> (u16, u16) {
    find_text_from(buffer, text, buffer.area.y)
}

fn find_text_after(buffer: &Buffer, after: &str, text: &str) -> (u16, u16) {
    let (_, after_y) = find_text(buffer, after);
    find_text_from(buffer, text, after_y)
}

fn find_text_from(buffer: &Buffer, text: &str, start_y: u16) -> (u16, u16) {
    let area = buffer.area;
    for y in start_y..area.y + area.height {
        let row = (area.x..area.x + area.width)
            .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
            .collect::<String>();
        if let Some(x) = row.find(text) {
            return (u16::try_from(x).expect("test buffer width fits in u16"), y);
        }
    }
    panic!(
        "expected to find {text:?} in:\n{}",
        buffer_rows(buffer).join("\n")
    );
}

fn buffer_rows(buffer: &ratatui::buffer::Buffer) -> Vec<String> {
    let area = buffer.area;
    (area.y..area.y + area.height)
        .map(|y| {
            (area.x..area.x + area.width)
                .filter_map(|x| buffer.cell((x, y)).map(|cell| cell.symbol()))
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .collect()
}

fn event_label(row: &str) -> Option<String> {
    let start = row.find("event-")?;
    let label = row[start..]
        .split_whitespace()
        .next()
        .expect("split_whitespace yields the matched event label");
    Some(label.to_owned())
}

fn selected_message(viewer: &TraceViewer) -> Option<FieldValue> {
    viewer
        .selected_event()
        .and_then(|event| event.fields.get("message").cloned())
}
