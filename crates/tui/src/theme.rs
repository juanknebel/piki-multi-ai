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
    dialog: DialogToml,
    help: HelpToml,
    general: GeneralToml,
    fuzzy_search: FuzzySearchToml,
    selection: SelectionToml,
    status: StatusToml,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct BorderToml {
    /// Focused-pane border color. Falls back to the deprecated
    /// `active_interact` (then `active_navigate`) for older theme files.
    active: Option<String>,
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
    group_header_bg: Option<String>,
    alt_bg: Option<String>,
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
    multi_select_bg: Option<String>,
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
    active_fg: Option<String>,
    inactive: Option<String>,
    inactive_bg: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct StatusBarToml {
    error_bg: Option<String>,
    error_fg: Option<String>,
    /// Background for the [PREFIX]/[SCROLL] chips. Falls back to the
    /// deprecated `interact_bg` for older theme files.
    prefix_bg: Option<String>,
    interact_bg: Option<String>,
    navigate_bg: Option<String>,
    mode_fg: Option<String>,
    separator_fg: Option<String>,
    text_fg: Option<String>,
    toast_info: Option<String>,
    toast_success: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct FooterToml {
    key: Option<String>,
    description: Option<String>,
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
    scrollbar_thumb: Option<String>,
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

#[derive(Deserialize, Default)]
#[serde(default)]
struct SelectionToml {
    bg: Option<String>,
    fg: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct StatusToml {
    running: Option<String>,
    needs_you: Option<String>,
    done: Option<String>,
    error: Option<String>,
    exited: Option<String>,
}

// ── Cabina palette (primitive tokens) ──

/// Build a Color from a `0xRRGGBB` literal.
const fn rgb(hex: u32) -> Color {
    Color::Rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

/// Primitive color tokens of the "Cabina" visual language. The roles in
/// [`Theme`] derive from these; render code never reads the palette directly.
///
/// Two rules govern the mapping:
/// - `iris` (the single accent) marks focus/interactivity and never state.
/// - The semantic colors (`ok`/`warn`/`err`/`info`) mark state and never focus.
pub struct Palette {
    /// Canvas: pane and terminal background.
    pub bg0: Color,
    /// Barely raised: alternate rows.
    pub bg1: Color,
    /// Raised: group headers, inactive tabs, unfocused selection.
    pub bg2: Color,
    /// Overlays and popups.
    pub bg3: Color,
    /// Borders of unfocused panes.
    pub line: Color,
    /// Borders of neutral dialogs.
    pub line_strong: Color,
    /// Primary text: names, values, the selected thing.
    pub fg0: Color,
    /// Secondary text: regular content.
    pub fg1: Color,
    /// Muted: details, unfocused titles, textual separators.
    pub fg2: Color,
    /// Ghost: placeholders, counters, the inactive.
    pub fg3: Color,
    /// THE accent: focus, active tab, matches, keys, cursor.
    pub iris: Color,
    /// Selection background in the focused pane.
    pub iris_wash: Color,
    /// Done, staged, additions, success toasts.
    pub ok: Color,
    /// "Needs you": permission prompts, idle with news, modified files.
    pub warn: Color,
    /// Errors, deletions, conflicts, destructive confirms.
    pub err: Color,
    /// Running activity, informative toasts, renames.
    pub info: Color,
}

impl Default for Palette {
    fn default() -> Self {
        Self {
            bg0: rgb(0x14141C),
            bg1: rgb(0x1B1B26),
            bg2: rgb(0x232331),
            bg3: rgb(0x2A2A3C),
            line: rgb(0x3D3D54),
            line_strong: rgb(0x4C4C68),
            fg0: rgb(0xECECF6),
            fg1: rgb(0xB4B4CC),
            fg2: rgb(0x7C7C99),
            fg3: rgb(0x56566E),
            iris: rgb(0xA78BFA),
            iris_wash: rgb(0x322D4D),
            ok: rgb(0x9BD186),
            warn: rgb(0xE8B15E),
            err: rgb(0xF0717D),
            info: rgb(0x84B0F2),
        }
    }
}

// ── Resolved Theme (Color values) ──

pub struct BorderTheme {
    /// Border color of the focused pane
    pub active: Color,
    pub inactive: Color,
}

pub struct WorkspaceListTheme {
    pub empty_text: Color,
    pub name_active: Color,
    pub name_inactive: Color,
    pub detail_selected: Color,
    pub detail_normal: Color,
    pub selected_bg: Color,
    pub group_header_bg: Color,
    pub alt_bg: Color,
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
    pub multi_select_bg: Color,
}

pub struct TabsTheme {
    pub active: Color,
    pub inactive: Color,
}

pub struct SubtabsTheme {
    /// Background of the active tab block.
    pub active: Color,
    /// Text on the active tab block.
    pub active_fg: Color,
    /// Text of inactive tab blocks.
    pub inactive: Color,
    /// Background of inactive tab blocks.
    pub inactive_bg: Color,
}

pub struct StatusBarTheme {
    pub error_bg: Color,
    pub error_fg: Color,
    /// Background for the PREFIX/SCROLL mode chips
    pub prefix_bg: Color,
    /// Quiet surface of the whole bar
    pub navigate_bg: Color,
    /// Text on the mode chip
    pub mode_fg: Color,
    pub separator_fg: Color,
    /// Body text of the bar (branch, counters)
    pub text_fg: Color,
    /// Glyph color of info toasts
    pub toast_info: Color,
    /// Glyph color of success toasts
    pub toast_success: Color,
}

pub struct FooterTheme {
    pub key: Color,
    pub description: Color,
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
    pub scrollbar_thumb: Color,
}

pub struct FuzzySearchTheme {
    pub border: Color,
    pub input_text: Color,
    pub match_highlight: Color,
    pub result_text: Color,
    pub selected_bg: Color,
    pub count_text: Color,
}

pub struct SelectionTheme {
    pub bg: Color,
    pub fg: Color,
}

/// Unified agent/process status vocabulary. Every surface that shows agent
/// state (Agents pane, tab bar, dashboard, workspace switcher) reads these
/// tokens; the glyphs live in `ui::cli_agent_status_view`.
pub struct StatusTheme {
    /// Activity: the running spinner (Agents pane only) and live processes.
    pub running: Color,
    /// "Your turn": permission prompts and idle-with-news. Propagates to
    /// ambient chrome (tabs, sidebar, header) — activity does not.
    pub needs_you: Color,
    pub done: Color,
    pub error: Color,
    /// Exited / not-started processes; almost invisible on purpose.
    pub exited: Color,
}

pub struct Theme {
    pub border: BorderTheme,
    pub workspace_list: WorkspaceListTheme,
    pub file_list: FileListTheme,
    pub tabs: TabsTheme,
    pub subtabs: SubtabsTheme,
    pub status_bar: StatusBarTheme,
    pub footer: FooterTheme,
    pub dialog: DialogTheme,
    pub help: HelpTheme,
    pub general: GeneralTheme,
    pub fuzzy_search: FuzzySearchTheme,
    pub selection: SelectionTheme,
    pub status: StatusTheme,
}

impl Default for Theme {
    fn default() -> Self {
        Self::from_palette(&Palette::default())
    }
}

impl Theme {
    /// Derive every role from the primitive palette. This is the single
    /// place that decides what each token *means* visually.
    pub fn from_palette(p: &Palette) -> Self {
        Self {
            border: BorderTheme {
                active: p.iris,
                inactive: p.line,
            },
            workspace_list: WorkspaceListTheme {
                empty_text: p.fg3,
                name_active: p.fg0,
                name_inactive: p.fg1,
                detail_selected: p.fg2,
                detail_normal: p.fg3,
                selected_bg: p.iris_wash,
                group_header_bg: p.bg2,
                alt_bg: p.bg1,
            },
            file_list: FileListTheme {
                empty_text: p.fg3,
                modified: p.warn,
                added: p.ok,
                deleted: p.err,
                renamed: p.info,
                untracked: p.fg3,
                conflicted: p.err,
                staged: p.ok,
                staged_modified: p.warn,
                file_path: p.fg0,
                selected_bg: p.iris_wash,
                multi_select_bg: p.bg3,
            },
            tabs: TabsTheme {
                active: p.iris,
                inactive: p.fg2,
            },
            subtabs: SubtabsTheme {
                active: p.iris,
                active_fg: p.bg0,
                // A raised surface + muted text so an inactive tab reads as a
                // clearly distinct (but secondary) block, not a smudge against
                // the bar background.
                inactive: p.fg2,
                inactive_bg: p.bg2,
            },
            status_bar: StatusBarTheme {
                error_bg: p.err,
                error_fg: p.bg0,
                prefix_bg: p.iris,
                navigate_bg: p.bg2,
                mode_fg: p.bg0,
                separator_fg: p.fg3,
                text_fg: p.fg1,
                toast_info: p.info,
                toast_success: p.ok,
            },
            footer: FooterTheme {
                key: p.iris,
                description: p.fg2,
            },
            dialog: DialogTheme {
                new_ws_border: p.line_strong,
                new_ws_active: p.iris,
                new_ws_inactive: p.fg2,
                delete_border: p.err,
                delete_text: p.fg1,
                delete_name: p.fg0,
                delete_yes: p.err,
                // The safe action stays neutral: green would say "this is the
                // good one", and semantics never editorialize a choice.
                delete_no: p.fg1,
                delete_cancel: p.fg3,
            },
            help: HelpTheme {
                border: p.line_strong,
            },
            general: GeneralTheme {
                welcome_text: p.fg1,
                muted_text: p.fg2,
                scrollbar_thumb: p.fg2,
            },
            fuzzy_search: FuzzySearchTheme {
                border: p.line_strong,
                input_text: p.fg0,
                match_highlight: p.iris,
                result_text: p.fg1,
                selected_bg: p.iris_wash,
                count_text: p.fg3,
            },
            selection: SelectionTheme {
                bg: p.iris_wash,
                fg: p.fg0,
            },
            status: StatusTheme {
                running: p.info,
                needs_you: p.warn,
                done: p.ok,
                error: p.err,
                exited: p.fg3,
            },
        }
    }
}

impl Theme {
    fn from_toml(t: ThemeToml) -> Self {
        let d = Self::default();
        Self {
            border: BorderTheme {
                active: resolve(
                    &t.border
                        .active
                        .clone()
                        .or_else(|| t.border.active_interact.clone())
                        .or_else(|| t.border.active_navigate.clone()),
                    d.border.active,
                ),
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
                group_header_bg: resolve(
                    &t.workspace_list.group_header_bg,
                    d.workspace_list.group_header_bg,
                ),
                alt_bg: resolve(&t.workspace_list.alt_bg, d.workspace_list.alt_bg),
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
                multi_select_bg: resolve(
                    &t.file_list.multi_select_bg,
                    d.file_list.multi_select_bg,
                ),
            },
            tabs: TabsTheme {
                active: resolve(&t.tabs.active, d.tabs.active),
                inactive: resolve(&t.tabs.inactive, d.tabs.inactive),
            },
            subtabs: SubtabsTheme {
                active: resolve(&t.subtabs.active, d.subtabs.active),
                active_fg: resolve(&t.subtabs.active_fg, d.subtabs.active_fg),
                inactive: resolve(&t.subtabs.inactive, d.subtabs.inactive),
                inactive_bg: resolve(&t.subtabs.inactive_bg, d.subtabs.inactive_bg),
            },
            status_bar: StatusBarTheme {
                error_bg: resolve(&t.status_bar.error_bg, d.status_bar.error_bg),
                error_fg: resolve(&t.status_bar.error_fg, d.status_bar.error_fg),
                prefix_bg: resolve(
                    &t.status_bar
                        .prefix_bg
                        .clone()
                        .or_else(|| t.status_bar.interact_bg.clone()),
                    d.status_bar.prefix_bg,
                ),
                navigate_bg: resolve(&t.status_bar.navigate_bg, d.status_bar.navigate_bg),
                mode_fg: resolve(&t.status_bar.mode_fg, d.status_bar.mode_fg),
                separator_fg: resolve(&t.status_bar.separator_fg, d.status_bar.separator_fg),
                text_fg: resolve(&t.status_bar.text_fg, d.status_bar.text_fg),
                toast_info: resolve(&t.status_bar.toast_info, d.status_bar.toast_info),
                toast_success: resolve(&t.status_bar.toast_success, d.status_bar.toast_success),
            },
            footer: FooterTheme {
                key: resolve(&t.footer.key, d.footer.key),
                description: resolve(&t.footer.description, d.footer.description),
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
                scrollbar_thumb: resolve(
                    &t.general.scrollbar_thumb,
                    d.general.scrollbar_thumb,
                ),
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
            selection: SelectionTheme {
                bg: resolve(&t.selection.bg, d.selection.bg),
                fg: resolve(&t.selection.fg, d.selection.fg),
            },
            status: StatusTheme {
                running: resolve(&t.status.running, d.status.running),
                needs_you: resolve(&t.status.needs_you, d.status.needs_you),
                done: resolve(&t.status.done, d.status.done),
                error: resolve(&t.status.error, d.status.error),
                exited: resolve(&t.status.exited, d.status.exited),
            },
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct ConfigToml {
    theme: Option<String>,
}

pub fn load_from(paths: &piki_core::paths::DataPaths) -> Theme {
    let config_dir = paths.config_dir();

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
    fn test_default_theme_derives_from_palette() {
        let p = Palette::default();
        let t = Theme::default();
        // The accent marks focus/interactivity...
        assert_eq!(t.border.active, p.iris);
        assert_eq!(t.subtabs.active, p.iris);
        assert_eq!(t.footer.key, p.iris);
        assert_eq!(t.fuzzy_search.match_highlight, p.iris);
        // ...and semantics mark state.
        assert_eq!(t.file_list.modified, p.warn);
        assert_eq!(t.file_list.added, p.ok);
        assert_eq!(t.file_list.deleted, p.err);
        assert_eq!(t.border.inactive, p.line);
    }

    #[test]
    fn test_partial_toml_override() {
        // The deprecated active_interact key still feeds border.active
        let toml_str = "[border]\nactive_interact = \"#ff0000\"\n";
        let t: ThemeToml = toml::from_str(toml_str).unwrap();
        let theme = Theme::from_toml(t);
        assert_eq!(theme.border.active, Color::Rgb(255, 0, 0));
        // Unset fields keep defaults
        assert_eq!(theme.border.inactive, Theme::default().border.inactive);

        // The new key wins over the deprecated one
        let toml_str = "[border]\nactive = \"#00ff00\"\nactive_interact = \"#ff0000\"\n";
        let t: ThemeToml = toml::from_str(toml_str).unwrap();
        let theme = Theme::from_toml(t);
        assert_eq!(theme.border.active, Color::Rgb(0, 255, 0));
    }

    #[test]
    fn test_empty_toml_produces_defaults() {
        let t: ThemeToml = toml::from_str("").unwrap();
        let theme = Theme::from_toml(t);
        let default = Theme::default();
        assert_eq!(theme.border.active, default.border.active);
        assert_eq!(theme.border.inactive, default.border.inactive);
        assert_eq!(theme.file_list.modified, default.file_list.modified);
        assert_eq!(theme.footer.key, default.footer.key);
    }

    #[test]
    fn test_invalid_hex_falls_back_to_white() {
        // "#gggggg" is 7 chars starting with '#' so it enters the hex branch,
        // but invalid hex digits default to 255 → Rgb(255,255,255)
        assert_eq!(parse_color("#gggggg"), Color::Rgb(255, 255, 255));
        // "#short" is only 6 chars → doesn't match hex branch → falls through to White
        assert_eq!(parse_color("#short"), Color::White);
        assert_eq!(parse_color("not_a_color"), Color::White);
    }

    #[test]
    fn test_section_override_preserves_other_sections() {
        let toml_str = "[file_list]\nmodified = \"#aabbcc\"\n";
        let t: ThemeToml = toml::from_str(toml_str).unwrap();
        let theme = Theme::from_toml(t);
        // Overridden
        assert_eq!(theme.file_list.modified, Color::Rgb(0xaa, 0xbb, 0xcc));
        // Other sections untouched
        let d = Theme::default();
        assert_eq!(theme.border.active, d.border.active);
        assert_eq!(theme.footer.key, d.footer.key);
    }
}
