//! Keys, modifiers and keystrokes.

use std::fmt;

/// Modifier key state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub shift: bool,
    pub control: bool,
    pub alt: bool,
    /// The Windows key / Command key / Super key.
    pub platform: bool,
}

impl Modifiers {
    pub const NONE: Modifiers = Modifiers {
        shift: false,
        control: false,
        alt: false,
        platform: false,
    };

    /// The platform's primary shortcut modifier: Command on macOS, Control elsewhere.
    pub fn secondary(&self) -> bool {
        if cfg!(target_os = "macos") {
            self.platform
        } else {
            self.control
        }
    }

    pub fn any(&self) -> bool {
        self.shift || self.control || self.alt || self.platform
    }

    /// True if any modifier other than shift is held (i.e. this is a shortcut, not text).
    pub fn is_command_like(&self) -> bool {
        self.control || self.alt || self.platform
    }

    /// Whether a key pressed with these modifiers types its character
    /// instead of acting as a shortcut. Windows reports AltGr (used for
    /// characters like `ą` or `€`) as Ctrl+Alt; on macOS Option composes
    /// characters.
    pub fn types_text(&self) -> bool {
        if self.platform {
            false
        } else if cfg!(target_os = "macos") {
            !self.control
        } else {
            self.control == self.alt
        }
    }
}

/// Keys that do not produce text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum NamedKey {
    Enter,
    Tab,
    Space,
    Backspace,
    Delete,
    Escape,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowDown,
    Home,
    End,
    PageUp,
    PageDown,
    Insert,
    F(u8),
    Shift,
    Control,
    Alt,
    Super,
    CapsLock,
    ContextMenu,
}

impl NamedKey {
    fn name(&self) -> String {
        match self {
            NamedKey::Enter => "enter".into(),
            NamedKey::Tab => "tab".into(),
            NamedKey::Space => "space".into(),
            NamedKey::Backspace => "backspace".into(),
            NamedKey::Delete => "delete".into(),
            NamedKey::Escape => "escape".into(),
            NamedKey::ArrowLeft => "left".into(),
            NamedKey::ArrowRight => "right".into(),
            NamedKey::ArrowUp => "up".into(),
            NamedKey::ArrowDown => "down".into(),
            NamedKey::Home => "home".into(),
            NamedKey::End => "end".into(),
            NamedKey::PageUp => "pageup".into(),
            NamedKey::PageDown => "pagedown".into(),
            NamedKey::Insert => "insert".into(),
            NamedKey::F(n) => format!("f{n}"),
            NamedKey::Shift => "shift".into(),
            NamedKey::Control => "control".into(),
            NamedKey::Alt => "alt".into(),
            NamedKey::Super => "super".into(),
            NamedKey::CapsLock => "capslock".into(),
            NamedKey::ContextMenu => "menu".into(),
        }
    }

    fn parse(s: &str) -> Option<NamedKey> {
        Some(match s {
            "enter" | "return" => NamedKey::Enter,
            "tab" => NamedKey::Tab,
            "space" => NamedKey::Space,
            "backspace" => NamedKey::Backspace,
            "delete" | "del" => NamedKey::Delete,
            "escape" | "esc" => NamedKey::Escape,
            "left" => NamedKey::ArrowLeft,
            "right" => NamedKey::ArrowRight,
            "up" => NamedKey::ArrowUp,
            "down" => NamedKey::ArrowDown,
            "home" => NamedKey::Home,
            "end" => NamedKey::End,
            "pageup" => NamedKey::PageUp,
            "pagedown" => NamedKey::PageDown,
            "insert" => NamedKey::Insert,
            "menu" => NamedKey::ContextMenu,
            _ => {
                let n = s.strip_prefix('f')?.parse::<u8>().ok()?;
                if (1..=24).contains(&n) {
                    NamedKey::F(n)
                } else {
                    return None;
                }
            }
        })
    }

    pub fn is_modifier(&self) -> bool {
        matches!(
            self,
            NamedKey::Shift
                | NamedKey::Control
                | NamedKey::Alt
                | NamedKey::Super
                | NamedKey::CapsLock
        )
    }
}

/// A logical key.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// A key producing a character. Stored lowercase for letters so that
    /// `ctrl-s` and `ctrl-shift-s` both match on `"s"`.
    Character(String),
    Named(NamedKey),
    Unidentified,
}

impl Key {
    pub fn character(s: &str) -> Key {
        Key::Character(s.to_lowercase())
    }
}

/// A single key press with modifiers, e.g. `ctrl-shift-p`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Keystroke {
    pub modifiers: Modifiers,
    pub key: Key,
    /// The text this key press produces with modifiers applied (e.g. `"?"` for
    /// `shift-/`), if any.
    pub key_char: Option<String>,
}

/// Error returned when a keystroke string cannot be parsed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeystrokeParseError(pub String);

impl fmt::Display for KeystrokeParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "invalid keystroke: {}", self.0)
    }
}

impl std::error::Error for KeystrokeParseError {}

impl Keystroke {
    pub fn new(modifiers: Modifiers, key: Key) -> Self {
        Keystroke {
            modifiers,
            key,
            key_char: None,
        }
    }

    /// Parses strings like `"ctrl-s"`, `"cmd-shift-p"`, `"secondary-enter"`,
    /// `"alt-f4"`. `secondary` maps to `cmd` on macOS and `ctrl` elsewhere.
    pub fn parse(source: &str) -> Result<Keystroke, KeystrokeParseError> {
        let mut modifiers = Modifiers::default();
        let mut key = None;
        let parts: Vec<&str> = split_keystroke(source);
        let last = parts.len().saturating_sub(1);
        for (i, part) in parts.iter().enumerate() {
            let lower = part.to_lowercase();
            if i < last {
                match lower.as_str() {
                    "ctrl" | "control" => modifiers.control = true,
                    "shift" => modifiers.shift = true,
                    "alt" | "option" => modifiers.alt = true,
                    "cmd" | "super" | "win" | "meta" | "platform" => modifiers.platform = true,
                    "secondary" => {
                        if cfg!(target_os = "macos") {
                            modifiers.platform = true
                        } else {
                            modifiers.control = true
                        }
                    }
                    _ => return Err(KeystrokeParseError(source.to_string())),
                }
            } else if let Some(named) = NamedKey::parse(&lower) {
                key = Some(Key::Named(named));
            } else if part.chars().count() == 1 {
                key = Some(Key::character(part));
            } else {
                return Err(KeystrokeParseError(source.to_string()));
            }
        }
        let key = key.ok_or_else(|| KeystrokeParseError(source.to_string()))?;
        Ok(Keystroke {
            modifiers,
            key,
            key_char: None,
        })
    }

    /// Whether this (pressed) keystroke satisfies `binding`.
    pub fn matches(&self, binding: &Keystroke) -> bool {
        if self.key == binding.key && self.modifiers == binding.modifiers {
            return true;
        }
        // `?` bound directly should match `shift-/` producing `?`.
        if !binding.modifiers.shift
            && let (Key::Character(want), Some(produced)) = (&binding.key, &self.key_char)
        {
            let mut mods = self.modifiers;
            mods.shift = false;
            return produced == want && mods == binding.modifiers;
        }
        false
    }
}

/// Splits on `-` while allowing `-` itself as the final key (`"ctrl--"`).
fn split_keystroke(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'-' && i > start {
            parts.push(&s[start..i]);
            start = i + 1;
        }
        i += 1;
    }
    if start < s.len() {
        parts.push(&s[start..]);
    }
    parts
}

impl fmt::Display for Keystroke {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.modifiers.control {
            f.write_str("ctrl-")?;
        }
        if self.modifiers.alt {
            f.write_str("alt-")?;
        }
        if self.modifiers.shift {
            f.write_str("shift-")?;
        }
        if self.modifiers.platform {
            f.write_str(if cfg!(target_os = "macos") {
                "cmd-"
            } else {
                "win-"
            })?;
        }
        match &self.key {
            Key::Character(c) => f.write_str(c),
            Key::Named(n) => f.write_str(&n.name()),
            Key::Unidentified => f.write_str("?"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_display() {
        let k = Keystroke::parse("ctrl-shift-p").unwrap();
        assert!(k.modifiers.control && k.modifiers.shift);
        assert_eq!(k.key, Key::Character("p".into()));
        assert_eq!(k.to_string(), "ctrl-shift-p");
        assert_eq!(
            Keystroke::parse("alt-f4").unwrap().key,
            Key::Named(NamedKey::F(4))
        );
        assert_eq!(
            Keystroke::parse("ctrl--").unwrap().key,
            Key::Character("-".into())
        );
        assert!(Keystroke::parse("hyper-x").is_err());
        assert!(Keystroke::parse("").is_err());
    }

    #[test]
    fn altgr_types_text() {
        let altgr = Modifiers {
            control: true,
            alt: true,
            ..Default::default()
        };
        let ctrl = Modifiers {
            control: true,
            ..Default::default()
        };
        assert!(Modifiers::NONE.types_text());
        assert!(!ctrl.types_text());
        if !cfg!(target_os = "macos") {
            assert!(altgr.types_text());
        }
    }

    #[test]
    fn shifted_character_matching() {
        let binding = Keystroke::parse("?").unwrap();
        let pressed = Keystroke {
            modifiers: Modifiers {
                shift: true,
                ..Default::default()
            },
            key: Key::Character("/".into()),
            key_char: Some("?".into()),
        };
        assert!(pressed.matches(&binding));
        let ctrl_s = Keystroke::parse("ctrl-s").unwrap();
        let pressed = Keystroke {
            modifiers: Modifiers {
                control: true,
                ..Default::default()
            },
            key: Key::character("S"),
            key_char: None,
        };
        assert!(pressed.matches(&ctrl_s));
    }
}
