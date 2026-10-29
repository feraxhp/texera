use std::io::stdout;

use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::{Block, Borders, Paragraph},
    Terminal,
};
use texera::{
    apply_action, DefaultKeymap, EditorAction, EditorView, EditorViewState, GlyphContext,
    GlyphTransformer, HistoryPlugin, Keymap, LineNumberPlugin, TransformedGlyph,
    UnsavedLinePlugin, ViewPlugin,
};
use texera_core::{AllowMultipleSelections, State, TabSize};

/// Whitespace visualizer plugin replacing spaces with dots and tabs with guided arrows.
#[derive(Clone)]
struct WhitespaceVisualizer {
    style: Style,
}

impl WhitespaceVisualizer {
    fn new(style: Style) -> Self {
        Self { style }
    }
}

impl ViewPlugin for WhitespaceVisualizer {}

impl GlyphTransformer for WhitespaceVisualizer {
    fn transform(&self, ctx: &GlyphContext) -> Option<TransformedGlyph> {
        match ctx.ch {
            ' ' => Some(TransformedGlyph::Char('·', Some(self.style))),
            '\t' => Some(TransformedGlyph::Tab {
                filler: '─',
                terminator: '→',
                style: Some(self.style),
            }),
            _ => None,
        }
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let initial_code = r#"// Whitespace Visualization Example
//
// Press [F4] to toggle whitespace visibility on/off.
// Press [F5] to cycle tab size: 2 -> 4 -> 8.
// Press [Esc] to exit.

fn demo_whitespace() {
	// Line indented with a TAB:
	let tab_indent = true;

    // Line indented with 4 SPACES:
    let space_indent = true;

	// Mixed tab stops alignment:
	col0:	stop1	stop2
	1:	stop1	stop2
	12:	stop1	stop2
	123:	stop1	stop2
}
"#;

    let tab_sizes = [4, 2, 8];
    let mut tab_idx = 0;

    let mut state = State::from(initial_code).with_extensions([
        HistoryPlugin::extension(None),
        AllowMultipleSelections(true).into(),
        TabSize(tab_sizes[tab_idx]).into(),
    ]);

    let mut unsaved_lines = UnsavedLinePlugin::new();
    unsaved_lines.mark_as_saved(&state);

    let keymap = DefaultKeymap::new();
    let mut view_state = EditorViewState::new();

    let mut show_whitespace = true;
    let mut show_gutter = true;
    let mut wrap_enabled = false;
    let mut status_message = "Ready. Press [F4] to toggle whitespace, [F5] to change tab size.".to_string();

    let whitespace_plugin = WhitespaceVisualizer::new(Style::default().fg(Color::DarkGray));

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    loop {
        terminal.draw(|f| {
            let area = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // Header
                    Constraint::Min(3),    // Editor
                    Constraint::Length(1), // Footer
                ])
                .split(area);

            let header = Paragraph::new(format!(
                " Texera | [F4] Whitespace: {} | [F5] TabSize: {} | [F2] Gutter | [F3] Wrap | [Esc] Quit",
                if show_whitespace { "ON" } else { "OFF" },
                state.tab_size(),
            ))
            .style(Style::default().bg(Color::DarkGray).fg(Color::White));
            f.render_widget(header, chunks[0]);

            let mut view = EditorView::new().with_wrap(wrap_enabled);

            if show_whitespace {
                view = view.with_glyph_transformer(whitespace_plugin.clone());
            }

            if show_gutter {
                let gutter = LineNumberPlugin::new()
                    .with_style(Style::default().fg(Color::DarkGray))
                    .with_marker(unsaved_lines.clone());
                view = view.with_gutter(gutter);
            }

            let editor_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(format!(
                    " Whitespace Demo (TabSize: {}, Lines: {}) ",
                    state.tab_size(),
                    state.doc().len_lines()
                ));
            let inner_area = editor_block.inner(chunks[1]);
            f.render_widget(editor_block, chunks[1]);
            f.render_stateful_widget(view.bind(&state), inner_area, &mut view_state);

            let footer = Paragraph::new(format!(
                " [{}] | Scroll: ({}, {}) | Whitespace: {} | TabSize: {}",
                status_message,
                view_state.scroll_x(),
                view_state.scroll_y(),
                if show_whitespace { "ON" } else { "OFF" },
                state.tab_size()
            ))
            .style(Style::default().bg(Color::DarkGray).fg(Color::Yellow));
            f.render_widget(footer, chunks[2]);
        })?;

        if event::poll(std::time::Duration::from_millis(50))? {
            if let Event::Key(key_event) = event::read()? {
                if key_event.code == KeyCode::Esc
                    || (key_event.code == KeyCode::Char('c') && key_event.modifiers.contains(KeyModifiers::CONTROL))
                {
                    break;
                }

                // Toggle Whitespace Plugin
                if key_event.code == KeyCode::F(4) {
                    show_whitespace = !show_whitespace;
                    status_message = format!("Whitespace visualizer: {}", if show_whitespace { "ENABLED" } else { "DISABLED" });
                    continue;
                }

                // Cycle Tab Size
                if key_event.code == KeyCode::F(5) {
                    tab_idx = (tab_idx + 1) % tab_sizes.len();
                    let new_size = tab_sizes[tab_idx];
                    state = state.clone().with_extensions(TabSize(new_size));
                    status_message = format!("Tab size updated to: {}", new_size);
                    continue;
                }

                // Toggle Gutter
                if key_event.code == KeyCode::F(2) {
                    show_gutter = !show_gutter;
                    status_message = format!("Gutter visible: {}", show_gutter);
                    continue;
                }

                // Toggle Wrap
                if key_event.code == KeyCode::F(3) {
                    wrap_enabled = !wrap_enabled;
                    status_message = format!("Line wrap: {}", wrap_enabled);
                    continue;
                }

                if let Some(action) = keymap.action_for_key(&key_event) {
                    let is_edit = matches!(
                        action,
                        EditorAction::InsertChar
                            | EditorAction::InsertNewline
                            | EditorAction::InsertTab
                            | EditorAction::DeleteBackward
                            | EditorAction::DeleteForward
                            | EditorAction::Undo
                            | EditorAction::Redo
                    );

                    match apply_action(&mut state, action, Some(&key_event)) {
                        Ok(changed) => {
                            if changed {
                                if is_edit {
                                    unsaved_lines.check_dirty_lines(&state);
                                }
                                status_message = format!("Action: {:?}", action);
                            }
                        }
                        Err(e) => {
                            status_message = format!("Error: {:?}", e);
                        }
                    }
                }
            }
        }
    }

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    println!("Whitespace viewer closed.");
    Ok(())
}
