use std::fs::File;
use std::time::Duration;

use color_eyre::Result;
use crossterm::event::{Event, KeyCode, KeyEvent};
use futures::StreamExt;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use ratatui::crossterm::event::EventStream;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use ratatui::DefaultTerminal;
use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, trace, warn, Level};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tui_tracing::{TraceFilter, TraceLayer, TraceScrollMode, TraceViewStatus, TraceViewer};

fn main() -> Result<()> {
    color_eyre::install()?;
    let (viewer, _guard) = init_tracing();
    let mut app = App::new(viewer);
    let runtime = tokio::runtime::Runtime::new()?;
    ratatui::run(|terminal| runtime.block_on(app.run(terminal)))
}

fn init_tracing() -> (TraceViewer, WorkerGuard) {
    let (tui_layer, store) = TraceLayer::new();
    let file = File::create("trace.log").expect("create trace log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file);
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false);

    tracing_subscriber::registry()
        .with(tui_layer)
        .with(fmt_layer)
        .init();

    (TraceViewer::new(store), guard)
}

#[derive(Debug)]
struct App {
    event_stream: EventStream,
    viewer: TraceViewer,
    cancellation_token: CancellationToken,
    min_level: Level,
}

impl App {
    fn new(viewer: TraceViewer) -> Self {
        Self {
            event_stream: EventStream::new(),
            viewer,
            cancellation_token: CancellationToken::new(),
            min_level: Level::DEBUG,
        }
    }

    async fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<()> {
        info!("starting demo");
        let token = self.cancellation_token.clone();
        tokio::spawn(generate_traces(token.clone()));
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);

        loop {
            tokio::select! {
                _ = token.cancelled() => break,
                _ = interval.tick() => {
                    terminal.draw(|frame| self.render(frame))?;
                }
                result = self.handle_event() => result?,
            }
        }

        Ok(())
    }

    fn render(&mut self, frame: &mut ratatui::Frame<'_>) {
        let [title_area, trace_area, status_area] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(0),
            Constraint::Length(1),
        ])
        .areas(frame.area());

        let title =
            Line::from("tui-tracing demo").style(Style::default().add_modifier(Modifier::BOLD));
        let status = demo_status(self.min_level, self.viewer.status())
            .style(Style::default().add_modifier(Modifier::DIM));

        frame.render_widget(Paragraph::new(title), title_area);
        frame.render_widget(&mut self.viewer, trace_area);
        frame.render_widget(Paragraph::new(status), status_area);
    }

    async fn handle_event(&mut self) -> Result<()> {
        let Some(event) = self.event_stream.next().await else {
            return Ok(());
        };

        let event = event?;
        match action_for_event(&event) {
            Some(Action::Quit) => self.cancellation_token.cancel(),
            Some(Action::CycleLevel) => self.cycle_level(),
            Some(Action::SelectNext) => self.viewer.select_next(),
            Some(Action::SelectPrevious) => self.viewer.select_previous(),
            Some(Action::ClearSelection) => self.viewer.clear_selection(),
            Some(Action::ScrollUp(lines)) => self.viewer.scroll_up(lines),
            Some(Action::ScrollDown(lines)) => self.viewer.scroll_down(lines),
            Some(Action::FollowTail) => self.viewer.follow_tail(),
            None => log_ignored_event(event),
        }

        Ok(())
    }

    fn cycle_level(&mut self) {
        self.min_level = match self.min_level {
            Level::ERROR => Level::WARN,
            Level::WARN => Level::INFO,
            Level::INFO => Level::DEBUG,
            Level::DEBUG => Level::TRACE,
            Level::TRACE => Level::ERROR,
        };
        self.viewer
            .set_filter(TraceFilter::all().with_min_level(self.min_level));
        info!(visible_level = %self.min_level, "updated display filter");
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    Quit,
    CycleLevel,
    SelectNext,
    SelectPrevious,
    ClearSelection,
    ScrollUp(u16),
    ScrollDown(u16),
    FollowTail,
}

fn action_for_event(event: &Event) -> Option<Action> {
    let Event::Key(KeyEvent { code, .. }) = event else {
        return None;
    };

    match code {
        KeyCode::Char('q') => Some(Action::Quit),
        KeyCode::Char('l') => Some(Action::CycleLevel),
        KeyCode::Char('j') => Some(Action::SelectNext),
        KeyCode::Char('k') => Some(Action::SelectPrevious),
        KeyCode::Esc => Some(Action::ClearSelection),
        KeyCode::Char('u') | KeyCode::Up => Some(Action::ScrollUp(1)),
        KeyCode::Char('d') | KeyCode::Down => Some(Action::ScrollDown(1)),
        KeyCode::Char('b') | KeyCode::PageUp => Some(Action::ScrollUp(10)),
        KeyCode::Char('f') | KeyCode::PageDown => Some(Action::ScrollDown(10)),
        KeyCode::Char('g') | KeyCode::Home => Some(Action::ScrollUp(u16::MAX)),
        KeyCode::Char('G') | KeyCode::End => Some(Action::FollowTail),
        _ => None,
    }
}

fn log_ignored_event(event: Event) {
    match event {
        Event::Key(event) => debug!(?event, "ignored key"),
        event => trace!(?event, "ignored terminal event"),
    }
}

fn demo_status(min_level: Level, status: TraceViewStatus) -> Line<'static> {
    let mode = match status.scroll_mode {
        TraceScrollMode::FollowTail => "tail",
        TraceScrollMode::Scrollback => "scrollback",
    };
    let selected = status
        .selected_visible_index
        .map(|index| format!("selected {}/{}", index + 1, status.visible_events))
        .unwrap_or_else(|| "selected none".to_owned());

    Line::from(format!(
        "q quit | l level {min_level}+ | j/k select | Esc clear | Up/Down or u/d scroll | PgUp/PgDn or b/f page | g/G jump | {mode} | {selected} | visible {}/{} | lost {}",
        status.visible_events,
        status.store.retained_events,
        status.store.lost_events()
    ))
}

async fn generate_traces(token: CancellationToken) {
    let mut rng = StdRng::from_entropy();
    let mut sequence = 0_u64;

    loop {
        let delay = Duration::from_millis(rng.gen_range(40..900));
        tokio::select! {
            _ = token.cancelled() => break,
            _ = tokio::time::sleep(delay) => {}
        }

        sequence += 1;
        emit_random_event(&mut rng, sequence);
    }
}

fn emit_random_event(rng: &mut StdRng, sequence: u64) {
    let request_id = format!("req-{:04}", rng.gen_range(1000..9999));
    let session = ["local", "ssh-prod", "devbox", "staging"][rng.gen_range(0..4)];
    let panel = ["trace", "status", "details", "filters"][rng.gen_range(0..4)];

    match rng.gen_range(0..100) {
        0..=6 => {
            let span = tracing::error_span!(
                target: "demo::storage",
                "persist_snapshot",
                %request_id,
                %session
            );
            let _guard = span.enter();
            error!(
                target: "demo::storage",
                sequence,
                path = "/tmp/tui-tracing/snapshot.json",
                bytes = rng.gen_range(8_000..250_000),
                error = "No space left on device",
                "failed to persist trace snapshot"
            );
        }
        7..=20 => {
            let span = tracing::warn_span!(
                target: "demo::network",
                "fetch_events",
                %request_id,
                endpoint = "/api/traces"
            );
            let _guard = span.enter();
            warn!(
                target: "demo::network",
                sequence,
                attempt = rng.gen_range(2..=5),
                retry_after_ms = rng.gen_range(100..2_500),
                status = 503,
                "trace event fetch failed; scheduling retry"
            );
        }
        21..=45 => {
            let span = tracing::info_span!(
                target: "demo::app",
                "apply_filter",
                %request_id,
                level = ?["ERROR", "WARN", "INFO", "DEBUG", "TRACE"][rng.gen_range(0..5)]
            );
            let _guard = span.enter();
            info!(
                target: "demo::app",
                sequence,
                visible = rng.gen_range(15..600),
                retained = rng.gen_range(900..10_000),
                elapsed_ms = rng.gen_range(1..35),
                "updated display filter"
            );
        }
        46..=72 => {
            let span = tracing::debug_span!(
                target: "demo::render",
                "draw_frame",
                frame = rng.gen_range(1_000..9_999),
                %panel
            );
            let _guard = span.enter();
            debug!(
                target: "demo::render",
                sequence,
                rows = rng.gen_range(20..80),
                dirty_regions = rng.gen_range(0..12),
                frame_time_ms = rng.gen_range(3..28),
                "rendered trace viewer"
            );
        }
        _ => {
            let span = tracing::trace_span!(
                target: "demo::input",
                "poll_events",
                source = "crossterm"
            );
            let _guard = span.enter();
            trace!(
                target: "demo::input",
                sequence,
                event = ?["key:Down", "key:End", "mouse:scroll", "resize"][rng.gen_range(0..4)],
                queue_depth = rng.gen_range(0..20),
                "processed terminal event"
            );
        }
    }
}
