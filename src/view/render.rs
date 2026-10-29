// Copyright (C) 2026 feraxhp
// 
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
// 
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.
// 
// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use texera_core::State;

use crate::traits::{Decoration, GlyphContext, TransformedGlyph};
use crate::view::editor_view::EditorView;
use crate::view::view_state::EditorViewState;
use crate::viewport::metrics::CellMetrics;
use crate::viewport::visual_line::{compute_visual_lines, doc_offset_to_visual_pos, VisualLine, VisualPosition};

/// Style definitions for primary and secondary cursors and text selection.
///
/// # Examples
///
/// ```rust
/// use ratatui::style::{Color, Style};
/// use texera::EditorThemeStyles;
///
/// let theme = EditorThemeStyles {
///     primary_cursor: Style::default().bg(Color::White),
///     secondary_cursor: Style::default().bg(Color::DarkGray),
///     selection: Style::default().bg(Color::Blue),
/// };
/// ```
#[derive(Clone, Copy, Debug)]
pub struct EditorThemeStyles {
    /// Style applied to the active primary cursor.
    pub primary_cursor: Style,
    /// Style applied to secondary cursors (when multiple cursors are active).
    pub secondary_cursor: Style,
    /// Style applied to character ranges in active selections.
    pub selection: Style,
}

impl Default for EditorThemeStyles {
    fn default() -> Self {
        Self {
            primary_cursor: Style::default().add_modifier(Modifier::REVERSED),
            secondary_cursor: Style::default().bg(Color::DarkGray).fg(Color::White),
            selection: Style::default().bg(Color::DarkGray),
        }
    }
}

/// Highlight ranges of one logical line (char offsets relative to the line start).
struct LineHighlights {
    logical: usize,
    /// Document char offset of the start of the logical line.
    line_from: usize,
    ranges: Vec<(std::ops::Range<usize>, Style)>,
}

impl LineHighlights {
    /// Ranges re-based to a visual segment starting at document char offset `segment_from`.
    fn for_segment(&self, segment_from: usize) -> Vec<(std::ops::Range<usize>, Style)> {
        let offset = segment_from.saturating_sub(self.line_from);
        if offset == 0 {
            return self.ranges.clone();
        }
        self.ranges
            .iter()
            .filter(|(range, _)| range.end > offset)
            .map(|(range, style)| (range.start.saturating_sub(offset)..range.end - offset, *style))
            .collect()
    }
}

/// Core rendering pipeline executing all layout, gutter, styling, and cursor composition.
///
/// Renders the editor view and document state directly into the provided Ratatui [`Buffer`].
///
/// # Examples
///
/// ```rust
/// use ratatui::buffer::Buffer;
/// use ratatui::layout::Rect;
/// use texera::{render_editor, EditorView, EditorViewState};
/// use texera_core::State;
///
/// let view = EditorView::new();
/// let state = State::from("hello world");
/// let area = Rect::new(0, 0, 40, 10);
/// let mut buf = Buffer::empty(area);
/// let mut view_state = EditorViewState::new();
///
/// render_editor(&view, &state, area, &mut buf, &mut view_state);
/// ```
pub fn render_editor(
    view: &EditorView,
    state: &State,
    area: Rect,
    buf: &mut Buffer,
    view_state: &mut EditorViewState,
) {
    if area.width == 0 || area.height == 0 {
        return;
    }

    view_state.last_area = Some(area);

    let styles = view.theme_styles;
    let total_doc_lines = state.doc().len_lines();
    let tab_size = state.tab_size();

    // 1. Calculate gutter width
    let gutter_width = if let Some(ref gutter) = view.gutter {
        gutter.width(total_doc_lines).min(area.width)
    } else {
        0
    };

    let text_area_width = area.width.saturating_sub(gutter_width);
    if text_area_width == 0 && gutter_width == 0 {
        return;
    }

    // 2. Compute visual line geometry and autoscroll
    view_state.scroller.update_dimensions(
        area.height as usize,
        text_area_width as usize,
        view.max_lines,
    );

    let primary_head = state.selection().main().head();
    let (visual_lines, visible_rows, scroll_x) = if !view.wrap {
        // Fast O(viewport_height) path for unwrapped mode:
        // In unwrapped mode, visual row index == logical line index.
        let doc = state.doc();
        let primary_line = doc.char_to_line(primary_head);
        let (primary_line_start, primary_line_text) = match doc.line(primary_line) {
            Ok(line) => (line.from(), line.text_trimmed()),
            Err(_) => (0, std::borrow::Cow::Borrowed("")),
        };
        let char_in_line = primary_head.saturating_sub(primary_line_start);
        let visual_col = CellMetrics::char_index_to_column(&primary_line_text, char_in_line, tab_size);

        let cursor_vis = VisualPosition {
            visual_row: primary_line,
            visual_col,
        };
        view_state.scroller.ensure_visible(cursor_vis, false);

        let rows_range = view_state.scroller.visible_rows_range(total_doc_lines);
        let mut vlines = Vec::with_capacity(rows_range.len());
        if !rows_range.is_empty() && rows_range.start < total_doc_lines {
            let start_line = rows_range.start;
            let end_line = (rows_range.end - 1).min(total_doc_lines.saturating_sub(1));
            for line in doc.iter_lines(Some(start_line), Some(end_line)) {
                let text = line.text_trimmed().into_owned();
                let vis_width = CellMetrics::string_width(&text, 0, tab_size);
                vlines.push(VisualLine {
                    logical_line_index: line.number(),
                    is_wrapped_continuation: false,
                    char_range: line.from()..line.to_trimmed(),
                    text,
                    visual_width: vis_width,
                });
            }
        }
        let count = vlines.len();
        (vlines, 0..count, view_state.scroller.scroll_x)
    } else {
        // Full visual line computation when wrapping is enabled
        let full_vlines = compute_visual_lines(
            state.doc(),
            view.wrap,
            text_area_width as usize,
            tab_size,
        );
        let cursor_vis = doc_offset_to_visual_pos(&full_vlines, primary_head, tab_size);
        view_state.scroller.ensure_visible(cursor_vis, true);
        let rows = view_state.scroller.visible_rows_range(full_vlines.len());
        let sx = view_state.scroller.scroll_x;
        (full_vlines, rows, sx)
    };

    // 4. Fill background with base style
    for y in area.top()..area.bottom() {
        for x in area.left()..area.right() {
            buf[(x, y)].set_style(view.base_style);
        }
    }

    // Pre-calculate selection intervals and cursor offsets sorted for fast binary search
    let mut all_cursors: Vec<(usize, bool)> = state.selection().iter()
        .map(|(range, is_primary)| (range.head(), is_primary))
        .collect();
    all_cursors.sort_unstable_by_key(|&(offset, _)| offset);

    let mut selected_intervals: Vec<std::ops::Range<usize>> = Vec::new();
    for (range, _) in state.selection().iter() {
        if !range.empty() {
            selected_intervals.push(range.from()..range.to());
        }
    }
    selected_intervals.sort_unstable_by_key(|r| r.start);

    // 4b. Let the highlighter advance its parse cache just as far as the viewport needs.
    if let Some(ref h) = view.highlighter {
        if visible_rows.start < visible_rows.end {
            let first = visual_lines[visible_rows.start].logical_line_index;
            let last = visual_lines[visible_rows.end - 1].logical_line_index;
            h.prepare_viewport(state, first..=last);
        }
    }

    // Syntax tokens of the logical line being drawn (shared by its wrapped visual rows).
    let mut line_highlights: Option<LineHighlights> = None;

    // 5. Render visible rows
    for (row_offset, vline_idx) in visible_rows.enumerate() {
        let screen_y = area.y + row_offset as u16;
        if screen_y >= area.bottom() {
            break;
        }

        let vline = &visual_lines[vline_idx];

        // 5.1. Render Gutter Column
        if gutter_width > 0 {
            if let Some(ref gutter) = view.gutter {
                let gutter_line = gutter.render_line(
                    vline.logical_line_index + 1,
                    total_doc_lines,
                    vline.is_wrapped_continuation,
                );

                let mut g_x = area.x;
                for span in &gutter_line.spans {
                    for ch in span.content.chars() {
                        if g_x < area.x + gutter_width {
                            buf[(g_x, screen_y)].set_char(ch).set_style(span.style);
                            g_x += 1;
                        }
                    }
                }
            }
        }

        // 5.2. Collect syntax highlights for current visual line
        let syntax_highlights = if let Some(ref h) = view.highlighter {
            let logical = vline.logical_line_index;
            if line_highlights.as_ref().map(|l| l.logical) != Some(logical) {
                line_highlights = Some(if view.wrap {
                    // Wrapped rows are slices of the logical line: lex the whole line once.
                    match state.doc().line(logical) {
                        Ok(line) => LineHighlights {
                            logical,
                            line_from: line.from(),
                            ranges: h.highlight_document_line(state, logical, &line.text_trimmed()),
                        },
                        Err(_) => LineHighlights { logical, line_from: vline.char_range.start, ranges: Vec::new() },
                    }
                } else {
                    LineHighlights {
                        logical,
                        line_from: vline.char_range.start,
                        ranges: h.highlight_document_line(state, logical, &vline.text),
                    }
                });
            }
            line_highlights
                .as_ref()
                .map(|l| l.for_segment(vline.char_range.start))
                .unwrap_or_default()
        } else {
            Vec::new()
        };

        // 5.3. Collect custom decorations for current visual line
        let mut custom_decorations = Vec::new();
        for provider in &view.decoration_providers {
            let set = provider.decorations(state, vline.char_range.clone());
            for range in set.iter() {
                custom_decorations.push((range.from..range.to, range.value.clone()));
            }
        }

        // 5.4. Render characters across available text columns
        let text_start_x = area.x + gutter_width;
        let mut visual_col = 0;
        let mut line_cursor_cols = Vec::new();

        // Fast binary search for cursors falling inside this visual line
        let start_idx = all_cursors.partition_point(|&(c, _)| c < vline.char_range.start);
        let end_idx = all_cursors.partition_point(|&(c, _)| c <= vline.char_range.end);
        for &(c_offset, is_primary) in &all_cursors[start_idx..end_idx] {
            let char_in_seg = c_offset - vline.char_range.start;
            let c_col = CellMetrics::char_index_to_column(&vline.text, char_in_seg, tab_size);
            line_cursor_cols.push((c_col, is_primary));
        }

        // Fast binary search/filter for selection intervals overlapping current line
        let sel_start = selected_intervals.partition_point(|inv| inv.end <= vline.char_range.start);
        let mut line_selections: Vec<&std::ops::Range<usize>> = Vec::new();
        for inv in &selected_intervals[sel_start..] {
            if inv.start >= vline.char_range.end {
                break;
            }
            line_selections.push(inv);
        }

        for (char_idx, ch) in vline.text.chars().enumerate() {
            let cell_w = CellMetrics::char_width(ch, visual_col, tab_size);
            let doc_char_offset = vline.char_range.start + char_idx;

            // Compute style hierarchy
            let mut style = view.base_style;

            // Layer 1: Syntax Highlighting
            for (range, syn_style) in &syntax_highlights {
                if char_idx >= range.start && char_idx < range.end {
                    style = style.patch(*syn_style);
                }
            }

            // Layer 2: Custom Decorations
            for (range, dec) in &custom_decorations {
                if doc_char_offset >= range.start && doc_char_offset < range.end {
                    match dec {
                        Decoration::Mark(dec_style) | Decoration::Line(dec_style) => {
                            style = style.patch(*dec_style);
                        }
                        Decoration::Widget(_, dec_style) => {
                            style = style.patch(*dec_style);
                        }
                    }
                }
            }

            // Layer 2.5: Glyph Transformation
            let mut transformed_glyph = None;
            if !view.glyph_transformers.is_empty() {
                let ctx = GlyphContext {
                    ch,
                    cell_width: cell_w,
                    visual_col,
                    doc_char_offset,
                    line_index: vline.logical_line_index,
                    current_style: style,
                    state,
                };
                for t in &view.glyph_transformers {
                    if let Some(res) = t.transform(&ctx) {
                        transformed_glyph = Some(res);
                        break;
                    }
                }
            }

            if let Some(ref tr) = transformed_glyph {
                let opt_style = match tr {
                    TransformedGlyph::Char(_, s) => *s,
                    TransformedGlyph::Tab { style: s, .. } => *s,
                    TransformedGlyph::Cells(_, s) => *s,
                };
                if let Some(s) = opt_style {
                    style = style.patch(s);
                }
            }

            // Layer 3: Text Selection (checked only against intervals overlapping this line)
            let is_selected = line_selections
                .iter()
                .any(|inv| doc_char_offset >= inv.start && doc_char_offset < inv.end);
            if is_selected {
                style = style.patch(styles.selection);
            }

            // Layer 4: Multi-cursor (solid block cursor occupying the character cell)
            for &(c_col, is_primary) in &line_cursor_cols {
                if visual_col <= c_col && c_col < visual_col + cell_w {
                    let cursor_style = if is_primary {
                        styles.primary_cursor
                    } else {
                        styles.secondary_cursor
                    };
                    style = style.patch(cursor_style);
                }
            }

            // Draw character cells if within visible horizontal window
            if visual_col + cell_w > scroll_x && visual_col < scroll_x + (text_area_width as usize) {
                match transformed_glyph {
                    Some(TransformedGlyph::Char(new_ch, _)) => {
                        let screen_x_offset = visual_col.saturating_sub(scroll_x);
                        let target_x = text_start_x + screen_x_offset as u16;
                        if target_x < area.right() {
                            buf[(target_x, screen_y)].set_char(new_ch).set_style(style);
                        }
                    }
                    Some(TransformedGlyph::Tab { filler, terminator, .. }) => {
                        for sp in 0..cell_w {
                            let v_col = visual_col + sp;
                            if v_col >= scroll_x && v_col < scroll_x + (text_area_width as usize) {
                                let screen_x_offset = v_col - scroll_x;
                                let target_x = text_start_x + screen_x_offset as u16;
                                if target_x < area.right() {
                                    let c = if sp == cell_w - 1 { terminator } else { filler };
                                    buf[(target_x, screen_y)].set_char(c).set_style(style);
                                }
                            }
                        }
                    }
                    Some(TransformedGlyph::Cells(cells, ..)) => {
                        for sp in 0..cell_w {
                            let v_col = visual_col + sp;
                            if v_col >= scroll_x && v_col < scroll_x + (text_area_width as usize) {
                                let screen_x_offset = v_col - scroll_x;
                                let target_x = text_start_x + screen_x_offset as u16;
                                if target_x < area.right() {
                                    let c = cells.get(sp).copied().unwrap_or(' ');
                                    buf[(target_x, screen_y)].set_char(c).set_style(style);
                                }
                            }
                        }
                    }
                    None => {
                        if ch == '\t' {
                            for sp in 0..cell_w {
                                let v_col = visual_col + sp;
                                if v_col >= scroll_x && v_col < scroll_x + (text_area_width as usize) {
                                    let screen_x_offset = v_col - scroll_x;
                                    let target_x = text_start_x + screen_x_offset as u16;
                                    if target_x < area.right() {
                                        buf[(target_x, screen_y)].set_char(' ').set_style(style);
                                    }
                                }
                            }
                        } else {
                            let screen_x_offset = visual_col.saturating_sub(scroll_x);
                            let target_x = text_start_x + screen_x_offset as u16;
                            if target_x < area.right() {
                                buf[(target_x, screen_y)].set_char(ch).set_style(style);
                            }
                        }
                    }
                }
            }

            visual_col += cell_w;
        }

        // 5.5. Render cursor at end of line (if cursor is past the last character)
        for &(c_col, is_primary) in &line_cursor_cols {
            if c_col >= visual_col {
                if c_col >= scroll_x && c_col < scroll_x + (text_area_width as usize) {
                    let screen_x_offset = c_col - scroll_x;
                    let target_x = text_start_x + screen_x_offset as u16;
                    if target_x < area.right() {
                        let cursor_style = if is_primary {
                            styles.primary_cursor
                        } else {
                            styles.secondary_cursor
                        };
                        buf[(target_x, screen_y)].set_char(' ').set_style(cursor_style);
                    }
                }
            }
        }
    }
}
