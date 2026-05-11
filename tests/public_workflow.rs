use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tracing::subscriber;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;
use tui_tracing::{FieldValue, TraceFilter, TraceLayer, TraceScrollMode, TraceStore, TraceViewer};

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
