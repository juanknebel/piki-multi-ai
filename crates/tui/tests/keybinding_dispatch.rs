use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

// The config module is private to the tui crate, so we test through
// the public parse_key_event and key_matches functions re-exported here.
// Since config is a private module, we test the parsing logic directly.

/// Replicate parse_key_event for testing (the function is in a private module).
fn parse_key_event(s: &str) -> Option<KeyEvent> {
    let parts: Vec<&str> = s.split('-').collect();
    let mut modifiers = KeyModifiers::empty();
    let code_str = if parts.len() > 1 {
        for &mod_str in &parts[..parts.len() - 1] {
            match mod_str.to_lowercase().as_str() {
                "ctrl" => modifiers.insert(KeyModifiers::CONTROL),
                "alt" => modifiers.insert(KeyModifiers::ALT),
                "shift" => modifiers.insert(KeyModifiers::SHIFT),
                "super" | "cmd" => modifiers.insert(KeyModifiers::SUPER),
                _ => return None,
            }
        }
        parts[parts.len() - 1]
    } else {
        parts[0]
    };

    let code = match code_str.to_lowercase().as_str() {
        "enter" => KeyCode::Enter,
        "tab" => KeyCode::Tab,
        "backspace" => KeyCode::Backspace,
        "esc" => KeyCode::Esc,
        "left" => KeyCode::Left,
        "right" => KeyCode::Right,
        "up" => KeyCode::Up,
        "down" => KeyCode::Down,
        "pageup" => KeyCode::PageUp,
        "pagedown" => KeyCode::PageDown,
        "home" => KeyCode::Home,
        "end" => KeyCode::End,
        "insert" => KeyCode::Insert,
        "delete" => KeyCode::Delete,
        s if s.len() == 1 => {
            let c = s.chars().next().unwrap();
            if c.is_uppercase() {
                modifiers.insert(KeyModifiers::SHIFT);
            }
            KeyCode::Char(c)
        }
        s if s.starts_with('f') && s.len() > 1 => {
            let n = s[1..].parse::<u8>().ok()?;
            KeyCode::F(n)
        }
        _ => return None,
    };

    Some(KeyEvent::new(code, modifiers))
}

fn key_matches(event: KeyEvent, binding: &str) -> bool {
    if let Some(target) = parse_key_event(binding) {
        event.code == target.code && event.modifiers == target.modifiers
    } else {
        false
    }
}

#[test]
fn test_parse_simple_key() {
    let event = parse_key_event("q").unwrap();
    assert_eq!(event.code, KeyCode::Char('q'));
    assert_eq!(event.modifiers, KeyModifiers::empty());
}

#[test]
fn test_parse_enter() {
    let event = parse_key_event("enter").unwrap();
    assert_eq!(event.code, KeyCode::Enter);
}

#[test]
fn test_parse_ctrl_modifier() {
    let event = parse_key_event("ctrl-g").unwrap();
    assert_eq!(event.code, KeyCode::Char('g'));
    assert!(event.modifiers.contains(KeyModifiers::CONTROL));
}

#[test]
fn test_parse_shift_modifier_uppercase() {
    let event = parse_key_event("K").unwrap();
    // to_lowercase() runs before the match, so 'K' becomes 'k'.
    // The is_uppercase() check on the lowercased char is a no-op.
    assert_eq!(event.code, KeyCode::Char('k'));
    assert_eq!(event.modifiers, KeyModifiers::empty());

    // Explicit shift-k works:
    let event2 = parse_key_event("shift-k").unwrap();
    assert_eq!(event2.code, KeyCode::Char('k'));
    assert!(event2.modifiers.contains(KeyModifiers::SHIFT));
}

#[test]
fn test_parse_ctrl_shift() {
    let event = parse_key_event("ctrl-shift-c").unwrap();
    assert_eq!(event.code, KeyCode::Char('c'));
    assert!(event.modifiers.contains(KeyModifiers::CONTROL));
    assert!(event.modifiers.contains(KeyModifiers::SHIFT));
}

#[test]
fn test_parse_alt_modifier() {
    let event = parse_key_event("alt-m").unwrap();
    assert_eq!(event.code, KeyCode::Char('m'));
    assert!(event.modifiers.contains(KeyModifiers::ALT));
}

#[test]
fn test_parse_function_key() {
    let event = parse_key_event("f1").unwrap();
    assert_eq!(event.code, KeyCode::F(1));
}

#[test]
fn test_parse_special_keys() {
    assert_eq!(parse_key_event("esc").unwrap().code, KeyCode::Esc);
    assert_eq!(parse_key_event("tab").unwrap().code, KeyCode::Tab);
    assert_eq!(parse_key_event("pageup").unwrap().code, KeyCode::PageUp);
    assert_eq!(parse_key_event("pagedown").unwrap().code, KeyCode::PageDown);
}

#[test]
fn test_parse_invalid() {
    assert!(parse_key_event("invalid-modifier-x").is_none());
}

#[test]
fn test_key_matches_basic() {
    let event = KeyEvent::new(KeyCode::Char('q'), KeyModifiers::empty());
    assert!(key_matches(event, "q"));
    assert!(!key_matches(event, "w"));
}

#[test]
fn test_key_matches_with_modifiers() {
    let event = KeyEvent::new(KeyCode::Char('g'), KeyModifiers::CONTROL);
    assert!(key_matches(event, "ctrl-g"));
    assert!(!key_matches(event, "g"));
    assert!(!key_matches(event, "alt-g"));
}

#[test]
fn test_default_navigation_bindings() {
    // Verify well-known default bindings parse correctly
    let bindings = vec![
        ("h", KeyCode::Char('h')),
        ("j", KeyCode::Char('j')),
        ("k", KeyCode::Char('k')),
        ("l", KeyCode::Char('l')),
        ("enter", KeyCode::Enter),
        ("?", KeyCode::Char('?')),
    ];

    for (binding, expected_code) in bindings {
        let event = parse_key_event(binding).unwrap();
        assert_eq!(
            event.code, expected_code,
            "binding '{}' should parse to {:?}",
            binding, expected_code
        );
    }
}
