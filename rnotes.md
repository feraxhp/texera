## Whats new

- `*+` add GlyphContext, GlyphTransformer y TransformedGlyph to create glyph tranform pluggins
- `*+` add WhitespacePlugin to show spaces and tabs based on tab_size facet 
  > requires `whitespace` feature

### example

```rust
use ratatui::style::{Color, Style};
use texera::{EditorView, WhitespacePlugin};

// 1. Default uses ('·' para espacios, ' ' de filler y '⇥' de terminador):
let plugin = WhitespacePlugin::default()
    .with_style(Style::default().fg(Color::DarkGray));

// 2. Personalize any char:
let custom_plugin = WhitespacePlugin::new()
    .with_space_char('·')
    .with_tab_chars('─', '>')
    .with_style(Style::default().fg(Color::DarkGray));

let view = EditorView::new().with_glyph_transformer(plugin);
```

## Internal

- better code structure for extensibility

by @feraxhp
---
**Full Changelog**: https://github.com/feraxhp/texera/compare/v0.2.0...v0.3.0
