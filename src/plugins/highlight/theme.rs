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

//! Syntax highlighting theme mapping token names to display colors.

use std::collections::HashMap;
use std::str::FromStr;
use std::path::Path;
use std::fs;

use ratatui::style::Color;

/// Syntax highlighting theme mapping token names to `ratatui::style::Color`.
///
/// Supports hierarchical token fallback (e.g. `keyword.control` falls back to `keyword`).
///
/// # Examples
///
/// ```rust
/// use ratatui::style::Color;
/// use texera::ThemeMap;
///
/// let mut theme = ThemeMap::default();
/// theme.set_color("keyword", Color::Magenta);
/// assert_eq!(theme.color_for("keyword.control"), Some(Color::Magenta));
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ThemeMap {
    /// Mapping from token category name to terminal display color.
    pub token_colors: HashMap<String, Color>,
}

impl ThemeMap {
    /// Load theme mapping from a JSON file on disk.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use texera::ThemeMap;
    ///
    /// let theme = ThemeMap::load_from_file("themes/dark.json");
    /// ```
    pub fn load_from_file(path: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let content = fs::read_to_string(path)?;
        Self::from_json_str(&content)
    }

    /// Load theme mapping from a JSON string.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ratatui::style::Color;
    /// use texera::ThemeMap;
    ///
    /// let json = r#"{"keyword": "Magenta", "string": "Green"}"#;
    /// let theme = ThemeMap::from_json_str(json).unwrap();
    /// assert_eq!(theme.color_for("keyword"), Some(Color::Magenta));
    /// ```
    pub fn from_json_str(json_content: &str) -> Result<Self, Box<dyn std::error::Error>> {
        let raw_map: HashMap<String, String> = serde_json::from_str(json_content)?;
        let mut token_colors = HashMap::new();
        for (token, color_str) in raw_map {
            if let Ok(color) = Color::from_str(&color_str) {
                token_colors.insert(token, color);
            }
        }
        Ok(Self { token_colors, })
    }

    /// Resolves the color of a token with a descending hierarchical fallback.
    ///
    /// `keyword.operator.arithmetic` is looked up as `keyword.operator.arithmetic`, then
    /// `keyword.operator`, then `keyword`.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ratatui::style::Color;
    /// use texera::ThemeMap;
    ///
    /// let mut theme = ThemeMap::default();
    /// theme.set_color("keyword", Color::Magenta);
    /// assert_eq!(theme.color_for("keyword.control"), Some(Color::Magenta));
    /// assert_eq!(theme.color_for("unknown"), None);
    /// ```
    pub fn color_for(&self, token: &str) -> Option<Color> {
        let mut key = token;
        loop {
            if let Some(color) = self.token_colors.get(key) {
                return Some(*color);
            }
            key = key.rsplit_once('.')?.0;
        }
    }

    /// Assigns a display color to a specific token name.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use ratatui::style::Color;
    /// use texera::ThemeMap;
    ///
    /// let mut theme = ThemeMap::default();
    /// theme.set_color("function.call", Color::Cyan);
    /// assert_eq!(theme.color_for("function.call"), Some(Color::Cyan));
    /// ```
    pub fn set_color(&mut self, token: impl Into<String>, color: Color) {
        self.token_colors.insert(token.into(), color);
    }
}
