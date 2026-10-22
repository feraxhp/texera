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
    apply_action, DefaultKeymap, EditorAction, EditorView, EditorViewState, HistoryPlugin,
    Keymap, LineNumberPlugin, HighlightPlugin, ThemeMap, UnsavedLinePlugin,
};
use texera_core::{AllowMultipleSelections, State};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initial Document Text & State with History & Multi-selection
    let initial_code = r#"// Welcome to Texera - Modular Ratatui Code Editor!
fn calculate_fibonacci(n: u32) -> u64 {
    let mut a: u64 = 0;
    let mut b: u64 = 1;
    for _ in 0..n {
        let temp = a + b;
        a = b;
        b = temp;
    }
    return a;
}

// Try editing any line to see the yellow dirty indicator!
// Press [Ctrl-S] to save and clear the dirty markers.
// Press [Ctrl-Z] to undo, [Ctrl-Y] to redo.
// Press [Alt-Down] to add multiple cursors.
"#;

    let mut state = State::from(initial_code).with_extensions([
        HistoryPlugin::extension(None),
        // Caches the syntax parse per document version (multi-line comments / strings).
        HighlightPlugin::syntax_extension(),
        AllowMultipleSelections(true).into(),
    ]);

    // 2. Syntax Highlighter powered by declarative TOML state machine and JSON theme
    let rust_toml = include_str!("../syntaxes/rust.toml");
    let mut rust_theme = ThemeMap::default();
    rust_theme.set_color("keyword", Color::Magenta);
    rust_theme.set_color("keyword.declaration", Color::Magenta);
    rust_theme.set_color("keyword.function", Color::Magenta);
    rust_theme.set_color("keyword.control", Color::Magenta);
    rust_theme.set_color("function.definition", Color::Blue);
    rust_theme.set_color("function.call", Color::LightBlue);
    rust_theme.set_color("type.builtin", Color::Cyan);
    rust_theme.set_color("type", Color::Cyan);
    rust_theme.set_color("string", Color::Green);
    rust_theme.set_color("number", Color::Yellow);
    rust_theme.set_color("comment", Color::DarkGray);

    let highlight = HighlightPlugin::new().with_language("rust", rust_toml, Some(rust_theme));

    // 3. Plugins Setup
    let mut unsaved_lines = UnsavedLinePlugin::new();
    unsaved_lines.mark_as_saved(&state);

    let keymap = DefaultKeymap::new();
    let mut view_state = EditorViewState::new();

    let mut show_gutter = true;
    let mut wrap_enabled = false;
    let mut status_message = "Ready. Edit text or press keys.".to_string();

    // 4. Terminal Initialization
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 5. Main Application Loop
    loop {
        // Render Frame
        terminal.draw(|f| {
            let area = f.area();
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(1), // Top Header Bar
                    Constraint::Min(3),    // Editor Area
                    Constraint::Length(1), // Bottom Status Bar
                ])
                .split(area);

            // Render Header Bar
            let header = Paragraph::new(
                " Texera Editor | [F2] Toggle Gutter | [F3] Toggle Wrap | [Ctrl+S] Save | [Esc] Quit",
            )
            .style(Style::default().bg(Color::DarkGray).fg(Color::White));
            f.render_widget(header, chunks[0]);

            // Build Editor View
            let mut view = EditorView::new()
                .with_highlighter(highlight.clone())
                .with_wrap(wrap_enabled);

            if show_gutter {
                let gutter = LineNumberPlugin::new()
                    .with_style(Style::default().fg(Color::DarkGray))
                    .with_marker(unsaved_lines.clone());
                view = view.with_gutter(gutter);
            }

            // Render Editor inside Bordered Block
            let editor_block = Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Blue))
                .title(format!(
                    " Code Editor (Lines: {}, Cursors: {}) ",
                    state.doc().len_lines(),
                    state.selection().ranges_ref().len()
                ));
            let inner_area = editor_block.inner(chunks[1]);
            f.render_widget(editor_block, chunks[1]);
            f.render_stateful_widget(view.bind(&state), inner_area, &mut view_state);

            // Syntax Progress Indicator
            let syntax_status = if let Some(prog) = HighlightPlugin::progress(&state) {
                if prog.is_complete() {
                    "Syntax: 100%".to_string()
                } else {
                    format!("Syntax: {}/{} ({:.0}%)", prog.lines_ready, prog.total_lines, prog.ratio() * 100.0)
                }
            } else {
                "Syntax: (off)".to_string()
            };

            // Status Bar Footer
            let footer = Paragraph::new(format!(
                " [{}] | Scroll X: {} | Scroll Y: {} | {} | Dirty: {}",
                status_message,
                view_state.scroll_x(),
                view_state.scroll_y(),
                syntax_status,
                if unsaved_lines.is_dirty() { "YES (*)" } else { "NO" }
            ))
            .style(Style::default().bg(Color::DarkGray).fg(Color::Yellow));
            f.render_widget(footer, chunks[2]);
        })?;

        // 6. Handle Keyboard Input (shorter poll timeout if background syntax parse is catching up)
        let is_syntax_ready = view_state.syntax_ready(&state);
        let poll_timeout = if is_syntax_ready {
            std::time::Duration::from_millis(50)
        } else {
            // Keep refreshing frames rapidly until viewport lines are parsed
            std::time::Duration::from_millis(8)
        };

        if event::poll(poll_timeout)? {
            if let Event::Key(key_event) = event::read()? {
                // Exit with Esc or Ctrl+C
                if key_event.code == KeyCode::Esc
                    || (key_event.code == KeyCode::Char('c') && key_event.modifiers.contains(KeyModifiers::CONTROL))
                {
                    break;
                }

                // Toggle Line Gutter with F2
                if key_event.code == KeyCode::F(2) {
                    show_gutter = !show_gutter;
                    status_message = format!("Gutter visible: {}", show_gutter);
                    continue;
                }

                // Toggle Soft Line Wrapping with F3
                if key_event.code == KeyCode::F(3) {
                    wrap_enabled = !wrap_enabled;
                    status_message = format!("Line wrap: {}", wrap_enabled);
                    continue;
                }

                // Save Document with Ctrl+S
                if key_event.code == KeyCode::Char('s') && key_event.modifiers.contains(KeyModifiers::CONTROL) {
                    unsaved_lines.mark_as_saved(&state);
                    status_message = "Document saved cleanly.".to_string();
                    continue;
                }

                // Translate key into EditorAction
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
                                status_message = format!("Action executed: {:?}", action);
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

    // 7. Cleanup Terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    println!("Texera terminal editor closed successfully.");
    Ok(())
}
