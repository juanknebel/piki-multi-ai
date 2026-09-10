//! Cross-frontend settings that live in the shared SQLite database instead
//! of `config.toml` — the desktop's Settings ▸ General tab writes them, and
//! BOTH frontends read them at startup.
//!
//! Precedence, highest first: **database override > `config.toml` >
//! built-in default**. An override is only ever an explicit user choice made
//! in the UI (`Some`); everything else stays `None` so `config.toml` keeps
//! applying to it. "Reset" = clearing the overrides, which brings
//! `config.toml` back — the file is never rewritten by the UI.
//!
//! Storage: one JSON document under [`PREF_KEY`] in `UiPrefsStorage`
//! (the `ui_preferences` table of `piki.db`), which the TUI and the desktop
//! already share. Only the fields below are overridable; the sound file
//! paths, keybindings, themes etc. remain `config.toml`-only.
//!
//! Readers:
//! - TUI — `App::new` (`crates/tui/src/app.rs`) folds the overrides into its
//!   `Config` right after `Config::load_from`, so `config.sessions.enabled`
//!   (checked in `event_loop.rs` before connecting the daemon) and
//!   `config.notifications` are already the effective values.
//! - Desktop — `main.rs` calls [`resolve`] once at startup (sessions gate +
//!   `NotificationsConfig::apply`); `set_app_settings` re-applies the
//!   notification layer live, the sessions choice takes effect on restart.

use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::notifications::NotificationsConfig;
use crate::storage::UiPrefsStorage;

/// `ui_preferences` key holding the JSON document.
pub const PREF_KEY: &str = "app_settings";

/// The overridable subset. `None` = "not chosen in the UI, use config.toml".
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// `[sessions] enabled` — persistent-session daemon on/off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sessions_enabled: Option<bool>,
    /// `[notifications] delivery` — `"off" | "system" | "terminal"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notification_delivery: Option<String>,
    /// `[notifications] sound` — the built-in chimes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sound: Option<bool>,
}

/// Valid `notification_delivery` values (mirrors `NotificationsConfig::parsed_delivery`).
pub const DELIVERY_VALUES: [&str; 3] = ["off", "system", "terminal"];

impl AppSettings {
    /// Parse the stored document. Anything unreadable — missing, corrupt,
    /// an older shape — yields no overrides, never an error: a broken
    /// preference row must not change how the app starts.
    pub fn from_json(json: &str) -> Self {
        serde_json::from_str(json).unwrap_or_default()
    }

    /// Load from the shared preferences (`None` storage = no overrides).
    pub fn load(prefs: Option<&dyn UiPrefsStorage>) -> Self {
        let Some(prefs) = prefs else {
            return Self::default();
        };
        match prefs.get_preference(PREF_KEY) {
            Ok(Some(json)) => Self::from_json(&json),
            Ok(None) => Self::default(),
            Err(e) => {
                tracing::warn!(%e, "could not read app settings from the database; using config.toml");
                Self::default()
            }
        }
    }

    /// Persist the document. An unknown `notification_delivery` is dropped
    /// (kept `None`) rather than stored, so a bad value can never stick.
    pub fn save(&self, prefs: &dyn UiPrefsStorage) -> anyhow::Result<()> {
        let json = serde_json::to_string(&self.sanitized())?;
        prefs.set_preference(PREF_KEY, &json)
    }

    /// True when nothing is overridden (config.toml applies to everything).
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Copy with an invalid delivery value cleared.
    pub fn sanitized(&self) -> Self {
        let mut s = self.clone();
        if let Some(d) = &s.notification_delivery
            && !DELIVERY_VALUES.contains(&d.as_str())
        {
            tracing::warn!(delivery = %d, "ignoring unknown notification delivery override");
            s.notification_delivery = None;
        }
        s
    }

    /// Effective `[sessions] enabled`: the override when set, else the
    /// `config.toml` value handed in (which already defaulted to `true`).
    pub fn sessions_enabled(&self, from_config: bool) -> bool {
        self.sessions_enabled.unwrap_or(from_config)
    }

    /// Effective `[notifications]`: `delivery` / `sound` replaced by the
    /// overrides when set; the sound paths always come from the file.
    pub fn notifications(&self, mut from_config: NotificationsConfig) -> NotificationsConfig {
        let s = self.sanitized();
        if let Some(d) = s.notification_delivery {
            from_config.delivery = d;
        }
        if let Some(sound) = s.sound {
            from_config.sound = sound;
        }
        from_config
    }
}

/// What actually applies after the three layers are merged, plus the two
/// lower layers so a UI can show where each value came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EffectiveSettings {
    pub sessions_enabled: bool,
    pub notifications: NotificationsConfig,
    /// Layer 2 — what `config.toml` says (defaults filled in).
    pub config_sessions_enabled: bool,
    pub config_notifications: NotificationsConfig,
    /// Layer 1 — the database overrides as stored.
    pub overrides: AppSettings,
}

/// Merge `config.toml` (read fresh from `config_path`) with the database
/// overrides. This is the ONE resolution both frontends must agree on.
pub fn resolve(config_path: &Path, prefs: Option<&dyn UiPrefsStorage>) -> EffectiveSettings {
    let overrides = AppSettings::load(prefs);
    let config_sessions_enabled = crate::session::sessions_enabled(config_path);
    let config_notifications = NotificationsConfig::from_config_file(config_path);
    resolve_with(overrides, config_sessions_enabled, config_notifications)
}

/// Pure merge used by [`resolve`]; the TUI feeds it its parsed `Config`.
pub fn resolve_with(
    overrides: AppSettings,
    config_sessions_enabled: bool,
    config_notifications: NotificationsConfig,
) -> EffectiveSettings {
    EffectiveSettings {
        sessions_enabled: overrides.sessions_enabled(config_sessions_enabled),
        notifications: overrides.notifications(config_notifications.clone()),
        config_sessions_enabled,
        config_notifications,
        overrides,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notifications::NotificationDelivery;
    use std::collections::{HashMap, HashSet};

    /// In-memory `UiPrefsStorage` so the round-trip test needs no SQLite.
    #[derive(Default)]
    struct MemPrefs(parking_lot::Mutex<HashMap<String, String>>);

    impl UiPrefsStorage for MemPrefs {
        fn get_collapsed_groups(&self) -> anyhow::Result<HashSet<String>> {
            Ok(HashSet::new())
        }
        fn set_collapsed_groups(&self, _: &HashSet<String>) -> anyhow::Result<()> {
            Ok(())
        }
        fn get_preference(&self, key: &str) -> anyhow::Result<Option<String>> {
            Ok(self.0.lock().get(key).cloned())
        }
        fn set_preference(&self, key: &str, value: &str) -> anyhow::Result<()> {
            self.0.lock().insert(key.to_string(), value.to_string());
            Ok(())
        }
    }

    fn config(delivery: &str, sound: bool) -> NotificationsConfig {
        NotificationsConfig {
            delivery: delivery.to_string(),
            sound,
            sound_path: Some("~/all.wav".into()),
            sound_done_path: None,
            sound_attention_path: Some("~/hey.wav".into()),
        }
    }

    #[test]
    fn no_override_means_config_wins_over_default() {
        let none = AppSettings::default();
        assert!(none.is_empty());
        // config.toml says off → off (default would be true).
        assert!(!none.sessions_enabled(false));
        assert!(none.sessions_enabled(true));
        let n = none.notifications(config("off", true));
        assert_eq!(n.parsed_delivery(), NotificationDelivery::Off);
        assert!(n.sound);
    }

    #[test]
    fn db_override_wins_over_config() {
        let o = AppSettings {
            sessions_enabled: Some(false),
            notification_delivery: Some("terminal".into()),
            sound: Some(false),
        };
        // config.toml says enabled + system + sound: the DB flips all three.
        assert!(!o.sessions_enabled(true));
        let n = o.notifications(config("system", true));
        assert_eq!(n.parsed_delivery(), NotificationDelivery::Terminal);
        assert!(!n.sound);
        // Paths are config.toml-only and survive untouched.
        assert_eq!(n.sound_path.as_deref(), Some("~/all.wav"));
        assert_eq!(n.sound_attention_path.as_deref(), Some("~/hey.wav"));
    }

    #[test]
    fn partial_override_only_touches_its_field() {
        let o = AppSettings {
            sound: Some(true),
            ..Default::default()
        };
        assert!(o.sessions_enabled(true));
        assert!(!o.sessions_enabled(false));
        let n = o.notifications(config("off", false));
        assert_eq!(n.parsed_delivery(), NotificationDelivery::Off);
        assert!(n.sound);
    }

    #[test]
    fn resolve_with_reports_all_three_layers() {
        let o = AppSettings {
            sessions_enabled: Some(false),
            ..Default::default()
        };
        let e = resolve_with(o.clone(), true, config("system", false));
        assert!(!e.sessions_enabled);
        assert!(e.config_sessions_enabled);
        assert_eq!(e.overrides, o);
        assert_eq!(e.notifications.delivery, "system");
    }

    #[test]
    fn resolve_reads_config_file_and_db() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[sessions]\nenabled = false\n[notifications]\ndelivery = \"off\"\n",
        )
        .unwrap();
        let prefs = MemPrefs::default();
        // Nothing in the DB: config.toml applies.
        let e = resolve(&path, Some(&prefs));
        assert!(!e.sessions_enabled);
        assert_eq!(e.notifications.delivery, "off");
        // Override in the DB: it beats the file.
        AppSettings {
            sessions_enabled: Some(true),
            notification_delivery: Some("system".into()),
            sound: None,
        }
        .save(&prefs)
        .unwrap();
        let e = resolve(&path, Some(&prefs));
        assert!(e.sessions_enabled);
        assert!(!e.config_sessions_enabled);
        assert_eq!(e.notifications.delivery, "system");
        assert_eq!(e.config_notifications.delivery, "off");
        // No storage at all (TUI tests, JSON backend): file only.
        let e = resolve(&path, None);
        assert!(!e.sessions_enabled);
        // Missing file + empty DB: built-in defaults.
        let e = resolve(&dir.path().join("nope.toml"), Some(&MemPrefs::default()));
        assert!(e.sessions_enabled);
        assert_eq!(
            e.notifications.parsed_delivery(),
            NotificationDelivery::System
        );
        assert!(!e.notifications.sound);
    }

    #[test]
    fn save_load_round_trip_and_tolerant_parse() {
        let prefs = MemPrefs::default();
        assert!(AppSettings::load(Some(&prefs)).is_empty());
        let o = AppSettings {
            sessions_enabled: Some(false),
            notification_delivery: Some("off".into()),
            sound: Some(true),
        };
        o.save(&prefs).unwrap();
        assert_eq!(AppSettings::load(Some(&prefs)), o);
        // Only the set fields are serialized (so `null`s never mask a future default).
        let raw = prefs.get_preference(PREF_KEY).unwrap().unwrap();
        assert!(!raw.contains("null"), "{raw}");
        // Garbage / old shapes → no overrides.
        assert!(AppSettings::from_json("not json").is_empty());
        assert!(AppSettings::from_json("{}").is_empty());
        assert!(AppSettings::from_json("{\"unknown\": 1}").is_empty());
        assert_eq!(
            AppSettings::from_json("{\"sound\": true}").sound,
            Some(true)
        );
    }

    #[test]
    fn unknown_delivery_is_never_stored_or_applied() {
        let bad = AppSettings {
            notification_delivery: Some("loud".into()),
            ..Default::default()
        };
        assert_eq!(bad.sanitized().notification_delivery, None);
        assert_eq!(
            bad.notifications(config("terminal", false)).delivery,
            "terminal"
        );
        let prefs = MemPrefs::default();
        bad.save(&prefs).unwrap();
        assert!(AppSettings::load(Some(&prefs)).is_empty());
    }
}
