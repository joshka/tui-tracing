//! Optional span busy/idle timing.
//!
//! [`TimingLayer`] records timing data into span extensions. [`crate::TraceLayer`]
//! can then copy the latest [`Timing`] into retained [`crate::SpanRecord`] values.
//!
//! # Common Workflow
//!
//! Install `TimingLayer` in the same subscriber stack as `TraceLayer` when selected
//! event detail should include span busy and idle durations.
//!
//! # Lifecycle And Side Effects
//!
//! The layer stores timing state in [`tracing`] span extensions. It has no background
//! task and no drop-time cleanup. Timing stops changing once a span closes.
//!
//! # Related Modules
//!
//! - [`crate::layer`] copies timing values into retained span records.
//! - [`crate::record`] stores optional timing data on [`crate::SpanRecord`].
//! - [`crate::viewer`] displays timing in selected-event detail.

use std::time::Duration;

use quanta::Instant;
use tracing::Subscriber;
use tracing::span::{self, Attributes};
use tracing_subscriber::Layer;
use tracing_subscriber::layer::Context;
use tracing_subscriber::registry::LookupSpan;

/// Subscriber layer that tracks busy and idle time for spans.
///
/// The layer stores [`Timing`] in each span's extensions. [`TraceLayer`](crate::TraceLayer)
/// reads that extension and copies the latest timing values into retained span records.
///
/// `TimingLayer` should be installed alongside [`crate::TraceLayer`]. By itself it
/// only updates span extensions; it does not retain or render anything.
///
/// ```
/// use tracing::subscriber;
/// use tracing_subscriber::Registry;
/// use tracing_subscriber::layer::SubscriberExt;
/// use tui_tracing::{TimingLayer, TraceLayer};
///
/// let (trace_layer, store) = TraceLayer::new();
/// let subscriber = Registry::default().with(TimingLayer).with(trace_layer);
///
/// subscriber::with_default(subscriber, || {
///     let span = tracing::info_span!("request");
///     let _guard = span.enter();
///     tracing::info!("handled request");
/// });
///
/// let snapshot = store.snapshot();
/// assert_eq!(snapshot.events.len(), 1);
/// ```
#[derive(Debug, Default)]
pub struct TimingLayer;

/// Busy and idle timing recorded for one tracing span.
///
/// Busy time is accumulated while the span is entered. Idle time is accumulated
/// while the span exists but is not currently entered.
///
/// Timing values are copied into [`crate::SpanRecord`] snapshots. They are cheap
/// to copy and contain no handles back to the subscriber.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    state: TimingState,
    idle: Duration,
    busy: Duration,
    last: Instant,
    enter_count: u64,
    exit_count: u64,
}

impl Default for Timing {
    fn default() -> Self {
        Self::new()
    }
}

/// Current lifecycle state for span timing.
///
/// The state describes the latest known span timing lifecycle, not the
/// application-level meaning of the span.
#[derive(Debug, Default, Clone, Copy, Hash, PartialEq, Eq)]
pub enum TimingState {
    /// The span is closed.
    ///
    /// No further timing information will be recorded.
    Closed,
    /// The span is currently idle.
    ///
    /// Timing information will be recorded when the span becomes busy or closed.
    #[default]
    Idle,
    /// The span is currently busy.
    ///
    /// Timing information will be recorded when the span becomes idle or closed.
    Busy,
}

impl<C> Layer<C> for TimingLayer
where
    C: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, _attrs: &Attributes<'_>, id: &span::Id, ctx: Context<'_, C>) {
        if let Some(span) = ctx.span(id) {
            span.extensions_mut().insert(Timing::new());
        }
    }

    fn on_enter(&self, id: &span::Id, ctx: Context<'_, C>) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let mut extensions = span.extensions_mut();
        if let Some(timing) = extensions.get_mut::<Timing>() {
            timing.enter();
        }
    }

    fn on_exit(&self, id: &span::Id, ctx: Context<'_, C>) {
        let Some(span) = ctx.span(id) else {
            return;
        };
        let mut extensions = span.extensions_mut();
        if let Some(timing) = extensions.get_mut::<Timing>() {
            timing.exit();
        }
    }

    fn on_close(&self, id: span::Id, ctx: Context<'_, C>) {
        let Some(span) = ctx.span(&id) else {
            return;
        };
        let mut extensions = span.extensions_mut();
        if let Some(timing) = extensions.get_mut::<Timing>() {
            timing.close();
        }
    }
}

impl Timing {
    /// Create a timing record in the idle state.
    ///
    /// Applications normally do not call this directly; [`TimingLayer`] creates
    /// timing records for new spans.
    pub fn new() -> Self {
        Self {
            state: TimingState::Idle,
            idle: Duration::ZERO,
            busy: Duration::ZERO,
            last: Instant::now(),
            enter_count: 0,
            exit_count: 0,
        }
    }

    /// Record that the span has been entered.
    ///
    /// If this is called while the span is idle, the idle time will be updated. If this is called
    /// while the span is busy, the busy time will be updated.
    ///
    /// This method is public for tests and custom subscriber integrations. Normal
    /// applications should let [`TimingLayer`] call it from subscriber callbacks.
    pub fn enter(&mut self) {
        self.record();
        self.enter_count += 1;
        self.state = TimingState::Busy;
    }

    /// Record that the span has been exited.
    ///
    /// If this is called while the span is busy, the busy time will be updated. If this is called
    /// while the span is idle, the idle time will be updated.
    ///
    /// This method is public for tests and custom subscriber integrations. Normal
    /// applications should let [`TimingLayer`] call it from subscriber callbacks.
    pub fn exit(&mut self) {
        self.record();
        self.exit_count += 1;
        self.state = TimingState::Idle;
    }

    /// Record that the span has been closed.
    ///
    /// If this is called while the span is idle, the idle time will be updated. If this is called
    /// while the span is busy, the busy time will be updated.
    ///
    /// After this is called, no further timing information will be recorded.
    fn close(&mut self) {
        self.record();
        self.state = TimingState::Closed;
    }

    fn record(&mut self) {
        let now = Instant::now();
        match self.state {
            TimingState::Idle => self.idle += now.duration_since(self.last),
            TimingState::Busy => self.busy += now.duration_since(self.last),
            TimingState::Closed => {}
        }
        self.last = now;
    }

    /// Return the current timing state.
    pub fn state(&self) -> TimingState {
        self.state
    }

    /// Return the idle time spent in this span.
    pub fn idle_duration(&self) -> Duration {
        self.idle
    }

    /// Return the busy time spent in this span.
    pub fn busy_duration(&self) -> Duration {
        self.busy
    }

    /// Return the total recorded time spent in this span.
    pub fn total_duration(&self) -> Duration {
        self.idle + self.busy
    }

    /// Return the number of times this span has been entered.
    pub fn enter_count(&self) -> u64 {
        self.enter_count
    }

    /// Return the number of times this span has been exited.
    ///
    /// Note that close does not count as an exit even though it will update the timing data.
    pub fn exit_count(&self) -> u64 {
        self.exit_count
    }
}

#[cfg(test)]
mod tests {
    use quanta::Clock;

    use super::*;

    #[test]
    fn timing_new() {
        let (clock, _mock) = Clock::mock();
        quanta::with_clock(&clock, || {
            let timing = Timing::new();
            assert_eq!(timing.state(), TimingState::Idle);
            assert_eq!(timing.idle_duration(), Duration::ZERO);
            assert_eq!(timing.busy_duration(), Duration::ZERO);
            assert_eq!(timing.total_duration(), Duration::ZERO);
            assert_eq!(timing.enter_count(), 0);
            assert_eq!(timing.exit_count(), 0);
        });
    }

    #[test]
    fn timing_enter() {
        let (clock, mock) = Clock::mock();
        quanta::with_clock(&clock, || {
            let mut timing = Timing::new();
            const IDLE_DURATION: Duration = Duration::from_secs(1);
            mock.increment(IDLE_DURATION);
            timing.enter();
            assert_eq!(timing.state(), TimingState::Busy);
            assert_eq!(timing.idle_duration(), IDLE_DURATION);
            assert_eq!(timing.busy_duration(), Duration::ZERO);
            assert_eq!(timing.total_duration(), IDLE_DURATION);
            assert_eq!(timing.enter_count(), 1);
            assert_eq!(timing.exit_count(), 0);
        });
    }

    #[test]
    fn timing_exit() {
        let (clock, mock) = Clock::mock();
        quanta::with_clock(&clock, || {
            let mut timing = Timing::new();
            timing.enter();
            const BUSY_DURATION: Duration = Duration::from_secs(1);
            mock.increment(BUSY_DURATION);
            timing.exit();
            assert_eq!(timing.state(), TimingState::Idle);
            assert_eq!(timing.idle_duration(), Duration::ZERO);
            assert_eq!(timing.busy_duration(), BUSY_DURATION);
            assert_eq!(timing.total_duration(), BUSY_DURATION);
            assert_eq!(timing.enter_count(), 1);
            assert_eq!(timing.exit_count(), 1);
        });
    }

    #[test]
    fn timing_enter_and_exit() {
        let (clock, mock) = Clock::mock();
        quanta::with_clock(&clock, || {
            let mut timing = Timing::new();
            const IDLE_DURATION: Duration = Duration::from_secs(1);
            mock.increment(IDLE_DURATION);
            timing.enter();
            const BUSY_DURATION: Duration = Duration::from_secs(2);
            mock.increment(BUSY_DURATION);
            timing.exit();
            assert_eq!(timing.state(), TimingState::Idle);
            assert_eq!(timing.idle_duration(), IDLE_DURATION);
            assert_eq!(timing.busy_duration(), BUSY_DURATION);
            assert_eq!(timing.total_duration(), IDLE_DURATION + BUSY_DURATION);
            assert_eq!(timing.enter_count(), 1);
            assert_eq!(timing.exit_count(), 1);
        });
    }

    #[test]
    fn timing_multiple() {
        let (clock, mock) = Clock::mock();
        quanta::with_clock(&clock, || {
            let mut timing = Timing::new();
            const IDLE_DURATION: Duration = Duration::from_secs(1);
            mock.increment(IDLE_DURATION);
            timing.enter();
            const BUSY_DURATION: Duration = Duration::from_secs(2);
            mock.increment(BUSY_DURATION);
            timing.exit();
            const IDLE_DURATION_2: Duration = Duration::from_secs(3);
            mock.increment(IDLE_DURATION_2);
            timing.enter();
            const BUSY_DURATION_2: Duration = Duration::from_secs(4);
            mock.increment(BUSY_DURATION_2);
            timing.exit();
            assert_eq!(timing.state(), TimingState::Idle);
            assert_eq!(timing.idle_duration(), IDLE_DURATION + IDLE_DURATION_2);
            assert_eq!(timing.busy_duration(), BUSY_DURATION + BUSY_DURATION_2);
            assert_eq!(
                timing.total_duration(),
                IDLE_DURATION + BUSY_DURATION + IDLE_DURATION_2 + BUSY_DURATION_2
            );
            assert_eq!(timing.enter_count(), 2);
            assert_eq!(timing.exit_count(), 2);
        });
    }

    #[test]
    fn timing_close_idle() {
        let (clock, mock) = Clock::mock();
        quanta::with_clock(&clock, || {
            let mut timing = Timing::new();
            const IDLE_DURATION: Duration = Duration::from_secs(1);
            mock.increment(IDLE_DURATION);
            timing.enter();
            const BUSY_DURATION: Duration = Duration::from_secs(2);
            mock.increment(BUSY_DURATION);
            timing.exit();
            const IDLE_DURATION_2: Duration = Duration::from_secs(3);
            mock.increment(IDLE_DURATION_2);
            timing.close();
            assert_eq!(timing.state(), TimingState::Closed);
            assert_eq!(timing.idle_duration(), IDLE_DURATION + IDLE_DURATION_2);
            assert_eq!(timing.busy_duration(), BUSY_DURATION);
            assert_eq!(
                timing.total_duration(),
                IDLE_DURATION + BUSY_DURATION + IDLE_DURATION_2
            );
            assert_eq!(timing.enter_count(), 1);
            assert_eq!(timing.exit_count(), 1);
        });
    }

    #[test]
    fn timing_close_busy() {
        let (clock, mock) = Clock::mock();
        quanta::with_clock(&clock, || {
            let mut timing = Timing::new();
            const IDLE_DURATION: Duration = Duration::from_secs(1);
            mock.increment(IDLE_DURATION);
            timing.enter();
            const BUSY_DURATION: Duration = Duration::from_secs(2);
            mock.increment(BUSY_DURATION);
            timing.close();
            assert_eq!(timing.state(), TimingState::Closed);
            assert_eq!(timing.idle_duration(), IDLE_DURATION);
            assert_eq!(timing.busy_duration(), BUSY_DURATION);
            assert_eq!(timing.total_duration(), IDLE_DURATION + BUSY_DURATION);
            assert_eq!(timing.enter_count(), 1);
            assert_eq!(timing.exit_count(), 0);
        });
    }

    #[test]
    fn timing_exit_while_idle() {
        let (clock, mock) = Clock::mock();
        quanta::with_clock(&clock, || {
            let mut timing = Timing::new();
            const IDLE_DURATION: Duration = Duration::from_secs(1);
            mock.increment(IDLE_DURATION);
            timing.enter();
            const BUSY_DURATION: Duration = Duration::from_secs(2);
            mock.increment(BUSY_DURATION);
            timing.exit();
            const IDLE_DURATION_2: Duration = Duration::from_secs(3);
            mock.increment(IDLE_DURATION_2);
            timing.exit();
            assert_eq!(timing.state(), TimingState::Idle);
            assert_eq!(timing.idle_duration(), IDLE_DURATION + IDLE_DURATION_2);
            assert_eq!(timing.busy_duration(), BUSY_DURATION);
            assert_eq!(
                timing.total_duration(),
                IDLE_DURATION + BUSY_DURATION + IDLE_DURATION_2
            );
            assert_eq!(timing.enter_count(), 1);
            assert_eq!(timing.exit_count(), 2);
        });
    }

    #[test]
    fn timing_enter_while_busy() {
        let (clock, mock) = Clock::mock();
        quanta::with_clock(&clock, || {
            let mut timing = Timing::new();
            const IDLE_DURATION: Duration = Duration::from_secs(1);
            mock.increment(IDLE_DURATION);
            timing.enter();
            const BUSY_DURATION: Duration = Duration::from_secs(2);
            mock.increment(BUSY_DURATION);
            timing.enter();
            assert_eq!(timing.state(), TimingState::Busy);
            assert_eq!(timing.idle_duration(), IDLE_DURATION);
            assert_eq!(timing.busy_duration(), BUSY_DURATION);
            assert_eq!(timing.total_duration(), IDLE_DURATION + BUSY_DURATION);
            assert_eq!(timing.enter_count(), 2);
            assert_eq!(timing.exit_count(), 0);
        });
    }

    #[test]
    fn timing_close_while_closed() {
        let (clock, mock) = Clock::mock();
        quanta::with_clock(&clock, || {
            let mut timing = Timing::new();
            const IDLE_DURATION: Duration = Duration::from_secs(1);
            mock.increment(IDLE_DURATION);
            timing.enter();
            const BUSY_DURATION: Duration = Duration::from_secs(2);
            mock.increment(BUSY_DURATION);
            timing.close();
            const IDLE_DURATION_2: Duration = Duration::from_secs(3);
            mock.increment(IDLE_DURATION_2);
            timing.close();
            assert_eq!(timing.state(), TimingState::Closed);
            assert_eq!(timing.idle_duration(), IDLE_DURATION); // should not include IDLE_DURATION_2
            assert_eq!(timing.busy_duration(), BUSY_DURATION);
            assert_eq!(timing.total_duration(), IDLE_DURATION + BUSY_DURATION);
        });
    }
}
