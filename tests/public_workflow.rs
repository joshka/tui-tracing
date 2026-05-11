use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tracing::subscriber;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;
use tui_tracing::{
    EventRecord, FieldMap, FieldValue, Level, TraceEventDetail, TraceFilter, TraceLayer,
    TraceScrollMode, TraceSpanDetail, TraceStore, TraceViewer,
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
fn viewer_uses_default_fmt_style_ordering() {
    let store = TraceStore::default();
    let subscriber = Registry::default().with(TraceLayer::from_store(store.clone()));

    subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("request", user = "alice");
        let _guard = span.enter();
        tracing::info!(answer = 42, "connected");
    });

    let mut viewer = TraceViewer::new(store);
    let rendered = render_viewer(&mut viewer, 140, 3);

    assert!(rendered.contains("INFO  request{user=\"alice\"}:"));
    assert!(rendered.contains("public_workflow"));
    assert!(rendered.contains(": connected answer=42"));
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
    let backend = TestBackend::new(width, height);
    let mut terminal = Terminal::new(backend).unwrap();
    terminal
        .draw(|frame| frame.render_widget(detail, frame.area()))
        .unwrap();
    buffer_rows(terminal.backend().buffer()).join("\n")
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
