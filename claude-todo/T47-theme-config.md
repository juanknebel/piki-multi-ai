# T47 — Externalize color configuration into TOML theme files

**Status**: DONE
**Blocked by**: —

## Problem

All colors are hardcoded as `Color::` constants across `src/ui/layout.rs` (39 usages),
`src/ui/subtabs.rs` (3), and `src/ui/diff.rs` (1). Users cannot customize the look
and feel without modifying source code.

## Solution

### Config structure

```
~/.config/piki-multi/
├── config.toml          # General config, includes `theme = "nord"`
└── themes/
    ├── default.toml     # Built-in default theme (shipped as fallback)
    └── nord.toml        # Example custom theme
```

**`config.toml`**:
```toml
theme = "default"
```

**`themes/default.toml`** — Semantic color roles:
```toml
[border]
active_interact = "Green"       # Pane border when interacting (green border)
active_navigate = "Yellow"      # Pane border when navigating (yellow border)
inactive = "DarkGray"           # Pane border when inactive

[status_bar]
interact_bg = "Green"           # Status bar bg in interact mode
navigate_bg = "Yellow"          # Status bar bg in navigate mode
fg = "Black"                    # Status bar text
error_bg = "Red"                # Error message bg
error_fg = "White"              # Error message fg
diff_bg = "DarkGray"            # Diff mode status bar bg
diff_fg = "White"               # Diff mode status bar fg

[tabs]
active = "Yellow"               # Active workspace tab
inactive = "DarkGray"           # Inactive workspace tabs

[subtabs]
active = "Cyan"                 # Active AI provider sub-tab
inactive = "DarkGray"           # Inactive AI provider sub-tabs

[workspace_list]
name_active = "White"           # Active workspace name
name_inactive = "Gray"          # Inactive workspace name
detail = "DarkGray"             # Status, file count, description, path
detail_selected = "Gray"        # Same when selected (lighter for visibility)
selected_bg = "DarkGray"        # Selected item background
empty_text = "DarkGray"         # "Press [n] to create" prompt

[file_list]
modified = "Yellow"             # M status
added = "Green"                 # A status
deleted = "Red"                 # D status
renamed = "Cyan"                # R status
untracked = "DarkGray"          # ? status
conflicted = "Magenta"          # C status
staged = "Green"                # S status
staged_modified = "Yellow"      # SM status
path = "White"                  # File path text
selected_bg = "DarkGray"        # Selected item background
empty_text = "DarkGray"         # "No files changed" prompt

[diff]
border = "Cyan"                 # Diff overlay border
empty_text = "DarkGray"         # No diff prompt

[dialog]
new_ws_border = "Yellow"        # New workspace dialog border/title
new_ws_field_active = "Yellow"  # Active field text
new_ws_field_inactive = "DarkGray"  # Inactive field text
delete_border = "Red"           # Delete confirmation border/title
delete_text = "White"           # Delete dialog labels
delete_name = "Yellow"          # Workspace name in delete dialog
delete_yes = "Red"              # "[y] Yes" option
delete_no = "Green"             # "[n] No" option
delete_cancel = "DarkGray"      # "[Esc] Cancel" option

[help]
border = "Cyan"                 # Help overlay border/title

[footer]
key = "Yellow"                  # Keybinding brackets
desc = "Gray"                   # Keybinding descriptions

[general]
text = "White"                  # General text
text_secondary = "Gray"         # Secondary text (welcome, etc.)
text_muted = "DarkGray"         # Muted text (command not found, etc.)
```

### Color format

Support these formats in TOML values:
- Named colors: `"Red"`, `"Green"`, `"Yellow"`, `"Blue"`, `"Cyan"`, `"Magenta"`,
  `"White"`, `"Black"`, `"Gray"`, `"DarkGray"`, `"LightRed"`, etc.
- Hex RGB: `"#88c0d0"`, `"#2e3440"`
- ANSI 256: `{ index = 148 }`

### Implementation

1. **`src/theme.rs`** — New module:
   - `Theme` struct with all semantic color fields, derives `Deserialize`
   - `impl Default for Theme` with current hardcoded colors
   - `load_theme(name: &str) -> Theme` — reads from `~/.config/piki-multi/themes/{name}.toml`,
     falls back to `Default`
   - `load_config() -> String` — reads `~/.config/piki-multi/config.toml`, returns theme name
   - Color parsing: `"#rrggbb"` → `Color::Rgb(r,g,b)`, named → `Color::Yellow`, etc.

2. **`src/app.rs`** — Add `theme: Theme` field to `App`

3. **`src/ui/layout.rs`** — Replace all `Color::` constants with `app.theme.*` lookups

4. **`src/ui/subtabs.rs`** — Accept `&Theme` param, use `theme.subtabs.*`

5. **`src/ui/diff.rs`** — Accept `&Theme` param, use `theme.diff.*`

### Dependencies

- `toml` crate for deserialization (add to Cargo.toml)

## Files to create/modify

- `Cargo.toml` — add `toml` dependency
- `src/theme.rs` — new module with Theme struct, loader, color parser
- `src/main.rs` — add `mod theme`, load theme on startup, pass to App
- `src/app.rs` — add `theme: Theme` field
- `src/ui/layout.rs` — replace hardcoded colors with theme lookups
- `src/ui/subtabs.rs` — use theme colors
- `src/ui/diff.rs` — use theme colors
- `~/.config/piki-multi/themes/default.toml` — ship as example/reference
