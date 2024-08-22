#![allow(unused)]
use core::fmt;
use std::{
    fs::File,
    iter::zip,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, RwLock,
    },
    thread,
    time::Duration,
};

use chrono::TimeDelta;
use color_eyre::Result;
use crossterm::event::{Event, KeyCode, KeyEvent};
use futures::StreamExt;
use indexmap::IndexMap;
use quanta::Instant;
use ratatui::{
    crossterm::event::EventStream,
    text::{self, Text, ToText},
    widgets::Paragraph,
    DefaultTerminal,
};
use tokio::{task::JoinSet, time::MissedTickBehavior};
use tokio_util::sync::CancellationToken;
use tracing::{debug, error, info, instrument, span, trace, Instrument};
use tracing_appender::non_blocking::{self, WorkerGuard};
use tracing_subscriber::{fmt::format::FmtSpan, layer::SubscriberExt, util::SubscriberInitExt};
use tui_tracing::{TimingLayer, TraceStore, TracingLayer};

#[tokio::main]
async fn main() -> Result<()> {
    color_eyre::install()?;
    let (logs, _guard) = init_logs();
    let mut app = App::new(logs);
    let terminal = ratatui::init();
    let app_result = app.run(terminal).await;
    ratatui::restore();
    app_result
}

/// Initialize tracing to log to the internal TUI layer and a file.
///
/// Returns the internal structure that keeps track of the logs and a guard that ensures the file
/// writer is dropped when the program exits (as the file writing is on a background thread).
fn init_logs() -> (TraceStore, WorkerGuard) {
    let (tui_layer, logs) = TracingLayer::new();
    let file = File::create("trace.log").unwrap();
    let (non_blocking, guard) = tracing_appender::non_blocking(file);
    let fmt_layer = tracing_subscriber::fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false);
    tracing_subscriber::registry()
        .with(TimingLayer::default())
        .with(tui_layer)
        .with(fmt_layer)
        .init();
    (logs, guard)
}

#[derive(Debug)]
struct App {
    event_stream: EventStream,
    data: AppData,
}

#[derive(Debug, Clone)]
struct AppData {
    logs: TraceStore,
    cancellation_token: CancellationToken,
}

impl App {
    fn new(logs: TraceStore) -> Self {
        let data = AppData {
            logs,
            cancellation_token: CancellationToken::new(),
        };
        Self {
            event_stream: EventStream::new(),
            data,
        }
    }

    async fn run(&mut self, mut terminal: DefaultTerminal) -> Result<()> {
        let app_data = self.data.clone();
        tokio::spawn(async move { Self::render_loop(terminal, app_data).await });
        self.event_loop().await;
        Ok(())
    }

    #[instrument(skip(self), fields(repeat = false))]
    async fn event_loop(&mut self) -> Result<()> {
        info!("Running");
        let token = self.data.cancellation_token.clone();
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                _ = token.cancelled() => break,
                _ = interval.tick() => self.tick(),
                _ = self.handle_events() => {}
            }
        }
        Ok(())
    }

    #[instrument(skip(self))]
    fn tick(&mut self) {
        self.data.logs.remove_expired(TimeDelta::milliseconds(9900));
    }

    #[instrument(skip_all)]
    async fn render_loop(mut terminal: DefaultTerminal, app_data: AppData) {
        const FPS: f64 = 1.0;
        let mut interval = tokio::time::interval(Duration::from_secs_f64(1.0 / FPS));
        interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
        loop {
            interval.tick().await;
            if let Err(err) = Self::render(&mut terminal, app_data.clone()) {
                error!("Error rendering: {:?}", err);
                break;
            }
        }
    }

    #[instrument(skip(terminal, data))]
    fn render(terminal: &mut DefaultTerminal, data: AppData) -> Result<()> {
        let start = Instant::now();
        terminal.draw(move |frame| {
            let initial_delay = start.elapsed();
            let area = frame.area();
            let spans = data.logs.spans();
            let spans_delay = start.elapsed().saturating_sub(initial_delay);
            let text: Text = spans
                .iter()
                .map(ToText::to_text)
                .flat_map(|t| t.lines)
                .collect();
            let scroll = (text.lines.len() as u16).saturating_sub(area.height);
            let create_text_delay = start.elapsed().saturating_sub(spans_delay);
            frame.render_widget(Paragraph::new(text).scroll((scroll, 0)), area);
            let render_delay = start.elapsed().saturating_sub(create_text_delay);
            trace!(
                frame_count = frame.count(),
                ?initial_delay,
                ?spans_delay,
                ?create_text_delay,
                ?render_delay,
                "Rendered"
            );
        })?;
        Ok(())
    }

    async fn handle_events(&mut self) -> Result<()> {
        use ratatui::crossterm::event;
        if let Some(event) = self.event_stream.next().await {
            match event {
                Ok(event) => {
                    debug!(?event, "Event");
                    if let Event::Key(event) = event {
                        if event.code == KeyCode::Char('q') {
                            self.data.cancellation_token.cancel();
                        }
                    }
                }
                Err(e) => {
                    error!("Error: {:?}", e);
                }
            }
        }
        Ok(())
    }
}
