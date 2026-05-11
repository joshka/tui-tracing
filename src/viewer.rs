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
use crate::store::{TraceSnapshot, TraceStore};

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
