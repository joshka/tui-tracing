//! Ratatui trace viewer.
//!
//! [`TraceViewer`] is the high-level TUI integration point. It owns display-time
//! state such as filters and scrollback, and renders through `impl Widget for
//! &mut TraceViewer` so applications do not need `StatefulWidget`.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Widget};

use crate::field::FieldMap;
use crate::filter::TraceFilter;
use crate::format::{event_line, FormatOptions};
use crate::record::{EventId, EventRecord, SpanId, SpanRecord};
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
    last_viewport_height: Option<u16>,
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
            last_viewport_height: None,
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

    /// Return renderable detail for the selected retained event.
    ///
    /// The returned value owns cloned record data from a single store snapshot, so
    /// callers can render it later without holding store locks. It returns `None`
    /// when no event is selected or when the selected event is no longer visible
    /// through the active display filter.
    pub fn selected_detail(&self) -> Option<TraceEventDetail> {
        let snapshot = self.store.snapshot();
        let event = self.selected_event_for_snapshot(&snapshot)?;
        Some(TraceEventDetail::from_snapshot(event, &snapshot))
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

    /// Scroll one rendered page toward older visible events.
    ///
    /// Page size is the height of the most recent render area. Before the first
    /// render, the viewer uses one line because no viewport height is known yet.
    pub fn page_up(&mut self) {
        self.scroll_up(self.page_scroll_lines());
    }

    /// Scroll one rendered page toward newer visible events.
    ///
    /// Page size is the height of the most recent render area. Before the first
    /// render, the viewer uses one line because no viewport height is known yet.
    pub fn page_down(&mut self) {
        self.scroll_down(self.page_scroll_lines());
    }

    /// Jump to the oldest retained event accepted by the display filter.
    pub fn jump_to_oldest(&mut self) {
        self.follow_tail = false;
        self.scroll_top = 0;
    }

    /// Jump to the newest retained event accepted by the display filter.
    ///
    /// This returns the viewer to follow-tail mode so newly retained visible events
    /// remain pinned to the bottom of the rendered area.
    pub fn jump_to_newest(&mut self) {
        self.follow_tail();
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
        self.last_viewport_height = Some(area_height);
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

    fn page_scroll_lines(&self) -> u16 {
        self.last_viewport_height.unwrap_or(1).max(1)
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

/// Renderable detail for one captured tracing event.
///
/// A detail value is an owned snapshot of the selected event and its span stack.
/// It is separate from compact row rendering so applications can choose whether to
/// show details in a bottom pane, side pane, popup, or another app-owned surface.
#[derive(Clone, Debug)]
pub struct TraceEventDetail {
    event: EventRecord,
    span_stack: Vec<TraceSpanDetail>,
}

impl TraceEventDetail {
    /// Create event detail from an event and already-resolved span stack.
    pub fn new(event: EventRecord, span_stack: Vec<TraceSpanDetail>) -> Self {
        Self { event, span_stack }
    }

    /// Return the event described by this detail.
    pub fn event(&self) -> &EventRecord {
        &self.event
    }

    /// Return the event span stack from root to innermost span.
    pub fn span_stack(&self) -> &[TraceSpanDetail] {
        &self.span_stack
    }

    fn from_snapshot(event: &EventRecord, snapshot: &TraceSnapshot) -> Self {
        let span_stack = event
            .span_stack
            .iter()
            .map(|span_id| {
                snapshot
                    .spans
                    .get(span_id)
                    .cloned()
                    .map(TraceSpanDetail::present)
                    .unwrap_or_else(|| TraceSpanDetail::missing(*span_id))
            })
            .collect();
        Self::new(event.clone(), span_stack)
    }

    /// Format this detail as Ratatui text.
    ///
    /// This is useful when callers need app-owned chrome or scrolling around the
    /// library's detail content. Rendering `&TraceEventDetail` directly uses the
    /// same text without applying any scroll offset.
    pub fn text(&self) -> Text<'static> {
        let mut lines = vec![
            heading("Event"),
            detail_line("time", self.event.timestamp.to_rfc3339()),
            detail_line("level", self.event.level.0.to_string()),
            detail_line("target", self.event.target.clone()),
            metadata_line("  module", self.event.module_path.as_deref()),
            location_line("  location", self.event.file.as_deref(), self.event.line),
            detail_line(
                "message",
                self.event
                    .fields
                    .get("message")
                    .map(ToString::to_string)
                    .unwrap_or_else(|| "<none>".to_owned()),
            ),
        ];

        push_event_fields(&mut lines, &self.event.fields);
        push_span_stack(&mut lines, &self.span_stack);

        lines.into()
    }
}

impl Widget for &TraceEventDetail {
    fn render(self, area: Rect, buf: &mut Buffer) {
        Paragraph::new(self.text()).render(area, buf);
    }
}

/// Span detail for an event span stack.
///
/// A span detail always keeps the referenced span id. The full span record is
/// present when it was retained in the same snapshot as the event.
#[derive(Clone, Debug)]
pub struct TraceSpanDetail {
    id: SpanId,
    record: Option<Box<SpanRecord>>,
}

impl TraceSpanDetail {
    /// Create detail for a retained span record.
    pub fn present(record: SpanRecord) -> Self {
        Self {
            id: record.id,
            record: Some(Box::new(record)),
        }
    }

    /// Create detail for a span id whose span record is not available.
    pub fn missing(id: SpanId) -> Self {
        Self { id, record: None }
    }

    /// Return the span id referenced by the event.
    pub fn id(&self) -> SpanId {
        self.id
    }

    /// Return the retained span record, if it is available.
    pub fn record(&self) -> Option<&SpanRecord> {
        self.record.as_deref()
    }
}

fn push_fields(lines: &mut Vec<Line<'static>>, heading: &'static str, fields: &FieldMap) {
    lines.push(heading_line(heading));
    if fields.is_empty() {
        lines.push(muted_line("  <none>"));
        return;
    }

    for (name, value) in fields {
        lines.push(detail_line(name, value.to_string()));
    }
}

fn push_event_fields(lines: &mut Vec<Line<'static>>, fields: &FieldMap) {
    lines.push(heading_line("Fields"));
    let mut pushed = false;
    for (name, value) in fields {
        if name == "message" {
            continue;
        }
        lines.push(detail_line(name, value.to_string()));
        pushed = true;
    }

    if !pushed {
        lines.push(muted_line("  <none>"));
    }
}

fn push_span_stack(lines: &mut Vec<Line<'static>>, span_stack: &[TraceSpanDetail]) {
    lines.push(heading("Span stack"));
    if span_stack.is_empty() {
        lines.push(muted_line("  <none>"));
        return;
    }

    for (index, detail) in span_stack.iter().enumerate() {
        if let Some(span) = detail.record() {
            push_span(lines, index, span);
        } else {
            lines.push(muted_line(format!(
                "  {index}. <missing span {}>",
                detail.id()
            )));
        }
    }
}

fn push_span(lines: &mut Vec<Line<'static>>, index: usize, span: &SpanRecord) {
    lines.push(Line::from(vec![
        Span::styled(
            format!("  {index}. "),
            Style::default().add_modifier(Modifier::DIM),
        ),
        Span::styled(
            span.name.clone(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        ),
    ]));
    lines.push(indented_detail_line("id", span.id.to_string()));
    lines.push(indented_detail_line("level", span.level.0.to_string()));
    lines.push(indented_detail_line("target", span.target.clone()));
    lines.push(metadata_line("     module", span.module_path.as_deref()));
    lines.push(location_line(
        "     location",
        span.file.as_deref(),
        span.line,
    ));
    lines.push(indented_detail_line(
        "lifecycle",
        if span.close_time.is_some() {
            "closed"
        } else {
            "open"
        },
    ));

    if let Some(timing) = span.timing {
        lines.push(indented_detail_line(
            "timing",
            format!(
                "state={:?} busy={:?} idle={:?} total={:?} enters={} exits={}",
                timing.state(),
                timing.busy_duration(),
                timing.idle_duration(),
                timing.total_duration(),
                timing.enter_count(),
                timing.exit_count()
            ),
        ));
    }
    push_fields(lines, "     fields", &span.fields);
}

fn heading(label: &'static str) -> Line<'static> {
    Line::from(label).style(
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD),
    )
}

fn heading_line(label: &'static str) -> Line<'static> {
    Line::from(label).style(Style::default().fg(Color::Yellow))
}

fn detail_line(label: impl Into<String>, value: impl Into<String>) -> Line<'static> {
    field_line("  ", label, value)
}

fn indented_detail_line(label: impl Into<String>, value: impl Into<String>) -> Line<'static> {
    field_line("     ", label, value)
}

fn field_line(
    prefix: &'static str,
    label: impl Into<String>,
    value: impl Into<String>,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(
            format!("{prefix}{}:", label.into()),
            Style::default().fg(Color::Blue),
        ),
        Span::raw(" "),
        Span::raw(value.into()),
    ])
}

fn muted_line(text: impl Into<String>) -> Line<'static> {
    Line::from(text.into()).style(Style::default().add_modifier(Modifier::DIM))
}

fn metadata_line(label: &'static str, value: Option<&str>) -> Line<'static> {
    labeled_line(label, value.unwrap_or("<unknown>").to_owned())
}

fn location_line(label: &'static str, file: Option<&str>, line: Option<u32>) -> Line<'static> {
    let value = match (file, line) {
        (Some(file), Some(line)) => format!("{file}:{line}"),
        (Some(file), None) => file.to_owned(),
        (None, Some(line)) => format!("<unknown>:{line}"),
        (None, None) => "<unknown>".to_owned(),
    };
    labeled_line(label, value)
}

fn labeled_line(label: &'static str, value: impl Into<String>) -> Line<'static> {
    let prefix_len = label.len() - label.trim_start().len();
    let (prefix, label) = label.split_at(prefix_len);
    field_line(prefix, label, value)
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
