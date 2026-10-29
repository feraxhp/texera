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

//! Filesystem directory scanning and language extension loader.

use std::path::PathBuf;
use std::path::Path;
use std::fs;

use super::plugin::HighlightPlugin;
use super::theme::ThemeMap;

impl HighlightPlugin {
    /// Extends the highlighter at runtime by scanning a directory.
    ///
    /// Two layouts are understood (and may be mixed):
    ///
    /// * `root/<lang>.toml` with an optional `root/<lang>.json` theme;
    /// * `root/<lang>/` containing a `*.toml` (preferably `<lang>.toml` / `syntax.toml`) and an
    ///   optional `theme.json` (or any `*.json`).
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use texera::HighlightPlugin;
    ///
    /// let plugin = HighlightPlugin::new().extend_from("syntaxes").unwrap();
    /// ```
    pub fn extend_from(mut self, root_dir: impl AsRef<Path>) -> Result<Self, Box<dyn std::error::Error>> {
        let root = root_dir.as_ref();
        if !root.exists() || !root.is_dir() {
            return Ok(self);
        }

        let mut entries: Vec<PathBuf> = fs::read_dir(root)?.filter_map(|e| e.ok().map(|e| e.path())).collect();
        entries.sort();

        let ext_of = |p: &Path| p.extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase);
        let stem_of = |p: &Path| p.file_stem().and_then(|n| n.to_str()).unwrap_or("unknown").to_string();

        for path in entries {
            if path.is_file() && ext_of(&path).as_deref() == Some("toml") {
                let name = stem_of(&path);
                let theme = ThemeMap::load_from_file(path.with_extension("json")).ok();
                self = self.with_language(name, fs::read_to_string(&path)?, theme);
            } else if path.is_dir() {
                let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("unknown").to_string();
                let mut files: Vec<PathBuf> =
                    fs::read_dir(&path)?.filter_map(|e| e.ok().map(|e| e.path())).filter(|p| p.is_file()).collect();
                files.sort();

                let preferred = [format!("{name}.toml"), "syntax.toml".to_string()];
                let toml = preferred
                    .iter()
                    .map(|f| path.join(f))
                    .find(|p| p.is_file())
                    .or_else(|| files.iter().find(|p| ext_of(p).as_deref() == Some("toml")).cloned());
                let theme_path = Some(path.join("theme.json"))
                    .filter(|p| p.is_file())
                    .or_else(|| files.iter().find(|p| ext_of(p).as_deref() == Some("json")).cloned());

                if let Some(toml) = toml {
                    let theme = theme_path.and_then(|p| ThemeMap::load_from_file(p).ok());
                    self = self.with_language(name, fs::read_to_string(&toml)?, theme);
                }
            }
        }

        Ok(self)
    }
}
