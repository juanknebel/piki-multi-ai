use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Span;
use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme as SyntectTheme, ThemeSet};
use syntect::parsing::{SyntaxReference, SyntaxSet};

pub struct SyntaxHighlighter {
    syntax_set: SyntaxSet,
    theme: SyntectTheme,
}

impl SyntaxHighlighter {
    pub fn new(theme_name: &str) -> Self {
        let syntax_set = SyntaxSet::load_defaults_newlines();
        let theme_set = ThemeSet::load_defaults();
        let theme = theme_set
            .themes
            .get(theme_name)
            .cloned()
            .unwrap_or_else(|| theme_set.themes["base16-ocean.dark"].clone());
        Self { syntax_set, theme }
    }

    pub fn find_syntax(&self, path: &str) -> Option<&SyntaxReference> {
        let ext = std::path::Path::new(path)
            .extension()
            .and_then(|s| s.to_str())?;
        self.syntax_set.find_syntax_by_extension(ext)
    }

    /// Find syntax by language name (e.g., "rust", "python", "javascript").
    pub fn find_syntax_by_name(&self, name: &str) -> Option<&SyntaxReference> {
        // Try exact match first, then case-insensitive
        self.syntax_set
            .find_syntax_by_token(name)
            .or_else(|| self.syntax_set.find_syntax_by_name(name))
    }

    pub fn highlighter_for(&self, syntax: &SyntaxReference) -> HighlightLines<'_> {
        HighlightLines::new(syntax, &self.theme)
    }

    /// Highlight a single line, returning ratatui Spans.
    /// `base_style` is applied underneath (for diff add/delete bg coloring).
    pub fn highlight_line(
        &self,
        hl: &mut HighlightLines<'_>,
        line: &str,
        base_style: Style,
    ) -> Vec<Span<'static>> {
        let ranges = hl
            .highlight_line(line, &self.syntax_set)
            .unwrap_or_default();
        ranges
            .into_iter()
            .map(|(style, text)| {
                let mut ratatui_style = base_style;
                ratatui_style.fg = Some(syntect_to_ratatui_color(style.foreground));
                if style
                    .font_style
                    .contains(syntect::highlighting::FontStyle::BOLD)
                {
                    ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
                }
                if style
                    .font_style
                    .contains(syntect::highlighting::FontStyle::ITALIC)
                {
                    ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
                }
                Span::styled(text.to_string(), ratatui_style)
            })
            .collect()
    }

    #[cfg(test)]
    pub fn syntax_set(&self) -> &SyntaxSet {
        &self.syntax_set
    }
}

fn syntect_to_ratatui_color(c: syntect::highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_default_theme() {
        let hl = SyntaxHighlighter::new("base16-ocean.dark");
        assert!(!hl.syntax_set().syntaxes().is_empty());
    }

    #[test]
    fn test_new_unknown_theme_falls_back() {
        // Should not panic, falls back to base16-ocean.dark
        let _hl = SyntaxHighlighter::new("nonexistent-theme");
    }

    #[test]
    fn test_find_syntax_by_extension() {
        let hl = SyntaxHighlighter::new("base16-ocean.dark");
        assert!(hl.find_syntax("main.rs").is_some());
        assert!(hl.find_syntax("script.py").is_some());
        assert!(hl.find_syntax("no_ext").is_none());
    }

    #[test]
    fn test_find_syntax_by_name() {
        let hl = SyntaxHighlighter::new("base16-ocean.dark");
        assert!(hl.find_syntax_by_name("rust").is_some());
        assert!(hl.find_syntax_by_name("py").is_some());
        assert!(hl.find_syntax_by_name("js").is_some());
    }

    #[test]
    fn test_highlight_line_returns_spans() {
        let hl = SyntaxHighlighter::new("base16-ocean.dark");
        let syntax = hl.find_syntax("test.rs").unwrap();
        let mut highlighter = hl.highlighter_for(syntax);
        let spans = hl.highlight_line(&mut highlighter, "fn main() {}", Style::default());
        assert!(!spans.is_empty());
        // All spans together should reconstruct the original text
        let reconstructed: String = spans.iter().map(|s| s.content.as_ref()).collect();
        assert_eq!(reconstructed, "fn main() {}");
    }

    #[test]
    fn test_highlight_line_with_base_style_preserves_bg() {
        let hl = SyntaxHighlighter::new("base16-ocean.dark");
        let syntax = hl.find_syntax("test.rs").unwrap();
        let mut highlighter = hl.highlighter_for(syntax);
        let base = Style::default().bg(Color::Rgb(0, 50, 0));
        let spans = hl.highlight_line(&mut highlighter, "let x = 1;", base);
        // Each span should have the base bg preserved
        for span in &spans {
            assert_eq!(span.style.bg, Some(Color::Rgb(0, 50, 0)));
        }
    }

    #[test]
    fn test_syntect_to_ratatui_color() {
        let c = syntect::highlighting::Color {
            r: 255,
            g: 128,
            b: 0,
            a: 255,
        };
        assert_eq!(syntect_to_ratatui_color(c), Color::Rgb(255, 128, 0));
    }
}
