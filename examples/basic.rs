use std::time::{Duration, Instant};

use crossterm::event::{self, Event, KeyCode};
use ratatui::DefaultTerminal;
use ratatui::layout::{Constraint, Layout};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::Paragraph;
use tracing::{Level, debug, error, info, warn};
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tui_tracing::{TraceFilter, TraceLayer, TraceScrollMode, TraceViewer};

fn main() -> std::io::Result<()> {
    let (layer, store) = TraceLayer::new();
    tracing_subscriber::registry().with(layer).init();

    let app = App::new(TraceViewer::new(store));
    ratatui::run(|terminal| app.run(terminal))
}

struct App {
    viewer: TraceViewer,
    min_level: Option<Level>,
    tick: u64,
}

impl App {
    fn new(mut viewer: TraceViewer) -> Self {
        viewer.set_filter(TraceFilter::all().with_min_level(Level::INFO));
        Self {
            viewer,
            min_level: Some(Level::INFO),
            tick: 0,
        }
    }

    fn run(mut self, terminal: &mut DefaultTerminal) -> std::io::Result<()> {
        info!("started example application");
        let mut next_tick = Instant::now();

        loop {
            terminal.draw(|frame| self.render(frame))?;

            let timeout = next_tick.saturating_duration_since(Instant::now());
            if event::poll(timeout)? && self.handle_event(event::read()?) {
                break;
            }

            if Instant::now() >= next_tick {
                self.emit_work();
                next_tick = Instant::now() + Duration::from_millis(700);
            }
        }

        Ok(())
    }

    fn render(&mut self, frame: &mut ratatui::Frame<'_>) {
        let [trace_area, status_area] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(frame.area());

        frame.render_widget(&mut self.viewer, trace_area);

        let status = self.status_line().style(
            Style::default()
                .fg(Color::White)
                .bg(Color::Blue)
                .add_modifier(Modifier::BOLD),
        );
        frame.render_widget(Paragraph::new(status), status_area);
    }

    fn status_line(&self) -> Line<'static> {
        let status = self.viewer.status();
        let level = self
            .min_level
            .map(|level| format!("{level}+"))
            .unwrap_or_else(|| "all".to_owned());
        let mode = match status.scroll_mode {
            TraceScrollMode::FollowTail => "tail",
            TraceScrollMode::Scrollback => "scrollback",
        };

        Line::from(format!(
            "q quit | l level | Up/Down scroll | End tail | level {level} | {mode} | visible {}/{}",
            status.visible_events, status.store.retained_events
        ))
    }

    fn handle_event(&mut self, event: Event) -> bool {
        let Event::Key(key) = event else {
            return false;
        };

        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => true,
            KeyCode::Char('l') => {
                self.cycle_level();
                false
            }
            KeyCode::Up => {
                self.viewer.scroll_up(1);
                false
            }
            KeyCode::Down => {
                self.viewer.scroll_down(1);
                false
            }
            KeyCode::PageUp => {
                self.viewer.page_up();
                false
            }
            KeyCode::PageDown => {
                self.viewer.page_down();
                false
            }
            KeyCode::End => {
                self.viewer.jump_to_newest();
                false
            }
            _ => {
                debug!(?key, "ignored key");
                false
            }
        }
    }

    fn cycle_level(&mut self) {
        self.min_level = match self.min_level {
            None => Some(Level::ERROR),
            Some(Level::ERROR) => Some(Level::WARN),
            Some(Level::WARN) => Some(Level::INFO),
            Some(Level::INFO) => Some(Level::DEBUG),
            Some(Level::DEBUG) => None,
            Some(Level::TRACE) => Some(Level::ERROR),
        };

        let filter = self
            .min_level
            .map(|level| TraceFilter::all().with_min_level(level))
            .unwrap_or_else(TraceFilter::all);
        self.viewer.set_filter(filter);

        if let Some(level) = self.min_level {
            info!(%level, "changed display filter");
        } else {
            info!("cleared display filter");
        }
    }

    fn emit_work(&mut self) {
        self.tick += 1;
        let span = tracing::info_span!("sync_worker", tick = self.tick);
        let _guard = span.enter();

        match self.tick % 8 {
            0 => error!(target: "example::sync", retry = true, "failed to sync account"),
            3 | 6 => warn!(target: "example::sync", latency_ms = 850, "sync is slow"),
            even if even % 2 == 0 => {
                debug!(target: "example::cache", entries = self.tick * 4, "updated cache")
            }
            _ => info!(target: "example::sync", records = 12, "synced account"),
        }
    }
}
