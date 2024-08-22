use std::time::Duration;

use quanta::Instant;
use tracing::{
    span::{self, Attributes},
    Subscriber,
};
use tracing_subscriber::{layer::Context, registry::LookupSpan, Layer};

/// A layer that tracks the time spent in each span.
///
/// This layer records the time spent in each span, storing the timing data in the span's
/// extensions. The layer records the time spent in each span as either "idle" time, when the
/// span is not executing, or "busy" time, when the span is executing. The layer records the
/// time spent in each span as a [`Timing`] resource, which can be accessed by other layers.
#[derive(Debug, Default)]
pub struct TimingLayer;

/// A resource tracking the idle and busy time spent in each span.
///
/// This is used by the [`TimingLayer`] to track the time spent in each span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timing {
    state: State,
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

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// The span is closed.
    ///
    /// No further timing information will be recorded.
    Closed,
    #[default]
    /// The span is currently idle.
    ///
    /// Timing information will be recorded when the span becomes busy or closed.
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
    /// Records that a new span has been created.
    fn on_new_span(&self, _attrs: &Attributes<'_>, id: &span::Id, ctx: Context<'_, C>) {
        let span = ctx.span(id).expect("span not found");
        let mut extensions = span.extensions_mut();
        extensions.insert(Timing::new());
    }

    /// Records that a span has been entered.
    ///
    /// The subscriber records the time spent in the span as "idle" time, as the span is not
    /// executing.
    fn on_enter(&self, id: &span::Id, ctx: Context<'_, C>) {
        let span = ctx.span(id).expect("span not found");
        let mut extensions = span.extensions_mut();
        let timings = extensions.get_mut::<Timing>().expect("timings not found");
        timings.enter();
    }

    /// Records that a span has been exited.
    ///
    /// The subscriber records the time spent in the span as "busy" time, as the span is executing.
    fn on_exit(&self, id: &span::Id, ctx: Context<'_, C>) {
        let span = ctx.span(id).expect("span not found");
        let mut extensions = span.extensions_mut();
        let timings = extensions.get_mut::<Timing>().expect("timings not found");
        timings.exit();
    }

    /// Records that a span has been closed.
    ///
    /// The subscriber records the time spent in the span as either "idle" time or "busy" time, as
    /// the span is not executing.
    fn on_close(&self, id: span::Id, ctx: Context<'_, C>) {
        let span = ctx.span(&id).expect("span not found");
        let mut extensions = span.extensions_mut();
        let timings = extensions.get_mut::<Timing>().expect("timings not found");
        timings.close();
    }
}

impl Timing {
    /// Create a new `Timing` resource.
    pub fn new() -> Self {
        Self {
            state: State::Idle,
            idle: Duration::ZERO,
            busy: Duration::ZERO,
            last: Instant::now(),
            enter_count: 0,
            exit_count: 0,
        }
    }

    /// Record that the span is active.
    ///
    /// If this is called while the span is idle, the idle time will be updated. If this is called
    /// while the span is busy, the busy time will be updated.
    pub fn enter(&mut self) {
        self.record();
        self.enter_count += 1;
        self.state = State::Busy;
    }

    /// Record that the span is idle.
    ///
    /// If this is called while the span is busy, the busy time will be updated. If this is called
    /// while the span is idle, the idle time will be updated.
    pub fn exit(&mut self) {
        self.record();
        self.exit_count += 1;
        self.state = State::Idle;
    }

    /// Record that the span has been closed.
    ///
    /// If this is called while the span is idle, the idle time will be updated. If this is called
    /// while the span is busy, the busy time will be updated.
    ///
    /// After this is called, no further timing information will be recorded.
    fn close(&mut self) {
        self.record();
        self.state = State::Closed;
    }

    fn record(&mut self) {
        let now = Instant::now();
        match self.state {
            State::Idle => self.idle += now.duration_since(self.last),
            State::Busy => self.busy += now.duration_since(self.last),
            State::Closed => {}
        }
        self.last = now;
    }

    /// Get the current state of the span.
    pub fn state(&self) -> State {
        self.state
    }

    /// Get the idle time spent in this span.
    pub fn idle_duration(&self) -> Duration {
        self.idle
    }

    /// Get the busy time spent in this span.
    pub fn busy_duration(&self) -> Duration {
        self.busy
    }

    /// Get the total time spent in this span.
    pub fn total_duration(&self) -> Duration {
        self.idle + self.busy
    }

    /// Get the number of times this span has been entered.
    pub fn enter_count(&self) -> u64 {
        self.enter_count
    }

    /// Get the number of times this span has been exited.
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
            assert_eq!(timing.state(), State::Idle);
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
            assert_eq!(timing.state(), State::Busy);
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
            assert_eq!(timing.state(), State::Idle);
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
            assert_eq!(timing.state(), State::Idle);
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
            assert_eq!(timing.state(), State::Idle);
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
            assert_eq!(timing.state(), State::Closed);
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
            assert_eq!(timing.state(), State::Closed);
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
            assert_eq!(timing.state(), State::Idle);
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
            assert_eq!(timing.state(), State::Busy);
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
            assert_eq!(timing.state(), State::Closed);
            assert_eq!(timing.idle_duration(), IDLE_DURATION); // should not include IDLE_DURATION_2
            assert_eq!(timing.busy_duration(), BUSY_DURATION);
            assert_eq!(timing.total_duration(), IDLE_DURATION + BUSY_DURATION);
        });
    }
}
