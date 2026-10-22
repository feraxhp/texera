# Texera (`texera`)

> **Texera** *(union between `text` + `tessera`)* is a modular code-editor widget for [Ratatui](https://github.com/ratatui/ratatui), following the **"Bring Your Own" (BYO)** philosophy: lightweight by default, decoupled, and fully extensible through traits.

---

## Acknowledgements & Credits

Texera is directly inspired by **[CodeMirror 6](https://codemirror.net/)**.

> [!IMPORTANT]
> The work made by `@Marijn Haverbeke` is amazing, so much,
> that I decided to not recreate the wheel, but re-implement it in Rust.

Special thanks and full credit go to **Marijn Haverbeke** and the **CodeMirror 6 community**.

> **¿Examples?**
> The new version of [ie](https://github.com/feraxhp/ie) was made using this crate.

---

## Features

- `Bring Your Own (BYO) Architecture`: Pure opt-in builder pattern. Zero overhead for features you don't use.
- `Native Ratatui Widget`: Implements `ratatui::widgets::StatefulWidget` via `EditorView` and `EditorViewState`.
- `Incremental Syntax Highlighting`: Declarative TOML-based Pushdown Automata compiled into backtrack-free DFAs (`regex-automata`).
- `Hierarchical Theming`: JSON-based token color maps with fallback (e.g. `keyword.control` → `keyword`).
- `Multi-Cursor & Selections`: Built-in support for multiple active cursors and non-contiguous selection ranges.
-️ `Undo & Redo`: Transactional history tracking via `HistoryPlugin`.
- `Smart Viewport & Soft Wrapping`: Bidirectional coordinate mapping (`VisualPosition` ↔ `doc_offset`), grapheme cluster measurement, tab stops, and autoscrolling.
- `Line Numbers & Git-Style Markers`: Dynamic gutters with unsaved/dirty line indicators powered by Myers diff.

---

## Quick Start

### 1. Add Dependencies

Add `texera`, `texera-core`, and `ratatui` to your `Cargo.toml`:

```toml
[dependencies]
texera = { version = "0.1", features = ["crossterm", "highlight"] }
texera-core = "0.99"
ratatui = { version = "0.30", features = ["crossterm"] }
crossterm = "0.29"
```

### 2. Minimal Example

Below is a complete, minimal terminal code editor:

```rust,no_run
use std::io::stdout;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::CrosstermBackend,
    style::{Color, Style},
    Terminal,
};
use texera::{
    apply_action, DefaultKeymap, EditorAction, EditorView, EditorViewState,
    HistoryPlugin, Keymap, LineNumberPlugin, HighlightPlugin, ThemeMap, UnsavedLinePlugin,
};
use texera_core::{AllowMultipleSelections, State};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Initialize document state with transactional history & multi-selection
    let code = "fn main() {\n    println!(\"Hello from Texera!\");\n}\n";
    let mut state = State::from(code).with_extensions([
        HistoryPlugin::extension(None),
        HighlightPlugin::syntax_extension(),
        AllowMultipleSelections(true).into(),
    ]);

    // 2. Configure syntax highlighting (Rust TOML grammar + theme)
    let rust_toml = r#"
    [meta]
    name = "rust"
    extensions = ["rs"]

    [states.root]
    rules = [
        { match = '\bfn\b', token = "keyword.function" },
        { match = '\b(?:let|mut|if|else|return)\b', token = "keyword" },
        { match = '"(?:[^"\\]|\\.)*"', token = "string" },
        { match = '//.*', token = "comment" },
    ]
    "#;

    let mut theme = ThemeMap::default();
    theme.set_color("keyword", Color::Magenta);
    theme.set_color("keyword.function", Color::Blue);
    theme.set_color("string", Color::Green);
    theme.set_color("comment", Color::DarkGray);

    let highlighter = HighlightPlugin::new().with_language("rust", rust_toml, Some(theme));

    // 3. Set up gutter and unsaved line tracker
    let mut unsaved = UnsavedLinePlugin::new();
    unsaved.mark_as_saved(&state);

    let gutter = LineNumberPlugin::new()
        .with_style(Style::default().fg(Color::DarkGray))
        .with_marker(unsaved);

    // 4. Configure editor view and keybindings
    let keymap = DefaultKeymap::new();
    let mut view_state = EditorViewState::new();
    let editor_view = EditorView::new()
        .with_gutter(gutter)
        .with_highlighter(highlighter)
        .with_wrap(true);

    // 5. Setup terminal
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // 6. Application loop
    loop {
        terminal.draw(|f| {
            let area = f.area();
            f.render_stateful_widget(editor_view.bind(&state), area, &mut view_state);
        })?;

        if let Event::Key(key) = event::read()? {
            // Exit on Ctrl-Q
            if key.code == KeyCode::Char('q') && key.modifiers.contains(KeyModifiers::CONTROL) {
                break;
            }

            // Map terminal key events to editor actions
            if let Some(action) = keymap.action_for_key(&key) {
                apply_action(&mut state, action, Some(&key))?;
            }
        }
    }

    // Cleanup terminal
    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    Ok(())
}
```

---

## Syntax Highlighting Specification

Texera includes a high-performance declarative syntax highlighter based on a **Pushdown Automaton (PDA)** and backtracking-free DFAs (`regex-automata`).

- For human developers designing syntax definitions, see **[toml.spec.md](toml.spec.md)**.
- For LLMs generating syntax rules automatically, see **[toml.spec.llm.md](toml.spec.llm.md)**.

---

## License

~~~
Copyright (C) 2026 feraxhp

This program is free software: you can redistribute it and/or modify
it under the terms of the GNU General Public License as published by
the Free Software Foundation, either version 3 of the License, or
(at your option) any later version.

This program is distributed in the hope that it will be useful,
but WITHOUT ANY WARRANTY; without even the implied warranty of
MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
GNU General Public License for more details.

You should have received a copy of the GNU General Public License
along with this program. If not, see <https://www.gnu.org/licenses/>.
~~~
