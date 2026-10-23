//! Validated hotkey combinations such as `Cmd+Shift+D`.

use crate::platform::Os;
use std::fmt;

/// A modifier key. `Meta` is Command on macOS, the Windows key on Windows, and Super
/// on Linux.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Modifier {
    Ctrl,
    Alt,
    Shift,
    Meta,
}

/// The non-modifier key of a hotkey.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Key {
    /// An ASCII letter (stored uppercase) or digit.
    Char(char),
    /// F1 to F24.
    Function(u8),
    Space,
    Tab,
    Enter,
    Escape,
    Backspace,
    Delete,
    Up,
    Down,
    Left,
    Right,
    Home,
    End,
    PageUp,
    PageDown,
}

/// One or more modifiers plus a key. Only [`parse`] and the defaults build one, so
/// every value is valid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Accelerator {
    modifiers: Vec<Modifier>,
    key: Key,
}

impl Accelerator {
    /// Builds an accelerator from modifiers known to be valid, in canonical order.
    pub(crate) fn new(modifiers: impl IntoIterator<Item = Modifier>, key: Key) -> Self {
        let mut modifiers: Vec<Modifier> = modifiers.into_iter().collect();
        modifiers.sort_unstable();
        modifiers.dedup();
        Self { modifiers, key }
    }

    /// The modifiers, sorted and without duplicates.
    #[must_use]
    pub fn modifiers(&self) -> &[Modifier] {
        &self.modifiers
    }

    #[must_use]
    pub fn key(&self) -> Key {
        self.key
    }

    /// How menus show the hotkey on `os`: `⇧⌘D` on macOS (Apple's modifier order,
    /// which is also the canonical order), `Ctrl+Shift+D` elsewhere.
    #[must_use]
    pub fn display(&self, os: Os) -> String {
        let modifiers = self
            .modifiers
            .iter()
            .map(|modifier| modifier_name(*modifier, os));
        let key = key_name(self.key, os);
        match os {
            Os::MacOs => modifiers.chain([key]).collect(),
            Os::Windows | Os::Linux => modifiers.chain([key]).collect::<Vec<_>>().join("+"),
        }
    }
}

fn modifier_name(modifier: Modifier, os: Os) -> String {
    let name = match (modifier, os) {
        (Modifier::Ctrl, Os::MacOs) => "⌃",
        (Modifier::Alt, Os::MacOs) => "⌥",
        (Modifier::Shift, Os::MacOs) => "⇧",
        (Modifier::Meta, Os::MacOs) => "⌘",
        (Modifier::Ctrl, _) => "Ctrl",
        (Modifier::Alt, _) => "Alt",
        (Modifier::Shift, _) => "Shift",
        (Modifier::Meta, Os::Windows) => "Win",
        (Modifier::Meta, Os::Linux) => "Super",
    };
    name.to_owned()
}

fn key_name(key: Key, os: Os) -> String {
    let mac = os == Os::MacOs;
    let name = match key {
        Key::Char(c) => return c.to_string(),
        Key::Function(n) => return format!("F{n}"),
        Key::Space => "Space",
        Key::Tab if mac => "⇥",
        Key::Tab => "Tab",
        Key::Enter if mac => "↩",
        Key::Enter => "Enter",
        Key::Escape if mac => "⎋",
        Key::Escape => "Esc",
        Key::Backspace if mac => "⌫",
        Key::Backspace => "Backspace",
        Key::Delete if mac => "⌦",
        Key::Delete => "Delete",
        Key::Up => "↑",
        Key::Down => "↓",
        Key::Left => "←",
        Key::Right => "→",
        Key::Home => "Home",
        Key::End => "End",
        Key::PageUp => "PageUp",
        Key::PageDown => "PageDown",
    };
    name.to_owned()
}

/// Why a hotkey string was rejected.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcceleratorError {
    Empty,
    UnknownPart(String),
    DuplicateModifier(String),
    MissingKey,
    MultipleKeys,
    /// Shift alone isn't enough: it would steal ordinary typing.
    MissingModifier,
}

impl fmt::Display for AcceleratorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => write!(f, "hotkey is empty"),
            Self::UnknownPart(part) => write!(f, "unknown key or modifier \"{part}\""),
            Self::DuplicateModifier(part) => write!(f, "modifier \"{part}\" is repeated"),
            Self::MissingKey => write!(f, "hotkey has no key, only modifiers"),
            Self::MultipleKeys => write!(f, "hotkey has more than one key"),
            Self::MissingModifier => write!(f, "hotkey needs Ctrl, Alt, or Cmd/Super"),
        }
    }
}

impl std::error::Error for AcceleratorError {}

/// One `+`-separated part of a hotkey string.
enum Part {
    Modifier(Modifier),
    Key(Key),
}

/// Parses a hotkey such as `"Cmd+Shift+D"`, case-insensitively and in any order.
///
/// `Cmd` means Command on macOS and Ctrl elsewhere, so one config works everywhere.
pub fn parse(text: &str, os: Os) -> Result<Accelerator, AcceleratorError> {
    if text.trim().is_empty() {
        return Err(AcceleratorError::Empty);
    }
    let (modifiers, key) = text.split('+').map(str::trim).try_fold(
        (Vec::new(), None),
        |(modifiers, key): (Vec<Modifier>, Option<Key>), name| match classify(name, os) {
            Some(Part::Modifier(modifier)) if modifiers.contains(&modifier) => {
                Err(AcceleratorError::DuplicateModifier(name.into()))
            }
            Some(Part::Modifier(modifier)) => {
                Ok((modifiers.into_iter().chain([modifier]).collect(), key))
            }
            Some(Part::Key(_)) if key.is_some() => Err(AcceleratorError::MultipleKeys),
            Some(Part::Key(found)) => Ok((modifiers, Some(found))),
            None => Err(AcceleratorError::UnknownPart(name.into())),
        },
    )?;
    let key = key.ok_or(AcceleratorError::MissingKey)?;
    if modifiers
        .iter()
        .all(|modifier| *modifier == Modifier::Shift)
    {
        return Err(AcceleratorError::MissingModifier);
    }
    Ok(Accelerator::new(modifiers, key))
}

fn classify(name: &str, os: Os) -> Option<Part> {
    modifier(name, os)
        .map(Part::Modifier)
        .or_else(|| key(name).map(Part::Key))
}

fn modifier(name: &str, os: Os) -> Option<Modifier> {
    match name.to_ascii_lowercase().as_str() {
        "cmd" | "command" if os == Os::MacOs => Some(Modifier::Meta),
        "cmd" | "command" | "ctrl" | "control" => Some(Modifier::Ctrl),
        "alt" | "option" | "opt" => Some(Modifier::Alt),
        "shift" => Some(Modifier::Shift),
        "super" | "win" | "meta" => Some(Modifier::Meta),
        _ => None,
    }
}

fn key(name: &str) -> Option<Key> {
    let lower = name.to_ascii_lowercase();
    let mut chars = lower.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if c.is_ascii_alphanumeric() => Some(Key::Char(c.to_ascii_uppercase())),
        _ => named_key(&lower),
    }
}

fn named_key(lower: &str) -> Option<Key> {
    let function = lower
        .strip_prefix('f')
        .and_then(|n| n.parse::<u8>().ok())
        .filter(|n| (1..=24).contains(n))
        .map(Key::Function);
    function.or(match lower {
        "space" => Some(Key::Space),
        "tab" => Some(Key::Tab),
        "enter" | "return" => Some(Key::Enter),
        "escape" | "esc" => Some(Key::Escape),
        "backspace" => Some(Key::Backspace),
        "delete" | "del" => Some(Key::Delete),
        "up" => Some(Key::Up),
        "down" => Some(Key::Down),
        "left" => Some(Key::Left),
        "right" => Some(Key::Right),
        "home" => Some(Key::Home),
        "end" => Some(Key::End),
        "pageup" => Some(Key::PageUp),
        "pagedown" => Some(Key::PageDown),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use Modifier::{Alt, Ctrl, Meta, Shift};

    fn accel(modifiers: &[Modifier], key: Key) -> Accelerator {
        Accelerator::new(modifiers.iter().copied(), key)
    }

    #[test]
    fn displays_hotkeys_per_os() {
        let cases = [
            ("Cmd+Shift+D", Os::MacOs, "⇧⌘D"),
            ("Ctrl+Alt+Shift+Cmd+R", Os::MacOs, "⌃⌥⇧⌘R"),
            ("Cmd+Shift+D", Os::Windows, "Ctrl+Shift+D"),
            ("Win+Alt+F5", Os::Windows, "Alt+Win+F5"),
            ("Super+Space", Os::Linux, "Super+Space"),
        ];

        cases.iter().for_each(|(text, os, expected)| {
            assert_eq!(parse(text, *os).unwrap().display(*os), *expected, "{text}");
        });
    }

    #[test]
    fn displays_every_named_key() {
        let names = [
            "space",
            "tab",
            "enter",
            "esc",
            "backspace",
            "delete",
            "up",
            "down",
            "left",
            "right",
            "home",
            "end",
            "pageup",
            "pagedown",
        ];
        let shown = |os| -> Vec<String> {
            names
                .iter()
                .map(|name| parse(&format!("Ctrl+{name}"), os).unwrap().display(os))
                .collect()
        };

        assert_eq!(
            shown(Os::MacOs),
            [
                "⌃Space",
                "⌃⇥",
                "⌃↩",
                "⌃⎋",
                "⌃⌫",
                "⌃⌦",
                "⌃↑",
                "⌃↓",
                "⌃←",
                "⌃→",
                "⌃Home",
                "⌃End",
                "⌃PageUp",
                "⌃PageDown"
            ]
        );
        assert_eq!(
            shown(Os::Linux),
            [
                "Ctrl+Space",
                "Ctrl+Tab",
                "Ctrl+Enter",
                "Ctrl+Esc",
                "Ctrl+Backspace",
                "Ctrl+Delete",
                "Ctrl+↑",
                "Ctrl+↓",
                "Ctrl+←",
                "Ctrl+→",
                "Ctrl+Home",
                "Ctrl+End",
                "Ctrl+PageUp",
                "Ctrl+PageDown"
            ]
        );
    }

    #[test]
    fn parses_valid_hotkeys() {
        let cases = [
            (
                "Cmd+Shift+D",
                Os::MacOs,
                accel(&[Shift, Meta], Key::Char('D')),
            ),
            (
                "Cmd+Shift+D",
                Os::Windows,
                accel(&[Ctrl, Shift], Key::Char('D')),
            ),
            (
                "cmd+shift+d",
                Os::Linux,
                accel(&[Ctrl, Shift], Key::Char('D')),
            ),
            ("Option+Space", Os::MacOs, accel(&[Alt], Key::Space)),
            (
                " Ctrl + Alt + 5 ",
                Os::Linux,
                accel(&[Ctrl, Alt], Key::Char('5')),
            ),
            ("D+Super", Os::Linux, accel(&[Meta], Key::Char('D'))),
            ("Win+F12", Os::Windows, accel(&[Meta], Key::Function(12))),
            (
                "Control+Shift+F1",
                Os::MacOs,
                accel(&[Ctrl, Shift], Key::Function(1)),
            ),
            ("Opt+Return", Os::MacOs, accel(&[Alt], Key::Enter)),
            ("Command+Esc", Os::MacOs, accel(&[Meta], Key::Escape)),
            ("Meta+Del", Os::Linux, accel(&[Meta], Key::Delete)),
        ];

        cases.into_iter().for_each(|(text, os, expected)| {
            assert_eq!(parse(text, os), Ok(expected), "{text} on {os:?}");
        });
    }

    #[test]
    fn parses_every_named_key() {
        let keys = [
            ("Tab", Key::Tab),
            ("Escape", Key::Escape),
            ("Backspace", Key::Backspace),
            ("Up", Key::Up),
            ("Down", Key::Down),
            ("Left", Key::Left),
            ("Right", Key::Right),
            ("Home", Key::Home),
            ("End", Key::End),
            ("PageUp", Key::PageUp),
            ("PageDown", Key::PageDown),
            ("F24", Key::Function(24)),
        ];

        keys.into_iter().for_each(|(name, expected)| {
            let parsed = parse(&format!("Alt+{name}"), Os::Linux);
            assert_eq!(parsed.map(|a| a.key()), Ok(expected), "{name}");
        });
    }

    #[test]
    fn rejects_invalid_hotkeys() {
        let cases = [
            ("", AcceleratorError::Empty),
            ("   ", AcceleratorError::Empty),
            ("Cmd+Hyper+D", AcceleratorError::UnknownPart("Hyper".into())),
            ("Cmd++D", AcceleratorError::UnknownPart(String::new())),
            ("Cmd+F25", AcceleratorError::UnknownPart("F25".into())),
            ("Cmd+F0", AcceleratorError::UnknownPart("F0".into())),
            ("Cmd+é", AcceleratorError::UnknownPart("é".into())),
            (
                "Shift+shift+D",
                AcceleratorError::DuplicateModifier("shift".into()),
            ),
            ("Cmd+Shift", AcceleratorError::MissingKey),
            ("Cmd+D+E", AcceleratorError::MultipleKeys),
            ("D", AcceleratorError::MissingModifier),
            ("Shift+D", AcceleratorError::MissingModifier),
        ];

        cases.into_iter().for_each(|(text, expected)| {
            assert_eq!(parse(text, Os::MacOs), Err(expected), "{text:?}");
        });
    }

    #[test]
    fn cmd_and_ctrl_collide_outside_macos() {
        assert_eq!(
            parse("Cmd+Ctrl+D", Os::MacOs).map(|a| a.modifiers().len()),
            Ok(2)
        );
        assert_eq!(
            parse("Cmd+Ctrl+D", Os::Linux),
            Err(AcceleratorError::DuplicateModifier("Ctrl".into()))
        );
    }

    #[test]
    fn errors_explain_the_problem() {
        let cases = [
            (AcceleratorError::Empty, "hotkey is empty"),
            (
                AcceleratorError::UnknownPart("X1".into()),
                "unknown key or modifier \"X1\"",
            ),
            (
                AcceleratorError::DuplicateModifier("Alt".into()),
                "modifier \"Alt\" is repeated",
            ),
            (
                AcceleratorError::MissingKey,
                "hotkey has no key, only modifiers",
            ),
            (
                AcceleratorError::MultipleKeys,
                "hotkey has more than one key",
            ),
            (
                AcceleratorError::MissingModifier,
                "hotkey needs Ctrl, Alt, or Cmd/Super",
            ),
        ];

        cases
            .into_iter()
            .for_each(|(error, expected)| assert_eq!(error.to_string(), expected));
        let boxed: Box<dyn std::error::Error> = Box::new(AcceleratorError::Empty);
        assert_eq!(boxed.to_string(), "hotkey is empty");
    }
}
