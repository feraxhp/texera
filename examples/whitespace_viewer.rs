use std::io::stdout;

use crossterm::terminal::EnterAlternateScreen;
use crossterm::terminal::LeaveAlternateScreen;
use crossterm::terminal::disable_raw_mode;
use texera_core::AllowMultipleSelections;
use crossterm::terminal::enable_raw_mode;
use ratatui::backend::CrosstermBackend;
use crossterm::event::KeyModifiers;
use ratatui::layout::Constraint;
use ratatui::widgets::Paragraph;
use ratatui::layout::Direction;
use crossterm::event::KeyCode;
use ratatui::widgets::Borders;
use texera::UnsavedLinePlugin;
use texera::LineNumberPlugin;
use texera::WhitespacePlugin;
use crossterm::event::Event;
use ratatui::layout::Layout;
use ratatui::widgets::Block;
use texera::EditorViewState;
use ratatui::style::Color;
use ratatui::style::Style;
use texera::DefaultKeymap;
use texera::HistoryPlugin;
use texera::EditorAction;
use texera_core::TabSize;
use texera::apply_action;
use texera::EditorView;
use texera_core::State;
use crossterm::execute;
use ratatui::Terminal;
use crossterm::event;
use texera::Keymap;

#[derive(Clone, Copy)]
struct TabStylePreset {
    name: &'static str,
    filler: char,
    terminator: char,
}

const TAB_PRESETS: &[TabStylePreset] = &[
    TabStylePreset {
        name: "Default (Space + Tab: '   ⇥')",
        filler: ' ',
        terminator: '⇥',
    },
    TabStylePreset {
        name: "ASCII Arrow (--->)",
        filler: '-',
        terminator: '>',
    },
    TabStylePreset {
        name: "Box + ASCII (───>)",
        filler: '─',
        terminator: '>',
    },
    TabStylePreset {
        name: "Box + Angle (───›)",
        filler: '─',
        terminator: '›',
    },
    TabStylePreset {
        name: "Box + Triangle (───▸)",
        filler: '─',
        terminator: '▸',
    },
    TabStylePreset {
        name: "Dot Guide (···>)",
        filler: '·',
        terminator: '>',
    },
    TabStylePreset {
        name: "Original Unicode (───→)",
        filler: '─',
        terminator: '→',
    },
];

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let initial_code = r#"// Whitespace Visualization Example using Texera's WhitespacePlugin
//
// Press [F4] to toggle whitespace on/off.
// Press [F5] to cycle tab size: 2 -> 4 -> 8.
// Press [F6] to cycle tab glyph styles.
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
    let mut preset_idx = 0;

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
    let mut status_message = "Ready. [F6] cycles tab glyphs, [F4] toggles on/off, [F5] changes size.".to_string();

    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    loop {
        let current_preset = TAB_PRESETS[preset_idx];
        let whitespace_plugin = WhitespacePlugin::default()
            .with_tab_chars(current_preset.filler, current_preset.terminator)
            .with_style(Style::default().fg(Color::DarkGray));

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
                " Texera | [F4] Whitespace: {} | [F5] TabSize: {} | [F6] Style: {} | [Esc] Quit",
                if show_whitespace { "ON" } else { "OFF" },
                state.tab_size(),
                current_preset.name,
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
                    " Whitespace Demo (Tab: '{}' / '{}', TabSize: {}) ",
                    current_preset.filler,
                    current_preset.terminator,
                    state.tab_size()
                ));
            let inner_area = editor_block.inner(chunks[1]);
            f.render_widget(editor_block, chunks[1]);
            f.render_stateful_widget(view.bind(&state), inner_area, &mut view_state);

            let footer = Paragraph::new(format!(
                " [{}] | Scroll: ({}, {}) | Style: {}",
                status_message,
                view_state.scroll_x(),
                view_state.scroll_y(),
                current_preset.name
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
                    status_message = format!("Whitespace: {}", if show_whitespace { "ENABLED" } else { "DISABLED" });
                    continue;
                }

                // Cycle Tab Size
                if key_event.code == KeyCode::F(5) {
                    tab_idx = (tab_idx + 1) % tab_sizes.len();
                    let new_size = tab_sizes[tab_idx];
                    state = state.clone().with_extensions(TabSize(new_size));
                    status_message = format!("Tab size: {}", new_size);
                    continue;
                }

                // Cycle Tab Style Preset
                if key_event.code == KeyCode::F(6) {
                    preset_idx = (preset_idx + 1) % TAB_PRESETS.len();
                    let next_preset = TAB_PRESETS[preset_idx];
                    status_message = format!("Switched style to: {}", next_preset.name);
                    continue;
                }

                // Toggle Gutter
                if key_event.code == KeyCode::F(2) {
                    show_gutter = !show_gutter;
                    status_message = format!("Gutter: {}", show_gutter);
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
