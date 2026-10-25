//! Maps fluentb's [`Accelerator`] onto the `global-hotkey` crate's types.

use crate::accelerator::{Accelerator, Key, Modifier};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};

const LETTERS: [Code; 26] = [
    Code::KeyA,
    Code::KeyB,
    Code::KeyC,
    Code::KeyD,
    Code::KeyE,
    Code::KeyF,
    Code::KeyG,
    Code::KeyH,
    Code::KeyI,
    Code::KeyJ,
    Code::KeyK,
    Code::KeyL,
    Code::KeyM,
    Code::KeyN,
    Code::KeyO,
    Code::KeyP,
    Code::KeyQ,
    Code::KeyR,
    Code::KeyS,
    Code::KeyT,
    Code::KeyU,
    Code::KeyV,
    Code::KeyW,
    Code::KeyX,
    Code::KeyY,
    Code::KeyZ,
];

const DIGITS: [Code; 10] = [
    Code::Digit0,
    Code::Digit1,
    Code::Digit2,
    Code::Digit3,
    Code::Digit4,
    Code::Digit5,
    Code::Digit6,
    Code::Digit7,
    Code::Digit8,
    Code::Digit9,
];

const FUNCTION_KEYS: [Code; 24] = [
    Code::F1,
    Code::F2,
    Code::F3,
    Code::F4,
    Code::F5,
    Code::F6,
    Code::F7,
    Code::F8,
    Code::F9,
    Code::F10,
    Code::F11,
    Code::F12,
    Code::F13,
    Code::F14,
    Code::F15,
    Code::F16,
    Code::F17,
    Code::F18,
    Code::F19,
    Code::F20,
    Code::F21,
    Code::F22,
    Code::F23,
    Code::F24,
];

/// The `global-hotkey` hotkey for `accelerator`, or `None` for a key it can't express.
#[must_use]
pub fn to_hotkey(accelerator: &Accelerator) -> Option<HotKey> {
    let modifiers = accelerator
        .modifiers()
        .iter()
        .fold(Modifiers::empty(), |all, modifier| {
            all | to_modifier(*modifier)
        });
    to_code(accelerator.key()).map(|code| HotKey::new(Some(modifiers), code))
}

fn to_modifier(modifier: Modifier) -> Modifiers {
    match modifier {
        Modifier::Ctrl => Modifiers::CONTROL,
        Modifier::Alt => Modifiers::ALT,
        Modifier::Shift => Modifiers::SHIFT,
        Modifier::Meta => Modifiers::META,
    }
}

fn to_code(key: Key) -> Option<Code> {
    match key {
        Key::Char(c) => u8::try_from(c).ok().and_then(|byte| match byte {
            b'A'..=b'Z' => LETTERS.get(usize::from(byte - b'A')).copied(),
            b'0'..=b'9' => DIGITS.get(usize::from(byte - b'0')).copied(),
            _ => None,
        }),
        Key::Function(n) => FUNCTION_KEYS.get(usize::from(n).wrapping_sub(1)).copied(),
        Key::Space => Some(Code::Space),
        Key::Tab => Some(Code::Tab),
        Key::Enter => Some(Code::Enter),
        Key::Escape => Some(Code::Escape),
        Key::Backspace => Some(Code::Backspace),
        Key::Delete => Some(Code::Delete),
        Key::Up => Some(Code::ArrowUp),
        Key::Down => Some(Code::ArrowDown),
        Key::Left => Some(Code::ArrowLeft),
        Key::Right => Some(Code::ArrowRight),
        Key::Home => Some(Code::Home),
        Key::End => Some(Code::End),
        Key::PageUp => Some(Code::PageUp),
        Key::PageDown => Some(Code::PageDown),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accelerator::parse;
    use crate::platform::Os;

    fn hotkey(text: &str) -> Option<HotKey> {
        to_hotkey(&parse(text, Os::MacOs).unwrap())
    }

    #[test]
    fn esc_maps_without_modifiers() {
        assert_eq!(
            to_hotkey(&Accelerator::esc()),
            Some(HotKey::new(Some(Modifiers::empty()), Code::Escape))
        );
    }

    #[test]
    fn maps_modifiers_and_keys() {
        let cases = [
            (
                "Cmd+Shift+D",
                Modifiers::META | Modifiers::SHIFT,
                Code::KeyD,
            ),
            (
                "Ctrl+Alt+Z",
                Modifiers::CONTROL | Modifiers::ALT,
                Code::KeyZ,
            ),
            ("Alt+0", Modifiers::ALT, Code::Digit0),
            ("Alt+9", Modifiers::ALT, Code::Digit9),
            ("Alt+F1", Modifiers::ALT, Code::F1),
            ("Alt+F24", Modifiers::ALT, Code::F24),
            ("Alt+Space", Modifiers::ALT, Code::Space),
        ];

        cases.into_iter().for_each(|(text, modifiers, code)| {
            assert_eq!(
                hotkey(text),
                Some(HotKey::new(Some(modifiers), code)),
                "{text}"
            );
        });
    }

    #[test]
    fn maps_every_named_key() {
        let cases = [
            ("Tab", Code::Tab),
            ("Enter", Code::Enter),
            ("Esc", Code::Escape),
            ("Backspace", Code::Backspace),
            ("Delete", Code::Delete),
            ("Up", Code::ArrowUp),
            ("Down", Code::ArrowDown),
            ("Left", Code::ArrowLeft),
            ("Right", Code::ArrowRight),
            ("Home", Code::Home),
            ("End", Code::End),
            ("PageUp", Code::PageUp),
            ("PageDown", Code::PageDown),
        ];

        cases.into_iter().for_each(|(name, code)| {
            assert_eq!(
                hotkey(&format!("Ctrl+{name}")),
                Some(HotKey::new(Some(Modifiers::CONTROL), code)),
                "{name}"
            );
        });
    }

    #[test]
    fn distinct_accelerators_get_distinct_ids() {
        let dictate = hotkey("Cmd+Shift+D").unwrap();
        let rewrite = hotkey("Cmd+Shift+R").unwrap();

        assert_ne!(dictate.id(), rewrite.id());
    }

    #[test]
    fn rejects_keys_that_cannot_exist() {
        assert_eq!(to_code(Key::Char('a')), None);
        assert_eq!(to_code(Key::Char('é')), None);
        assert_eq!(to_code(Key::Function(0)), None);
        assert_eq!(to_code(Key::Function(25)), None);
    }
}
