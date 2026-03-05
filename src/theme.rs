use ratatui::style::Color;
use serde::Deserialize;

fn parse_color(s: &str) -> Color {
    match s {
        "Black" => Color::Black,
        "Red" => Color::Red,
        "Green" => Color::Green,
        "Yellow" => Color::Yellow,
        "Blue" => Color::Blue,
        "Magenta" => Color::Magenta,
        "Cyan" => Color::Cyan,
        "Gray" => Color::Gray,
        "DarkGray" => Color::DarkGray,
        "LightRed" => Color::LightRed,
        "LightGreen" => Color::LightGreen,
        "LightYellow" => Color::LightYellow,
        "LightBlue" => Color::LightBlue,
        "LightMagenta" => Color::LightMagenta,
        "LightCyan" => Color::LightCyan,
        "White" => Color::White,
        s if s.starts_with('#') && s.len() == 7 => {
            let r = u8::from_str_radix(&s[1..3], 16).unwrap_or(255);
            let g = u8::from_str_radix(&s[3..5], 16).unwrap_or(255);
            let b = u8::from_str_radix(&s[5..7], 16).unwrap_or(255);
            Color::Rgb(r, g, b)
        }
        _ => Color::White,
    }
}

/// Resolve an optional TOML string to a Color, falling back to a default.
fn resolve(opt: &Option<String>, default: Color) -> Color {
    match opt {
        Some(s) => parse_color(s),
        None => default,
    }
}

// ── TOML deserialization structs (all Option<String>) ──

#[derive(Deserialize, Default)]
#[serde(default)]
struct ThemeToml {
    border: BorderToml,
    workspace_list: WorkspaceListToml,
    file_list: FileListToml,
    tabs: TabsToml,
    subtabs: SubtabsToml,
    status_bar: StatusBarToml,
    footer: FooterToml,
    diff: DiffToml,
    dialog: DialogToml,
    help: HelpToml,
    general: GeneralToml,
    fuzzy_search: FuzzySearchToml,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BorderToml {
    active_interact: Option<String>,
    active_navigate: Option<String>,
    inactive: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct WorkspaceListToml {
    empty_text: Option<String>,
    name_active: Option<String>,
    name_inactive: Option<String>,
    detail_selected: Option<String>,
    detail_normal: Option<String>,
    selected_bg: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct FileListToml {
    empty_text: Option<String>,
    modified: Option<String>,
    added: Option<String>,
    deleted: Option<String>,
    renamed: Option<String>,
    untracked: Option<String>,
    conflicted: Option<String>,
    staged: Option<String>,
    staged_modified: Option<String>,
    file_path: Option<String>,
    selected_bg: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct TabsToml {
    active: Option<String>,
    inactive: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct SubtabsToml {
    active: Option<String>,
    inactive: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct StatusBarToml {
    error_bg: Option<String>,
    error_fg: Option<String>,
    diff_bg: Option<String>,
    diff_fg: Option<String>,
    interact_bg: Option<String>,
    navigate_bg: Option<String>,
    mode_fg: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct FooterToml {
    key: Option<String>,
    description: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct DiffToml {
    border: Option<String>,
    empty_text: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct DialogToml {
    new_ws_border: Option<String>,
    new_ws_active: Option<String>,
    new_ws_inactive: Option<String>,
    delete_border: Option<String>,
    delete_text: Option<String>,
    delete_name: Option<String>,
    delete_yes: Option<String>,
    delete_no: Option<String>,
    delete_cancel: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct HelpToml {
    border: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct GeneralToml {
    welcome_text: Option<String>,
    muted_text: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct FuzzySearchToml {
    border: Option<String>,
    input_text: Option<String>,
    match_highlight: Option<String>,
    result_text: Option<String>,
    selected_bg: Option<String>,
    count_text: Option<String>,
}

// ── Resolved Theme (Color values) ──

pub struct BorderTheme {
    pub active_interact: Color,
    pub active_navigate: Color,
    pub inactive: Color,
}

pub struct WorkspaceListTheme {
    pub empty_text: Color,
    pub name_active: Color,
    pub name_inactive: Color,
    pub detail_selected: Color,
    pub detail_normal: Color,
    pub selected_bg: Color,
}

pub struct FileListTheme {
    pub empty_text: Color,
    pub modified: Color,
    pub added: Color,
    pub deleted: Color,
    pub renamed: Color,
    pub untracked: Color,
    pub conflicted: Color,
    pub staged: Color,
    pub staged_modified: Color,
    pub file_path: Color,
    pub selected_bg: Color,
}

pub struct TabsTheme {
    pub active: Color,
    pub inactive: Color,
}

pub struct SubtabsTheme {
    pub active: Color,
    pub inactive: Color,
}

pub struct StatusBarTheme {
    pub error_bg: Color,
    pub error_fg: Color,
    pub diff_bg: Color,
    pub diff_fg: Color,
    pub interact_bg: Color,
    pub navigate_bg: Color,
    pub mode_fg: Color,
}

pub struct FooterTheme {
    pub key: Color,
    pub description: Color,
}

pub struct DiffTheme {
    pub border: Color,
    pub empty_text: Color,
}

pub struct DialogTheme {
    pub new_ws_border: Color,
    pub new_ws_active: Color,
    pub new_ws_inactive: Color,
    pub delete_border: Color,
    pub delete_text: Color,
    pub delete_name: Color,
    pub delete_yes: Color,
    pub delete_no: Color,
    pub delete_cancel: Color,
}

pub struct HelpTheme {
    pub border: Color,
}

pub struct GeneralTheme {
    pub welcome_text: Color,
    pub muted_text: Color,
}

pub struct FuzzySearchTheme {
    pub border: Color,
    pub input_text: Color,
    pub match_highlight: Color,
    pub result_text: Color,
    pub selected_bg: Color,
    pub count_text: Color,
}

pub struct Theme {
    pub border: BorderTheme,
    pub workspace_list: WorkspaceListTheme,
    pub file_list: FileListTheme,
    pub tabs: TabsTheme,
    pub subtabs: SubtabsTheme,
    pub status_bar: StatusBarTheme,
    pub footer: FooterTheme,
    pub diff: DiffTheme,
    pub dialog: DialogTheme,
    pub help: HelpTheme,
    pub general: GeneralTheme,
    pub fuzzy_search: FuzzySearchTheme,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            border: BorderTheme {
                active_interact: Color::Green,
                active_navigate: Color::Yellow,
                inactive: Color::DarkGray,
            },
            workspace_list: WorkspaceListTheme {
                empty_text: Color::DarkGray,
                name_active: Color::White,
                name_inactive: Color::Gray,
                detail_selected: Color::Gray,
                detail_normal: Color::DarkGray,
                selected_bg: Color::DarkGray,
            },
            file_list: FileListTheme {
                empty_text: Color::DarkGray,
                modified: Color::Yellow,
                added: Color::Green,
                deleted: Color::Red,
                renamed: Color::Cyan,
                untracked: Color::DarkGray,
                conflicted: Color::Magenta,
                staged: Color::Green,
                staged_modified: Color::Yellow,
                file_path: Color::White,
                selected_bg: Color::DarkGray,
            },
            tabs: TabsTheme {
                active: Color::Yellow,
                inactive: Color::DarkGray,
            },
            subtabs: SubtabsTheme {
                active: Color::Cyan,
                inactive: Color::DarkGray,
            },
            status_bar: StatusBarTheme {
                error_bg: Color::Red,
                error_fg: Color::White,
                diff_bg: Color::DarkGray,
                diff_fg: Color::White,
                interact_bg: Color::Green,
                navigate_bg: Color::Yellow,
                mode_fg: Color::Black,
            },
            footer: FooterTheme {
                key: Color::Yellow,
                description: Color::Gray,
            },
            diff: DiffTheme {
                border: Color::Cyan,
                empty_text: Color::DarkGray,
            },
            dialog: DialogTheme {
                new_ws_border: Color::Yellow,
                new_ws_active: Color::Yellow,
                new_ws_inactive: Color::DarkGray,
                delete_border: Color::Red,
                delete_text: Color::White,
                delete_name: Color::Yellow,
                delete_yes: Color::Red,
                delete_no: Color::Green,
                delete_cancel: Color::DarkGray,
            },
            help: HelpTheme {
                border: Color::Cyan,
            },
            general: GeneralTheme {
                welcome_text: Color::Gray,
                muted_text: Color::DarkGray,
            },
            fuzzy_search: FuzzySearchTheme {
                border: Color::Cyan,
                input_text: Color::White,
                match_highlight: Color::Yellow,
                result_text: Color::Gray,
                selected_bg: Color::DarkGray,
                count_text: Color::DarkGray,
            },
        }
    }
}

impl Theme {
    fn from_toml(t: ThemeToml) -> Self {
        let d = Self::default();
        Self {
            border: BorderTheme {
                active_interact: resolve(&t.border.active_interact, d.border.active_interact),
                active_navigate: resolve(&t.border.active_navigate, d.border.active_navigate),
                inactive: resolve(&t.border.inactive, d.border.inactive),
            },
            workspace_list: WorkspaceListTheme {
                empty_text: resolve(&t.workspace_list.empty_text, d.workspace_list.empty_text),
                name_active: resolve(&t.workspace_list.name_active, d.workspace_list.name_active),
                name_inactive: resolve(
                    &t.workspace_list.name_inactive,
                    d.workspace_list.name_inactive,
                ),
                detail_selected: resolve(
                    &t.workspace_list.detail_selected,
                    d.workspace_list.detail_selected,
                ),
                detail_normal: resolve(
                    &t.workspace_list.detail_normal,
                    d.workspace_list.detail_normal,
                ),
                selected_bg: resolve(&t.workspace_list.selected_bg, d.workspace_list.selected_bg),
            },
            file_list: FileListTheme {
                empty_text: resolve(&t.file_list.empty_text, d.file_list.empty_text),
                modified: resolve(&t.file_list.modified, d.file_list.modified),
                added: resolve(&t.file_list.added, d.file_list.added),
                deleted: resolve(&t.file_list.deleted, d.file_list.deleted),
                renamed: resolve(&t.file_list.renamed, d.file_list.renamed),
                untracked: resolve(&t.file_list.untracked, d.file_list.untracked),
                conflicted: resolve(&t.file_list.conflicted, d.file_list.conflicted),
                staged: resolve(&t.file_list.staged, d.file_list.staged),
                staged_modified: resolve(&t.file_list.staged_modified, d.file_list.staged_modified),
                file_path: resolve(&t.file_list.file_path, d.file_list.file_path),
                selected_bg: resolve(&t.file_list.selected_bg, d.file_list.selected_bg),
            },
            tabs: TabsTheme {
                active: resolve(&t.tabs.active, d.tabs.active),
                inactive: resolve(&t.tabs.inactive, d.tabs.inactive),
            },
            subtabs: SubtabsTheme {
                active: resolve(&t.subtabs.active, d.subtabs.active),
                inactive: resolve(&t.subtabs.inactive, d.subtabs.inactive),
            },
            status_bar: StatusBarTheme {
                error_bg: resolve(&t.status_bar.error_bg, d.status_bar.error_bg),
                error_fg: resolve(&t.status_bar.error_fg, d.status_bar.error_fg),
                diff_bg: resolve(&t.status_bar.diff_bg, d.status_bar.diff_bg),
                diff_fg: resolve(&t.status_bar.diff_fg, d.status_bar.diff_fg),
                interact_bg: resolve(&t.status_bar.interact_bg, d.status_bar.interact_bg),
                navigate_bg: resolve(&t.status_bar.navigate_bg, d.status_bar.navigate_bg),
                mode_fg: resolve(&t.status_bar.mode_fg, d.status_bar.mode_fg),
            },
            footer: FooterTheme {
                key: resolve(&t.footer.key, d.footer.key),
                description: resolve(&t.footer.description, d.footer.description),
            },
            diff: DiffTheme {
                border: resolve(&t.diff.border, d.diff.border),
                empty_text: resolve(&t.diff.empty_text, d.diff.empty_text),
            },
            dialog: DialogTheme {
                new_ws_border: resolve(&t.dialog.new_ws_border, d.dialog.new_ws_border),
                new_ws_active: resolve(&t.dialog.new_ws_active, d.dialog.new_ws_active),
                new_ws_inactive: resolve(&t.dialog.new_ws_inactive, d.dialog.new_ws_inactive),
                delete_border: resolve(&t.dialog.delete_border, d.dialog.delete_border),
                delete_text: resolve(&t.dialog.delete_text, d.dialog.delete_text),
                delete_name: resolve(&t.dialog.delete_name, d.dialog.delete_name),
                delete_yes: resolve(&t.dialog.delete_yes, d.dialog.delete_yes),
                delete_no: resolve(&t.dialog.delete_no, d.dialog.delete_no),
                delete_cancel: resolve(&t.dialog.delete_cancel, d.dialog.delete_cancel),
            },
            help: HelpTheme {
                border: resolve(&t.help.border, d.help.border),
            },
            general: GeneralTheme {
                welcome_text: resolve(&t.general.welcome_text, d.general.welcome_text),
                muted_text: resolve(&t.general.muted_text, d.general.muted_text),
            },
            fuzzy_search: FuzzySearchTheme {
                border: resolve(&t.fuzzy_search.border, d.fuzzy_search.border),
                input_text: resolve(&t.fuzzy_search.input_text, d.fuzzy_search.input_text),
                match_highlight: resolve(
                    &t.fuzzy_search.match_highlight,
                    d.fuzzy_search.match_highlight,
                ),
                result_text: resolve(&t.fuzzy_search.result_text, d.fuzzy_search.result_text),
                selected_bg: resolve(&t.fuzzy_search.selected_bg, d.fuzzy_search.selected_bg),
                count_text: resolve(&t.fuzzy_search.count_text, d.fuzzy_search.count_text),
            },
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ConfigToml {
    theme: Option<String>,
}

pub fn load() -> Theme {
    let config_dir = match dirs::config_dir() {
        Some(d) => d.join("piki-multi"),
        None => return Theme::default(),
    };

    let config_path = config_dir.join("config.toml");
    let theme_name = std::fs::read_to_string(&config_path)
        .ok()
        .and_then(|s| toml::from_str::<ConfigToml>(&s).ok())
        .and_then(|c| c.theme)
        .unwrap_or_else(|| "default".to_string());

    let theme_path = config_dir
        .join("themes")
        .join(format!("{}.toml", theme_name));
    let theme_toml = match std::fs::read_to_string(&theme_path) {
        Ok(s) => match toml::from_str::<ThemeToml>(&s) {
            Ok(t) => t,
            Err(_) => return Theme::default(),
        },
        Err(_) => return Theme::default(),
    };

    Theme::from_toml(theme_toml)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_color_named() {
        assert_eq!(parse_color("Red"), Color::Red);
        assert_eq!(parse_color("Green"), Color::Green);
        assert_eq!(parse_color("DarkGray"), Color::DarkGray);
    }

    #[test]
    fn test_parse_color_hex() {
        assert_eq!(parse_color("#ff0000"), Color::Rgb(255, 0, 0));
        assert_eq!(parse_color("#00ff00"), Color::Rgb(0, 255, 0));
        assert_eq!(parse_color("#0000ff"), Color::Rgb(0, 0, 255));
    }

    #[test]
    fn test_parse_color_unknown() {
        assert_eq!(parse_color("garbage"), Color::White);
    }

    #[test]
    fn test_default_theme_matches_hardcoded() {
        let t = Theme::default();
        assert_eq!(t.border.active_interact, Color::Green);
        assert_eq!(t.border.active_navigate, Color::Yellow);
        assert_eq!(t.border.inactive, Color::DarkGray);
        assert_eq!(t.file_list.modified, Color::Yellow);
        assert_eq!(t.subtabs.active, Color::Cyan);
    }

    #[test]
    fn test_partial_toml_override() {
        let toml_str = "[border]\nactive_interact = \"#ff0000\"\n";
        let t: ThemeToml = toml::from_str(toml_str).unwrap();
        let theme = Theme::from_toml(t);
        assert_eq!(theme.border.active_interact, Color::Rgb(255, 0, 0));
        // Unset fields keep defaults
        assert_eq!(theme.border.active_navigate, Color::Yellow);
        assert_eq!(theme.border.inactive, Color::DarkGray);
    }
}
