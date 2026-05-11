//! Ratatui trace viewer.
//!
//! [`TraceViewer`] is the high-level TUI integration point. It owns display-time
//! state such as filters and scrollback, and renders through `impl Widget for
//! &mut TraceViewer` so applications do not need `StatefulWidget`.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::text::Text;
use ratatui::widgets::{Paragraph, Widget};

use crate::filter::TraceFilter;
use crate::format::{event_line, FormatOptions};
use crate::store::{TraceSnapshot, TraceStore, TraceStoreStatus};

/// Event-stream trace viewer for Ratatui applications.
///
/// The viewer does not install tracing subscribers and does not own capture policy.
/// It renders retained records from a [`TraceStore`] using display-time filters.
#[derive(Clone, Debug)]
pub struct TraceViewer {
    store: TraceStore,
    filter: TraceFilter,
    format: FormatOptions,
    follow_tail: bool,
    scroll_top: u16,
    last_tail_scroll: u16,
}

impl TraceViewer {
    /// Create a viewer for the given store.
    pub fn new(store: TraceStore) -> Self {
        Self {
            store,
            filter: TraceFilter::all(),
            format: FormatOptions::default(),
            follow_tail: true,
            scroll_top: 0,
            last_tail_scroll: 0,
        }
    }

    /// Return the store rendered by this viewer.
    pub fn store(&self) -> TraceStore {
        self.store.clone()
    }

    /// Return the active display-time filter.
    pub fn filter(&self) -> &TraceFilter {
        &self.filter
    }

    /// Replace the display-time filter.
    pub fn set_filter(&mut self, filter: TraceFilter) {
        self.filter = filter;
    }

    /// Return structured status data for app-owned status bars.
    ///
    /// This clones a lightweight snapshot of retained records to count visible
    /// events against the current display filter. It does not render or format row
    /// text, and display-time filtering does not affect the storage counters.
    pub fn status(&self) -> TraceViewStatus {
        let snapshot = self.store.snapshot();
        self.status_for_snapshot(&snapshot)
    }

    /// Return the active formatting options.
    pub fn format_options(&self) -> &FormatOptions {
        &self.format
    }

    /// Replace the formatting options.
    pub fn set_format_options(&mut self, format: FormatOptions) {
        self.format = format;
    }

    /// Keep the newest visible event pinned to the bottom of the rendered area.
    pub fn follow_tail(&mut self) {
        self.follow_tail = true;
        self.scroll_top = self.last_tail_scroll;
    }

    /// Scroll older by `lines`.
    pub fn scroll_up(&mut self, lines: u16) {
        if self.follow_tail {
            self.scroll_top = self.last_tail_scroll;
        }
        self.follow_tail = false;
        self.scroll_top = self.scroll_top.saturating_sub(lines);
    }

    /// Scroll newer by `lines`, returning to follow mode at the bottom.
    pub fn scroll_down(&mut self, lines: u16) {
        let next_scroll = self.scroll_top.saturating_add(lines);
        if next_scroll >= self.last_tail_scroll {
            self.follow_tail = true;
            self.scroll_top = self.last_tail_scroll;
        } else {
            self.scroll_top = next_scroll;
        }
    }

    fn visible_text(&self, snapshot: &TraceSnapshot) -> Text<'static> {
        snapshot
            .events
            .iter()
            .filter(|event| self.filter.matches_event(event, snapshot))
            .map(|event| event_line(event, snapshot, &self.format))
            .collect()
    }

    fn visible_event_count(&self, snapshot: &TraceSnapshot) -> usize {
        snapshot
            .events
            .iter()
            .filter(|event| self.filter.matches_event(event, snapshot))
            .count()
    }

    fn status_for_snapshot(&self, snapshot: &TraceSnapshot) -> TraceViewStatus {
        let visible_events = self.visible_event_count(snapshot);
        TraceViewStatus {
            scroll_mode: if self.follow_tail {
                TraceScrollMode::FollowTail
            } else {
                TraceScrollMode::Scrollback
            },
            scroll_top: self.scroll_top,
            tail_scroll: self.last_tail_scroll,
            visible_events,
            hidden_events: snapshot
                .status
                .retained_events
                .saturating_sub(visible_events),
            filter: self.filter.clone(),
            store: snapshot.status,
        }
    }

    fn scroll(&mut self, text_height: usize, area_height: u16) -> u16 {
        let tail_scroll = u16::try_from(text_height)
            .unwrap_or(u16::MAX)
            .saturating_sub(area_height);
        self.last_tail_scroll = tail_scroll;

        if self.follow_tail {
            self.scroll_top = tail_scroll;
            tail_scroll
        } else {
            self.scroll_top = self.scroll_top.min(tail_scroll);
            self.scroll_top
        }
    }
}

impl Widget for &mut TraceViewer {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let snapshot = self.store.snapshot();
        let text = self.visible_text(&snapshot);
        let visible_lines = text.lines.len();
        let scroll = self.scroll(visible_lines, area.height);
        Paragraph::new(text).scroll((scroll, 0)).render(area, buf);
    }
}

/// Current scrolling mode for a [`TraceViewer`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TraceScrollMode {
    /// The viewer follows the newest visible event.
    FollowTail,

    /// The user has moved into scrollback.
    Scrollback,
}

/// Structured status summary for app-owned trace viewer chrome.
///
/// This type intentionally carries data, not formatted text. Applications can use
/// it to build status bars, headers, telemetry, or tests without coupling to the
/// library's demo wording.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceViewStatus {
    /// Current scroll behavior.
    pub scroll_mode: TraceScrollMode,

    /// First visible row offset from the top of the filtered event stream.
    pub scroll_top: u16,

    /// Last computed tail offset for the filtered event stream.
    ///
    /// This value is updated during rendering because it depends on the render
    /// area height. Before the first render it is zero.
    pub tail_scroll: u16,

    /// Number of retained events accepted by the active display filter.
    pub visible_events: usize,

    /// Number of retained events hidden by the active display filter.
    pub hidden_events: usize,

    /// Active display filter.
    pub filter: TraceFilter,

    /// Storage status for retained records and storage-level event loss.
    pub store: TraceStoreStatus,
}
