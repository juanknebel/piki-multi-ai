//! Notification sounds for agent state changes (herdr-inspired).
//!
//! Two short chimes are synthesized as WAV at runtime (no audio assets, no
//! Rust audio dependencies) and played through whatever CLI player the
//! system has — pw-play/paplay/aplay on Linux, afplay on macOS. Users can
//! point to their own sound files instead; those are handed to the same
//! players, so any format the player decodes works.
//!
//! The sound layer is independent from the toast delivery mode: frontends
//! call [`set_settings`] once at startup, and `notifications::push_and_toast`
//! triggers [`play`] under the same "user isn't already looking at it"
//! suppression as toasts.

use std::path::PathBuf;
use std::sync::OnceLock;

use parking_lot::Mutex;

const DISABLE_SOUND_ENV: &str = "PIKI_DISABLE_SOUND";
const SAMPLE_RATE: u32 = 44_100;

/// Which notification sound to play.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sound {
    /// Agent finished its task (idle / stop events). Ascending chime.
    Done,
    /// Agent needs the user (permission / waiting events). Descending chime.
    Attention,
}

/// Global sound configuration, set once by the frontend from its config.
#[derive(Debug, Clone, Default)]
pub struct SoundSettings {
    pub enabled: bool,
    /// Custom sound file used for all events (any format the system player
    /// decodes). Falls back to the built-in chime when unset or unplayable.
    pub path: Option<PathBuf>,
    /// Per-event overrides; fall back to `path`, then to the built-in chime.
    pub done_path: Option<PathBuf>,
    pub attention_path: Option<PathBuf>,
}

impl SoundSettings {
    fn path_for(&self, sound: Sound) -> Option<PathBuf> {
        match sound {
            Sound::Done => self.done_path.as_ref().or(self.path.as_ref()),
            Sound::Attention => self.attention_path.as_ref().or(self.path.as_ref()),
        }
        .cloned()
    }
}

static SETTINGS: OnceLock<Mutex<SoundSettings>> = OnceLock::new();

fn settings() -> &'static Mutex<SoundSettings> {
    SETTINGS.get_or_init(|| Mutex::new(SoundSettings::default()))
}

/// Replace the global sound settings. Call once at startup (later calls
/// simply replace, so a future config reload works too).
pub fn set_settings(s: SoundSettings) {
    *settings().lock() = s;
}

/// Play a notification sound in a background thread. No-op when sound is
/// disabled, `PIKI_DISABLE_SOUND` is set, or no player is available (the
/// failure is logged, never propagated).
pub fn play(sound: Sound) {
    if std::env::var_os(DISABLE_SOUND_ENV).is_some() {
        return;
    }
    let snapshot = settings().lock().clone();
    if !snapshot.enabled {
        return;
    }
    let custom = snapshot.path_for(sound);
    std::thread::spawn(move || {
        if let Some(path) = custom {
            if play_file(&path) {
                return;
            }
            tracing::warn!(path = %path.display(), ?sound, "custom sound failed, falling back to built-in chime");
        }
        match builtin_wav_path(sound) {
            Ok(path) => {
                if !play_file(&path) {
                    tracing::warn!(?sound, "no working audio player found (tried pw-play/paplay/aplay/afplay)");
                }
            }
            Err(e) => tracing::warn!(error = %e, "failed to materialize built-in sound"),
        }
    });
}

/// Write the synthesized chime to a stable temp path (overwritten each time
/// — it's a few KB) and return it.
fn builtin_wav_path(sound: Sound) -> std::io::Result<PathBuf> {
    let name = match sound {
        Sound::Done => "piki-sound-done.wav",
        Sound::Attention => "piki-sound-attention.wav",
    };
    let path = std::env::temp_dir().join(name);
    std::fs::write(&path, builtin_wav(sound))?;
    Ok(path)
}

fn builtin_wav(sound: Sound) -> Vec<u8> {
    match sound {
        // Gentle ascending major third + fifth — "all done".
        Sound::Done => synth_wav(&[(659.25, 0.11), (880.0, 0.16)]),
        // Insistent descending pair — "I need you".
        Sound::Attention => synth_wav(&[(880.0, 0.10), (659.25, 0.10), (880.0, 0.14)]),
    }
}

/// Try each candidate CLI player until one plays the file successfully.
fn play_file(path: &std::path::Path) -> bool {
    let candidates: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("afplay", &[])]
    } else {
        &[("pw-play", &[]), ("paplay", &[]), ("aplay", &["-q"])]
    };
    for (player, args) in candidates {
        let status = std::process::Command::new(player)
            .args(*args)
            .arg(path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status();
        if let Ok(s) = status
            && s.success()
        {
            return true;
        }
    }
    false
}

/// Synthesize a sequence of sine notes `(freq_hz, duration_secs)` as a
/// 16-bit mono PCM WAV, with short fades to avoid clicks and a small gap
/// between notes.
fn synth_wav(notes: &[(f32, f32)]) -> Vec<u8> {
    const AMPLITUDE: f32 = 0.32;
    const FADE_SECS: f32 = 0.008;
    const GAP_SECS: f32 = 0.03;

    let mut samples: Vec<i16> = Vec::new();
    let gap_len = (GAP_SECS * SAMPLE_RATE as f32) as usize;
    for (i, &(freq, dur)) in notes.iter().enumerate() {
        if i > 0 {
            samples.extend(std::iter::repeat_n(0i16, gap_len));
        }
        let note_len = (dur * SAMPLE_RATE as f32) as usize;
        let fade_len = ((FADE_SECS * SAMPLE_RATE as f32) as usize).min(note_len / 2);
        for n in 0..note_len {
            let t = n as f32 / SAMPLE_RATE as f32;
            let mut v = (t * freq * std::f32::consts::TAU).sin() * AMPLITUDE;
            if n < fade_len {
                v *= n as f32 / fade_len as f32;
            }
            let from_end = note_len - n;
            if from_end < fade_len {
                v *= from_end as f32 / fade_len as f32;
            }
            samples.push((v * i16::MAX as f32) as i16);
        }
    }

    let data_len = (samples.len() * 2) as u32;
    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synth_wav_produces_valid_riff_header() {
        let wav = synth_wav(&[(440.0, 0.05)]);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[36..40], b"data");
        let data_len = u32::from_le_bytes(wav[40..44].try_into().unwrap());
        assert_eq!(wav.len(), 44 + data_len as usize);
        let riff_len = u32::from_le_bytes(wav[4..8].try_into().unwrap());
        assert_eq!(riff_len, 36 + data_len);
        // ~0.05s at 44.1kHz mono 16-bit ≈ 4410 bytes
        assert!(data_len >= 4000, "data too short: {data_len}");
    }

    #[test]
    fn synth_wav_has_nonzero_audio() {
        let wav = synth_wav(&[(440.0, 0.05)]);
        let peak = wav[44..]
            .chunks_exact(2)
            .map(|c| i16::from_le_bytes([c[0], c[1]]).unsigned_abs())
            .max()
            .unwrap();
        assert!(peak > 5000, "peak too quiet: {peak}");
    }

    #[test]
    fn builtin_sounds_differ() {
        assert_ne!(builtin_wav(Sound::Done), builtin_wav(Sound::Attention));
    }

    #[test]
    fn path_for_falls_back_to_shared_path() {
        let s = SoundSettings {
            enabled: true,
            path: Some(PathBuf::from("/tmp/all.wav")),
            done_path: Some(PathBuf::from("/tmp/done.wav")),
            attention_path: None,
        };
        assert_eq!(s.path_for(Sound::Done), Some(PathBuf::from("/tmp/done.wav")));
        assert_eq!(
            s.path_for(Sound::Attention),
            Some(PathBuf::from("/tmp/all.wav"))
        );
    }
}
