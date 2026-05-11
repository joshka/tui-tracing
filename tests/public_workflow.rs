use ratatui::backend::TestBackend;
use ratatui::Terminal;
use tracing::subscriber;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::Registry;
use tui_tracing::{FieldValue, TraceFilter, TraceLayer, TraceStore, TraceViewer};

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
