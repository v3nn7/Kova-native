//! Data display: tables, pagination and breadcrumbs.

use super::focus_ring;
use crate::element::{AnyElement, Interactive, IntoElement};
use crate::elements::{Div, column, div, dynamic, icon, keyed, row, text};
use crate::icons;
use crate::style::Styled;
use crate::theme::theme;
use kova_native_core::{DurationExt, SharedString, Signal, memo, signal};
use kova_native_layout::Track;
use kova_native_text::FontWeight;
use std::cmp::Ordering;
use std::rc::Rc;

type Compare<T> = Rc<dyn Fn(&T, &T) -> Ordering>;
type Cell<T> = Rc<dyn Fn(&T) -> AnyElement>;
type RowFn<T> = Rc<dyn Fn(&T)>;
type Predicate<T> = Rc<dyn Fn(&T) -> bool>;

/// Sort direction of a [`DataTable`] column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortDirection {
    Ascending,
    Descending,
}

/// A column of a [`DataTable`]: header, width, cell renderer, optional sorting.
pub struct TableColumn<T> {
    title: SharedString,
    width: Track,
    align_end: bool,
    render: Cell<T>,
    compare: Option<Compare<T>>,
}

/// Creates a column. `render` builds the cell for a row.
pub fn table_column<T: 'static, E: IntoElement>(
    title: impl Into<SharedString>,
    render: impl Fn(&T) -> E + 'static,
) -> TableColumn<T> {
    TableColumn {
        title: title.into(),
        width: Track::Fr(1.0),
        align_end: false,
        render: Rc::new(move |row| render(row).into_any()),
        compare: None,
    }
}

impl<T: 'static> TableColumn<T> {
    /// A fixed width in logical px.
    pub fn width(mut self, px: f32) -> Self {
        self.width = Track::Px(px);
        self
    }

    /// A share of the remaining width (default 1).
    pub fn grow(mut self, fr: f32) -> Self {
        self.width = Track::Fr(fr);
        self
    }

    /// Any track definition.
    pub fn track(mut self, track: Track) -> Self {
        self.width = track;
        self
    }

    /// Right-aligns header and cells (numbers, actions).
    pub fn align_end(mut self) -> Self {
        self.align_end = true;
        self
    }

    /// Makes the column sortable by a key; clicking the header cycles
    /// ascending → descending → unsorted.
    pub fn sortable_by<K: Ord>(mut self, key: impl Fn(&T) -> K + 'static) -> Self {
        self.compare = Some(Rc::new(move |a, b| key(a).cmp(&key(b))));
        self
    }
}

/// A dense, sortable table with keyed rows. See [`data_table`].
pub struct DataTable<T: 'static> {
    rows: Rc<dyn Fn() -> Vec<T>>,
    key: Rc<dyn Fn(&T) -> u64>,
    columns: Vec<TableColumn<T>>,
    on_row_click: Option<RowFn<T>>,
    active: Option<Predicate<T>>,
    empty: Option<Rc<dyn Fn() -> AnyElement>>,
    sort: Option<Signal<Option<(usize, SortDirection)>>>,
    max_height: Option<f32>,
    dense: bool,
}

/// Creates a table over `rows` (re-read when the signals it reads change).
/// `key` identifies a row across updates: rows with the same key keep their
/// cells, so sorting and filtering only move nodes. Cells render once per
/// key; put changing per-row values in signals, or change the key.
///
/// ```ignore
/// data_table(move || keys.get(), |k| k.id, vec![
///     table_column("Name", |k: &ApiKey| text(k.name.clone())).sortable_by(|k| k.name.clone()),
///     table_column("Created", |k: &ApiKey| text(k.created.clone())).width(140.0),
/// ])
/// .on_row_click(move |k| selected.set(Some(k.id)))
/// ```
pub fn data_table<T: Clone + 'static, K: std::hash::Hash>(
    rows: impl Fn() -> Vec<T> + 'static,
    key: impl Fn(&T) -> K + 'static,
    columns: Vec<TableColumn<T>>,
) -> DataTable<T> {
    use std::hash::BuildHasher;
    let hasher = std::hash::BuildHasherDefault::<rustc_hash::FxHasher>::default();
    DataTable {
        rows: Rc::new(rows),
        key: Rc::new(move |row| hasher.hash_one(key(row))),
        columns,
        on_row_click: None,
        active: None,
        empty: None,
        sort: None,
        max_height: None,
        dense: false,
    }
}

impl<T: Clone + 'static> DataTable<T> {
    /// Called when a row is clicked (or activated with Enter).
    pub fn on_row_click(mut self, f: impl Fn(&T) + 'static) -> Self {
        self.on_row_click = Some(Rc::new(f));
        self
    }

    /// Highlights rows for which `f` returns true; may read signals.
    pub fn row_active(mut self, f: impl Fn(&T) -> bool + 'static) -> Self {
        self.active = Some(Rc::new(f));
        self
    }

    /// Shown instead of the body when there are no rows.
    pub fn empty_state<E: IntoElement>(mut self, f: impl Fn() -> E + 'static) -> Self {
        self.empty = Some(Rc::new(move || f().into_any()));
        self
    }

    /// External sort state (column index and direction), e.g. to persist it.
    pub fn sort(mut self, sort: Signal<Option<(usize, SortDirection)>>) -> Self {
        self.sort = Some(sort);
        self
    }

    /// Scrolls the body (header stays visible) beyond this height.
    pub fn max_height(mut self, px: f32) -> Self {
        self.max_height = Some(px);
        self
    }

    /// Tighter rows.
    pub fn dense(mut self) -> Self {
        self.dense = true;
        self
    }
}

impl<T: Clone + 'static> IntoElement for DataTable<T> {
    fn into_any(self) -> AnyElement {
        let t = theme();
        let DataTable {
            rows,
            key,
            columns,
            on_row_click,
            active,
            empty,
            sort,
            max_height,
            dense,
        } = self;
        let sort = sort.unwrap_or_else(|| signal(None));
        let tracks: Vec<Track> = columns.iter().map(|c| c.width).collect();
        let columns = Rc::new(columns);
        let py = if dense { 5.0 } else { 8.0 };
        let ring = focus_ring(&t);

        let header = div()
            .grid_template_columns(tracks.clone())
            .gap_x(12.0)
            .px(12.0)
            .py(7.0)
            .bg(t.surface_sunken)
            .border_b(1.0)
            .border_color(t.border)
            .text_size(11.5)
            .font_weight(FontWeight::Semibold)
            .text_color(t.text_muted)
            .children(columns.iter().enumerate().map(|(i, c)| {
                let mut cell = row()
                    .gap(4.0)
                    .min_w(0.0)
                    .child(text(c.title.clone()).whitespace_nowrap());
                if c.align_end {
                    cell = cell.justify_end();
                }
                if c.compare.is_some() {
                    let (text_c, subtle) = (t.text, t.text_subtle);
                    cell = cell
                        .id(("table-sort", i))
                        .cursor_pointer()
                        .hover(move |s| s.text_color(text_c))
                        .on_click(move |_| {
                            sort.update(|s| {
                                *s = match *s {
                                    Some((c, SortDirection::Ascending)) if c == i => {
                                        Some((i, SortDirection::Descending))
                                    }
                                    Some((c, SortDirection::Descending)) if c == i => None,
                                    _ => Some((i, SortDirection::Ascending)),
                                }
                            })
                        })
                        .child(dynamic(move || {
                            let glyph = match sort.get() {
                                Some((c, SortDirection::Ascending)) if c == i => icons::SORT_ASC,
                                Some((c, SortDirection::Descending)) if c == i => icons::SORT_DESC,
                                _ => icons::SORT,
                            };
                            icon(glyph).size(12.0).color(subtle)
                        }));
                }
                cell
            }));

        let sorted_rows = {
            let (rows, columns) = (rows.clone(), columns.clone());
            move || {
                let mut list = rows();
                if let Some((index, direction)) = sort.get()
                    && let Some(compare) = columns.get(index).and_then(|c| c.compare.clone())
                {
                    list.sort_by(|a, b| match direction {
                        SortDirection::Ascending => compare(a, b),
                        SortDirection::Descending => compare(b, a),
                    });
                }
                list
            }
        };
        let (hover_bg, active_bg, border) = (t.surface_raised, t.accent_soft, t.border);
        let render_row = move |item: T| -> Div {
            let mut line = div()
                .grid_template_columns(tracks.clone())
                .gap_x(12.0)
                .items_center()
                .px(12.0)
                .py(py)
                .border_b(1.0)
                .border_color(border)
                .hover(move |s| s.bg(hover_bg))
                .transition(90.ms())
                .children(columns.iter().map(|c| {
                    let cell = row().min_w(0.0).overflow_hidden().child((c.render)(&item));
                    if c.align_end {
                        cell.justify_end()
                    } else {
                        cell
                    }
                }));
            if let Some(active) = &active {
                let (active, item) = (active.clone(), item.clone());
                line = line.bind(move |s| if active(&item) { s.bg(active_bg) } else { s });
            }
            if let Some(click) = &on_row_click {
                let (click, item) = (click.clone(), item.clone());
                line = line
                    .cursor_pointer()
                    .focusable()
                    .focus_visible(move |s| s.shadow(ring.inset()))
                    .on_click(move |_| click(&item));
            }
            line
        };
        let is_empty = {
            let rows = rows.clone();
            memo(move || rows().is_empty())
        };
        let key_fn = key.clone();
        let body = dynamic(move || {
            if is_empty.get()
                && let Some(empty) = &empty
            {
                return empty();
            }
            let (sorted_rows, key_fn, render_row) =
                (sorted_rows.clone(), key_fn.clone(), render_row.clone());
            keyed(sorted_rows, move |r| key_fn(r), render_row).into_any()
        })
        .flex_col();
        let body = match max_height {
            Some(h) => body.max_h(h).overflow_y_scroll(),
            None => body,
        };
        column()
            .w_full()
            .border(1.0)
            .border_color(t.border)
            .rounded(t.radius)
            .overflow_hidden()
            .bg(t.surface)
            .text_size(13.0)
            .child(header)
            .child(body)
            .into_any()
    }
}

/// Page navigation for `page` (zero-based) out of `page_count()` pages:
/// previous/next buttons and a compact window of page numbers.
pub fn pagination(page: Signal<usize>, page_count: impl Fn() -> usize + 'static) -> Div {
    let t = theme();
    let (raised, accent_soft, accent, text_c, muted) = (
        t.surface_raised,
        t.accent_soft,
        t.accent_hover,
        t.text,
        t.text_muted,
    );
    let radius = t.radius_small;
    let count = Rc::new(page_count);
    let nav = move |glyph: &'static [u8], delta: isize, count: Rc<dyn Fn() -> usize>| {
        div()
            .size(28.0)
            .center()
            .rounded(radius)
            .text_color(muted)
            .cursor_pointer()
            .hover(move |s| s.bg(raised).text_color(text_c))
            .bind({
                let count = count.clone();
                move |s| {
                    let p = page.get() as isize + delta;
                    s.disabled(p < 0 || p >= count() as isize)
                }
            })
            .disabled_style(|s| s.opacity(0.35))
            .on_click(move |_| {
                let p = page.get_untracked() as isize + delta;
                if p >= 0 && p < count() as isize {
                    page.set(p as usize);
                }
            })
            .child(icon(glyph).size(15.0))
    };
    let count_dyn: Rc<dyn Fn() -> usize> = count.clone();
    let numbers = dynamic(move || {
        let total = count_dyn().max(1);
        let current = page.get().min(total - 1);
        row()
            .gap(2.0)
            .children(
                page_window(current, total)
                    .into_iter()
                    .map(move |slot| match slot {
                        None => text("…").color(muted).px(6.0).into_any(),
                        Some(p) => {
                            let selected = p == current;
                            div()
                                .min_w(28.0)
                                .h(28.0)
                                .px(6.0)
                                .center()
                                .rounded(radius)
                                .text_size(12.5)
                                .font_weight(FontWeight::Medium)
                                .cursor_pointer()
                                .when(selected, |d| d.bg(accent_soft).text_color(accent))
                                .when(!selected, |d| {
                                    d.text_color(muted)
                                        .hover(move |s| s.bg(raised).text_color(text_c))
                                })
                                .on_click(move |_| page.set(p))
                                .child(text((p + 1).to_string()))
                                .into_any()
                        }
                    }),
            )
    });
    row()
        .gap(2.0)
        .child(nav(icons::CHEVRON_LEFT, -1, count.clone()))
        .child(numbers)
        .child(nav(icons::CHEVRON_RIGHT, 1, count))
}

/// Page numbers to show around `current`: first, last, and current ± 1,
/// with `None` marking elided ranges.
pub fn page_window(current: usize, total: usize) -> Vec<Option<usize>> {
    if total <= 7 {
        return (0..total).map(Some).collect();
    }
    let mut pages = vec![0, total - 1];
    let start = current.saturating_sub(1).max(1);
    let end = (current + 1).min(total - 2);
    pages.extend(start..=end);
    // Keep the window wide near the edges so the control does not jump.
    if current <= 2 {
        pages.extend(1..=4);
    }
    if current + 3 >= total {
        pages.extend(total - 5..total - 1);
    }
    pages.sort_unstable();
    pages.dedup();
    let mut out = Vec::new();
    for (i, p) in pages.iter().enumerate() {
        if i > 0 && *p > pages[i - 1] + 1 {
            out.push(None);
        }
        out.push(Some(*p));
    }
    out
}

/// A path of locations; every crumb but the last is clickable and calls
/// `on_select` with its index.
pub fn breadcrumbs<S: Into<SharedString>>(
    items: impl IntoIterator<Item = S>,
    on_select: impl Fn(usize) + 'static,
) -> Div {
    let t = theme();
    let items: Vec<SharedString> = items.into_iter().map(Into::into).collect();
    let last = items.len().saturating_sub(1);
    let on_select = Rc::new(on_select);
    let (text_c, muted, subtle) = (t.text, t.text_muted, t.text_subtle);
    row()
        .gap(6.0)
        .text_size(13.0)
        .children(items.into_iter().enumerate().map(move |(i, label)| {
            let crumb = if i == last {
                text(label).color(text_c).medium().into_any()
            } else {
                let on_select = on_select.clone();
                text(label)
                    .color(muted)
                    .cursor_pointer()
                    .hover(move |s| s.text_color(text_c))
                    .on_click(move |_| on_select(i))
                    .into_any()
            };
            let mut part = row().gap(6.0).child(crumb);
            if i < last {
                part = part.child(icon(icons::CHEVRON_RIGHT).size(13.0).color(subtle));
            }
            part
        }))
}

#[cfg(test)]
mod tests {
    use super::page_window;

    #[test]
    fn page_window_elides_far_pages() {
        assert_eq!(page_window(0, 3), vec![Some(0), Some(1), Some(2)]);
        assert_eq!(
            page_window(10, 20),
            vec![Some(0), None, Some(9), Some(10), Some(11), None, Some(19)]
        );
        assert_eq!(
            page_window(0, 20),
            vec![Some(0), Some(1), Some(2), Some(3), Some(4), None, Some(19)]
        );
        assert_eq!(
            page_window(19, 20),
            vec![
                Some(0),
                None,
                Some(15),
                Some(16),
                Some(17),
                Some(18),
                Some(19)
            ]
        );
    }
}
