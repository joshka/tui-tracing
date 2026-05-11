use ratatui::Terminal;
use ratatui::backend::TestBackend;
use tracing::subscriber;
use tracing_subscriber::Registry;
use tracing_subscriber::layer::SubscriberExt;
use tui_tracing::{TraceFilter, TraceLayer, TraceViewer};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let (layer, store) = TraceLayer::new();
    let subscriber = Registry::default().with(layer);

    subscriber::with_default(subscriber, || {
        let span = tracing::info_span!("startup", component = "example");
        let _guard = span.enter();
        tracing::info!(ready = true, "application ready");
        tracing::debug!(cache_entries = 3, "cache warmed");
    });

    let mut viewer = TraceViewer::new(store);
    viewer.set_filter(TraceFilter::all().with_min_level(tracing::Level::INFO));

    let backend = TestBackend::new(100, 4);
    let mut terminal = Terminal::new(backend)?;
    terminal.draw(|frame| frame.render_widget(&mut viewer, frame.area()))?;

    let buffer = terminal.backend().buffer();
    let area = buffer.area;
    for y in area.y..area.y + area.height {
        for x in area.x..area.x + area.width {
            if let Some(cell) = buffer.cell((x, y)) {
                print!("{}", cell.symbol());
            }
        }
        println!();
    }

    Ok(())
}
