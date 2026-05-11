//! Ratatui trace viewer.
//!
//! [`TraceViewer`] is the high-level [`ratatui`] integration point. It owns
//! display-time state such as filters and scrollback, and renders through
//! `impl Widget for &mut TraceViewer` so applications do not need
//! [`ratatui::widgets::StatefulWidget`].
//!
//! # Common Workflow
//!
//! Store a [`TraceViewer`] in application state, mutate it from input handlers, and
//! render `&mut viewer` in the area assigned to trace output.
//!
//! # Lifecycle And Side Effects
//!
//! Rendering mutates scroll state because follow-tail and page movement depend on
//! the [`ratatui::layout::Rect`] height. The widget does not install subscribers,
//! write files, spawn tasks, or manage terminal state.
//!
//! # Related Modules
//!
//! - [`crate::store`] retains the records rendered by the viewer.
//! - [`crate::filter`] owns display-time matching.
//! - [`crate::record`] and [`crate::field`] define the data shown in rows and selected-event
//!   detail.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Widget};

use crate::field::FieldMap;
use crate::filter::TraceFilter;
use crate::format::{FormatOptions, event_line};
use crate::record::{EventId, EventRecord, SpanId, SpanRecord};
use crate::store::{TraceSnapshot, TraceStore, TraceStoreStatus};

/// Event-stream trace viewer for [`ratatui`] applications.
///
/// The viewer does not install tracing subscribers and does not own capture policy.
/// It renders retained records from a [`TraceStore`] using display-time filters.
///
/// `TraceViewer` is cheap to clone, but clones have independent filter, scroll,
/// formatting, and selection state. Clones share the same underlying store.
///
/// ```
/// use ratatui::Terminal;
/// use ratatui::backend::TestBackend;
/// use tracing::subscriber;
/// use tracing_subscriber::Registry;
/// use tracing_subscriber::layer::SubscriberExt;
/// use tui_tracing::{TraceLayer, TraceViewer};
///
/// let (layer, store) = TraceLayer::new();
/// let subscriber = Registry::default().with(layer);
///
/// subscriber::with_default(subscriber, || {
///     tracing::info!("ready");
/// });
///
/// let mut viewer = TraceViewer::new(store);
/// let backend = TestBackend::new(80, 3);
/// let mut terminal = Terminal::new(backend).unwrap();
/// terminal
///     .draw(|frame| frame.render_widget(&mut viewer, frame.area()))
///     .unwrap();
///
/// assert_eq!(viewer.status().visible_events, 1);
/// ```
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
    ///
    /// New viewers follow the tail, show all retained events, use compact default
    /// formatting, and have no selected event.
    ///
    /// ```
    /// use tui_tracing::{TraceStore, TraceViewer};
    ///
    /// let store = TraceStore::default();
    /// let viewer = TraceViewer::new(store);
    ///
    /// assert_eq!(viewer.status().visible_events, 0);
    /// ```
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
    ///
    /// This does not change retained records. If the current selection is no
    /// longer visible through the new filter, selection is cleared.
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
    ///
    /// The returned event is cloned from a fresh store snapshot. It may be stale
    /// immediately if capture continues concurrently.
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
    ///
    /// "First" means oldest in the filtered event stream.
    pub fn select_first(&mut self) {
        let snapshot = self.store.snapshot();
        self.selected_event_id = self.visible_events(&snapshot).first().map(|event| event.id);
    }

    /// Select the last visible event if there is one.
    ///
    /// "Last" means newest in the filtered event stream.
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

    /// Scroll enough to keep the selected event visible.
    ///
    /// This uses the height from the most recent render. Before the first render,
    /// the viewer assumes a one-line viewport. Calling this leaves follow-tail mode
    /// unless the selected event is already visible within the current viewport.
    pub fn reveal_selection(&mut self) {
        let snapshot = self.store.snapshot();
        let Some(selected_index) = self.selected_visible_index(&snapshot) else {
            return;
        };
        let selected_index = u16::try_from(selected_index).unwrap_or(u16::MAX);
        let viewport_height = self.page_scroll_lines();
        let viewport_bottom = self.scroll_top.saturating_add(viewport_height);

        if selected_index < self.scroll_top {
            self.follow_tail = false;
            self.scroll_top = selected_index;
        } else if selected_index >= viewport_bottom {
            self.follow_tail = false;
            self.scroll_top = selected_index.saturating_sub(viewport_height.saturating_sub(1));
        }
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
    ///
    /// Formatting options affect compact event rows only. They do not change
    /// capture, filtering, selection, or selected-event detail content.
    pub fn set_format_options(&mut self, format: FormatOptions) {
        self.format = format;
    }

    /// Return whether compact event rows include span context.
    ///
    /// Span context is hidden by default because it can dominate compact rows.
    /// Selected-event detail still includes the full span stack regardless of this
    /// setting.
    pub fn show_span_context(&self) -> bool {
        self.format.show_span_context
    }

    /// Set whether compact event rows include span context.
    ///
    /// This updates display formatting only. It does not change captured records,
    /// active filters, selection, or scroll position.
    pub fn set_show_span_context(&mut self, show: bool) {
        self.format.show_span_context = show;
    }

    /// Toggle span context in compact event rows and return the new value.
    ///
    /// This is a convenience for application keybindings. Selected-event detail
    /// remains complete whether compact rows show span context or not.
    pub fn toggle_span_context(&mut self) -> bool {
        self.format.show_span_context = !self.format.show_span_context;
        self.format.show_span_context
    }

    /// Return whether compact event rows include source locations.
    ///
    /// Source locations are hidden by default because they are often too wide for
    /// the main event stream. Selected-event detail still includes captured source
    /// location metadata regardless of this setting.
    pub fn show_source_locations(&self) -> bool {
        self.format.show_location
    }

    /// Set whether compact event rows include source locations.
    ///
    /// This updates display formatting only. It does not change captured records,
    /// active filters, selection, or scroll position.
    pub fn set_show_source_locations(&mut self, show: bool) {
        self.format.show_location = show;
    }

    /// Toggle source locations in compact event rows and return the new value.
    ///
    /// This is a convenience for application keybindings. Selected-event detail
    /// remains complete whether compact rows show source locations or not.
    pub fn toggle_source_locations(&mut self) -> bool {
        self.format.show_location = !self.format.show_location;
        self.format.show_location
    }

    /// Keep the newest visible event pinned to the bottom of the rendered area.
    pub fn follow_tail(&mut self) {
        self.follow_tail = true;
        self.scroll_top = self.last_tail_scroll;
    }

    /// Scroll older by `lines`.
    ///
    /// Calling this leaves follow-tail mode. The final scroll offset is clamped
    /// during the next render, when the viewer knows the current viewport height.
    pub fn scroll_up(&mut self, lines: u16) {
        if self.follow_tail {
            self.scroll_top = self.last_tail_scroll;
        }
        self.follow_tail = false;
        self.scroll_top = self.scroll_top.saturating_sub(lines);
    }

    /// Scroll newer by `lines`, returning to follow mode at the bottom.
    ///
    /// The bottom is based on the most recently rendered viewport. If no render
    /// has happened yet, the stored tail offset is zero.
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

    fn visible_text(&self, snapshot: &TraceSnapshot, width: u16) -> Text<'static> {
        let selected_event_id = self.selected_event_id;
        self.visible_events(snapshot)
            .into_iter()
            .map(|event| {
                let line = event_line(event, snapshot, &self.format, width);
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
        let text = self.visible_text(&snapshot, area.width);
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
///
/// `TraceEventDetail` is independent from the store after construction. Rendering
/// it does not mutate viewer state.
///
/// ```
/// use tracing::subscriber;
/// use tracing_subscriber::Registry;
/// use tracing_subscriber::layer::SubscriberExt;
/// use tui_tracing::{TraceLayer, TraceViewer};
///
/// let (layer, store) = TraceLayer::new();
/// let subscriber = Registry::default().with(layer);
///
/// subscriber::with_default(subscriber, || {
///     tracing::warn!(code = 503, "retrying request");
/// });
///
/// let mut viewer = TraceViewer::new(store);
/// viewer.select_first();
///
/// let detail = viewer.selected_detail().expect("selected event has detail");
/// let text = detail.text().to_string();
/// assert!(text.contains("retrying request"));
/// assert!(text.contains("code"));
/// ```
#[derive(Clone, Debug)]
pub struct TraceEventDetail {
    event: EventRecord,
    span_stack: Vec<TraceSpanDetail>,
}

impl TraceEventDetail {
    /// Create event detail from an event and already-resolved span stack.
    ///
    /// Most applications should call [`TraceViewer::selected_detail`] instead so
    /// the span stack is resolved from one store snapshot.
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

    /// Format this detail as [`ratatui::text::Text`].
    ///
    /// This is useful when callers need app-owned chrome or scrolling around the
    /// library's detail content. Rendering `&TraceEventDetail` directly uses the
    /// same text without applying any scroll offset.
    pub fn text(&self) -> Text<'static> {
        let mut lines = vec![
            heading("Event"),
            metadata_line("time", self.event.timestamp.to_rfc3339()),
            metadata_line("level", self.event.level.0.to_string()),
            metadata_line("target", self.event.target.clone()),
            optional_metadata_line("  module", self.event.module_path.as_deref()),
            location_line("  location", self.event.file.as_deref(), self.event.line),
            message_line(
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
///
/// Missing span records are expected when a detail is synthesized by an
/// application or when future retention policies keep events without the full span
/// metadata they reference.
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
    lines.push(span_fields_heading(heading));
    if fields.is_empty() {
        lines.push(muted_line("        <none>"));
        return;
    }

    for (name, value) in fields {
        lines.push(span_field_line(name, value.to_string()));
    }
}

fn push_event_fields(lines: &mut Vec<Line<'static>>, fields: &FieldMap) {
    lines.push(event_fields_heading("  fields"));
    let mut pushed = false;
    for (name, value) in fields {
        if name == "message" {
            continue;
        }
        lines.push(event_field_line(name, value.to_string()));
        pushed = true;
    }

    if !pushed {
        lines.push(muted_line("     <none>"));
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
        Span::styled(format!("  {index}. "), span_index_style()),
        Span::styled(span.name.clone(), span_name_style()),
    ]));
    lines.push(span_metadata_line("id", span.id.to_string()));
    lines.push(span_metadata_line("level", span.level.0.to_string()));
    lines.push(span_metadata_line("target", span.target.clone()));
    lines.push(optional_span_metadata_line(
        "     module",
        span.module_path.as_deref(),
    ));
    lines.push(location_line(
        "     location",
        span.file.as_deref(),
        span.line,
    ));
    lines.push(span_metadata_line(
        "lifecycle",
        if span.close_time.is_some() {
            "closed"
        } else {
            "open"
        },
    ));

    if let Some(timing) = span.timing {
        lines.push(span_metadata_line(
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
    Line::from(label).style(section_heading_style())
}

fn event_fields_heading(label: &'static str) -> Line<'static> {
    Line::from(label).style(subsection_heading_style())
}

fn span_fields_heading(label: &'static str) -> Line<'static> {
    Line::from(label).style(subsection_heading_style())
}

fn metadata_line(label: impl Into<String>, value: impl Into<String>) -> Line<'static> {
    field_line(
        "  ",
        label,
        value,
        metadata_label_style(),
        metadata_value_style(),
    )
}

fn optional_metadata_line(label: &'static str, value: Option<&str>) -> Line<'static> {
    labeled_line(
        label,
        value.unwrap_or("<unknown>").to_owned(),
        metadata_label_style(),
        metadata_value_style_for(value),
    )
}

fn message_line(label: impl Into<String>, value: impl Into<String>) -> Line<'static> {
    let value = value.into();
    let value_style = if value == "<none>" {
        missing_value_style()
    } else {
        value_style()
    };
    field_line("  ", label, value, message_label_style(), value_style)
}

fn event_field_line(label: impl Into<String>, value: impl Into<String>) -> Line<'static> {
    field_line(
        "     ",
        label,
        value,
        event_field_label_style(),
        value_style(),
    )
}

fn span_metadata_line(label: impl Into<String>, value: impl Into<String>) -> Line<'static> {
    field_line(
        "     ",
        label,
        value,
        metadata_label_style(),
        metadata_value_style(),
    )
}

fn optional_span_metadata_line(label: &'static str, value: Option<&str>) -> Line<'static> {
    labeled_line(
        label,
        value.unwrap_or("<unknown>").to_owned(),
        metadata_label_style(),
        metadata_value_style_for(value),
    )
}

fn span_field_line(label: impl Into<String>, value: impl Into<String>) -> Line<'static> {
    field_line(
        "        ",
        label,
        value,
        span_field_label_style(),
        value_style(),
    )
}

fn field_line(
    prefix: &'static str,
    label: impl Into<String>,
    value: impl Into<String>,
    label_style: Style,
    value_style: Style,
) -> Line<'static> {
    Line::from(vec![
        Span::styled(format!("{prefix}{}:", label.into()), label_style),
        Span::raw(" "),
        Span::styled(value.into(), value_style),
    ])
}

fn muted_line(text: impl Into<String>) -> Line<'static> {
    Line::from(text.into()).style(missing_value_style())
}

fn location_line(label: &'static str, file: Option<&str>, line: Option<u32>) -> Line<'static> {
    let value = match (file, line) {
        (Some(file), Some(line)) => format!("{file}:{line}"),
        (Some(file), None) => file.to_owned(),
        (None, Some(line)) => format!("<unknown>:{line}"),
        (None, None) => "<unknown>".to_owned(),
    };
    let style = if file.is_some() {
        metadata_value_style()
    } else {
        missing_value_style()
    };
    labeled_line(label, value, metadata_label_style(), style)
}

fn labeled_line(
    label: &'static str,
    value: impl Into<String>,
    label_style: Style,
    value_style: Style,
) -> Line<'static> {
    let prefix_len = label.len() - label.trim_start().len();
    let (prefix, label) = label.split_at(prefix_len);
    field_line(prefix, label, value, label_style, value_style)
}

fn section_heading_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
}

fn subsection_heading_style() -> Style {
    Style::default()
        .fg(Color::Gray)
        .add_modifier(Modifier::BOLD)
}

fn metadata_label_style() -> Style {
    Style::default().fg(Color::Gray)
}

fn metadata_value_style() -> Style {
    Style::default()
}

fn metadata_value_style_for(value: Option<&str>) -> Style {
    if value.is_some() {
        metadata_value_style()
    } else {
        missing_value_style()
    }
}

fn message_label_style() -> Style {
    Style::default().fg(Color::Gray)
}

fn event_field_label_style() -> Style {
    Style::default().fg(Color::Gray)
}

fn span_field_label_style() -> Style {
    Style::default().fg(Color::Gray)
}

fn value_style() -> Style {
    Style::default()
}

fn missing_value_style() -> Style {
    Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::DIM)
}

fn span_index_style() -> Style {
    Style::default()
        .fg(Color::DarkGray)
        .add_modifier(Modifier::DIM)
}

fn span_name_style() -> Style {
    Style::default()
        .fg(Color::Cyan)
        .add_modifier(Modifier::BOLD)
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
    ///
    /// Applications can show this directly in a status bar or use it to decide
    /// whether new events should be visually emphasized.
    pub scroll_mode: TraceScrollMode,

    /// First visible row offset from the top of the filtered event stream.
    ///
    /// This is updated during rendering and is measured in rendered rows, not
    /// event ids.
    pub scroll_top: u16,

    /// Last computed tail offset for the filtered event stream.
    ///
    /// This value is updated during rendering because it depends on the render
    /// area height. Before the first render it is zero.
    pub tail_scroll: u16,

    /// Number of retained events accepted by the active display filter.
    ///
    /// This is computed from a fresh store snapshot when status is requested.
    pub visible_events: usize,

    /// Number of retained events hidden by the active display filter.
    ///
    /// This does not include events evicted or dropped by store retention.
    pub hidden_events: usize,

    /// Selected event position in the filtered visible event stream.
    ///
    /// The value is zero-based. `None` means no selected event is currently
    /// visible.
    pub selected_visible_index: Option<usize>,

    /// Selected event identifier, if the selected event is still visible.
    ///
    /// This can be `None` even when the viewer has visible events.
    pub selected_event_id: Option<EventId>,

    /// Active display filter.
    ///
    /// This clone lets applications show filter state without borrowing the
    /// viewer.
    pub filter: TraceFilter,

    /// Storage status for retained records and storage-level event loss.
    pub store: TraceStoreStatus,
}
