//! Ratatui trace viewer.
//!
//! [`TraceViewer`] is the high-level TUI integration point. It owns display-time
//! state such as filters and scrollback, and renders through `impl Widget for
//! &mut TraceViewer` so applications do not need `StatefulWidget`.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::Text;
use ratatui::widgets::{Paragraph, Widget};

use crate::filter::TraceFilter;
use crate::format::{event_line, FormatOptions};
use crate::record::{EventId, EventRecord};
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
    selected_event_id: Option<EventId>,
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
            selected_event_id: None,
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
        self.clamp_selection();
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

    /// Return the selected retained event if it is still visible.
    pub fn selected_event(&self) -> Option<EventRecord> {
        let snapshot = self.store.snapshot();
        self.selected_event_for_snapshot(&snapshot).cloned()
    }

    /// Select the first visible event if there is one.
    pub fn select_first(&mut self) {
        let snapshot = self.store.snapshot();
        self.selected_event_id = self.visible_events(&snapshot).first().map(|event| event.id);
    }

    /// Select the last visible event if there is one.
    pub fn select_last(&mut self) {
        let snapshot = self.store.snapshot();
        self.selected_event_id = self.visible_events(&snapshot).last().map(|event| event.id);
    }

    /// Move selection toward newer visible events.
    ///
    /// If no event is selected, this selects the first visible event.
    pub fn select_next(&mut self) {
        let snapshot = self.store.snapshot();
        let visible_events = self.visible_events(&snapshot);
        self.selected_event_id = next_selected_event_id(&visible_events, self.selected_event_id);
    }

    /// Move selection toward older visible events.
    ///
    /// If no event is selected, this selects the last visible event.
    pub fn select_previous(&mut self) {
        let snapshot = self.store.snapshot();
        let visible_events = self.visible_events(&snapshot);
        self.selected_event_id =
            previous_selected_event_id(&visible_events, self.selected_event_id);
    }

    /// Clear the selected event.
    pub fn clear_selection(&mut self) {
        self.selected_event_id = None;
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
        let selected_event_id = self.selected_event_id;
        self.visible_events(snapshot)
            .into_iter()
            .map(|event| {
                let line = event_line(event, snapshot, &self.format);
                if Some(event.id) == selected_event_id {
                    line.style(Style::default().add_modifier(Modifier::REVERSED))
                } else {
                    line
                }
            })
            .collect()
    }

    fn visible_event_count(&self, snapshot: &TraceSnapshot) -> usize {
        self.visible_events(snapshot).len()
    }

    fn visible_events<'snapshot>(
        &self,
        snapshot: &'snapshot TraceSnapshot,
    ) -> Vec<&'snapshot EventRecord> {
        snapshot
            .events
            .iter()
            .filter(|event| self.filter.matches_event(event, snapshot))
            .collect()
    }

    fn selected_event_for_snapshot<'snapshot>(
        &self,
        snapshot: &'snapshot TraceSnapshot,
    ) -> Option<&'snapshot EventRecord> {
        let selected_event_id = self.selected_event_id?;
        self.visible_events(snapshot)
            .into_iter()
            .find(|event| event.id == selected_event_id)
    }

    fn selected_visible_index(&self, snapshot: &TraceSnapshot) -> Option<usize> {
        let selected_event_id = self.selected_event_id?;
        self.visible_events(snapshot)
            .into_iter()
            .position(|event| event.id == selected_event_id)
    }

    fn clamp_selection(&mut self) {
        if self.selected_event().is_none() {
            self.selected_event_id = None;
        }
    }

    fn status_for_snapshot(&self, snapshot: &TraceSnapshot) -> TraceViewStatus {
        let visible_events = self.visible_event_count(snapshot);
        let selected_visible_index = self.selected_visible_index(snapshot);
        let selected_event_id = selected_visible_index.and(self.selected_event_id);
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
            selected_visible_index,
            selected_event_id,
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

fn next_selected_event_id(
    visible_events: &[&EventRecord],
    selected_event_id: Option<EventId>,
) -> Option<EventId> {
    if visible_events.is_empty() {
        return None;
    }

    let Some(selected_event_id) = selected_event_id else {
        return Some(visible_events[0].id);
    };

    let Some(selected_index) = visible_events
        .iter()
        .position(|event| event.id == selected_event_id)
    else {
        return visible_events.first().map(|event| event.id);
    };
    let next_index = (selected_index + 1).min(visible_events.len() - 1);
    Some(visible_events[next_index].id)
}

fn previous_selected_event_id(
    visible_events: &[&EventRecord],
    selected_event_id: Option<EventId>,
) -> Option<EventId> {
    if visible_events.is_empty() {
        return None;
    }

    let Some(selected_event_id) = selected_event_id else {
        return visible_events.last().map(|event| event.id);
    };

    let Some(selected_index) = visible_events
        .iter()
        .position(|event| event.id == selected_event_id)
    else {
        return visible_events.last().map(|event| event.id);
    };
    let previous_index = selected_index.saturating_sub(1);
    Some(visible_events[previous_index].id)
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

    /// Selected event position in the filtered visible event stream.
    pub selected_visible_index: Option<usize>,

    /// Selected event identifier, if the selected event is still visible.
    pub selected_event_id: Option<EventId>,

    /// Active display filter.
    pub filter: TraceFilter,

    /// Storage status for retained records and storage-level event loss.
    pub store: TraceStoreStatus,
}
