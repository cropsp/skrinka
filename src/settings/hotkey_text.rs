//! Hotkey text: what the Settings window records from a key press, in the format
//! `global_hotkey::hotkey::HotKey::from_str` parses (`Ctrl+Shift+Alt+Super+Key`), and how it is
//! shown (`Super` as `Win`).

use windows::Win32::UI::Input::KeyboardAndMouse::*;

/// Modifier keys held with the key.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub win: bool,
}

impl Mods {
    fn any(self) -> bool {
        self.ctrl || self.shift || self.alt || self.win
    }
}

/// Virtual key → global-hotkey key name: the inverse of global-hotkey 0.8's own `Code` → VK
/// table (`platform_impl/windows`), so a recorded key registers as the same key.
fn key_name(vk: VIRTUAL_KEY) -> Option<&'static str> {
    const LETTERS: [&str; 26] = [
        "A", "B", "C", "D", "E", "F", "G", "H", "I", "J", "K", "L", "M", "N", "O", "P", "Q", "R", "S", "T",
        "U", "V", "W", "X", "Y", "Z",
    ];
    const DIGITS: [&str; 10] = ["0", "1", "2", "3", "4", "5", "6", "7", "8", "9"];
    const NUMPAD: [&str; 10] = ["Num0", "Num1", "Num2", "Num3", "Num4", "Num5", "Num6", "Num7", "Num8", "Num9"];
    const FKEYS: [&str; 24] = [
        "F1", "F2", "F3", "F4", "F5", "F6", "F7", "F8", "F9", "F10", "F11", "F12", "F13", "F14", "F15", "F16",
        "F17", "F18", "F19", "F20", "F21", "F22", "F23", "F24",
    ];
    let code = vk.0;
    let name = match vk {
        _ if (VK_A.0..=VK_Z.0).contains(&code) => LETTERS[(code - VK_A.0) as usize],
        _ if (VK_0.0..=VK_9.0).contains(&code) => DIGITS[(code - VK_0.0) as usize],
        _ if (VK_NUMPAD0.0..=VK_NUMPAD9.0).contains(&code) => NUMPAD[(code - VK_NUMPAD0.0) as usize],
        _ if (VK_F1.0..=VK_F24.0).contains(&code) => FKEYS[(code - VK_F1.0) as usize],
        VK_SPACE => "Space",
        VK_RETURN => "Enter",
        VK_TAB => "Tab",
        VK_BACK => "Backspace",
        VK_ESCAPE => "Escape",
        VK_DELETE => "Delete",
        VK_INSERT => "Insert",
        VK_HOME => "Home",
        VK_END => "End",
        VK_PRIOR => "PageUp",
        VK_NEXT => "PageDown",
        VK_UP => "Up",
        VK_DOWN => "Down",
        VK_LEFT => "Left",
        VK_RIGHT => "Right",
        VK_SNAPSHOT => "PrintScreen",
        VK_SCROLL => "ScrollLock",
        VK_PAUSE => "Pause",
        VK_ADD => "NumAdd",
        VK_SUBTRACT => "NumSubtract",
        VK_MULTIPLY => "NumMultiply",
        VK_DIVIDE => "NumDivide",
        VK_DECIMAL => "NumDecimal",
        VK_OEM_MINUS => "Minus",
        VK_OEM_PLUS => "Equal",
        VK_OEM_4 => "BracketLeft",
        VK_OEM_6 => "BracketRight",
        VK_OEM_1 => "Semicolon",
        VK_OEM_7 => "Quote",
        VK_OEM_3 => "Backquote",
        VK_OEM_5 => "Backslash",
        VK_OEM_COMMA => "Comma",
        VK_OEM_PERIOD => "Period",
        VK_OEM_2 => "Slash",
        _ => return None,
    };
    Some(name)
}

/// A key that is only a modifier; the Settings window waits for the real key.
pub fn is_modifier(vk: VIRTUAL_KEY) -> bool {
    matches!(
        vk,
        VK_SHIFT | VK_LSHIFT | VK_RSHIFT | VK_CONTROL | VK_LCONTROL | VK_RCONTROL | VK_MENU | VK_LMENU
            | VK_RMENU | VK_LWIN | VK_RWIN
    )
}

/// The stored hotkey text, or `None` for a key Skrinka cannot register (no name, or no
/// modifier on a key other than F1–F24 and PrintScreen).
pub fn format(mods: Mods, vk: VIRTUAL_KEY) -> Option<String> {
    let key = key_name(vk)?;
    let alone_ok = (VK_F1.0..=VK_F24.0).contains(&vk.0) || vk == VK_SNAPSHOT;
    if !mods.any() && !alone_ok {
        return None;
    }
    let mut parts = Vec::with_capacity(5);
    for (held, name) in [(mods.ctrl, "Ctrl"), (mods.shift, "Shift"), (mods.alt, "Alt"), (mods.win, "Super")] {
        if held {
            parts.push(name);
        }
    }
    parts.push(key);
    Some(parts.join("+"))
}

/// The hotkey as shown to the user: `Super` (and its aliases) as `Win`.
pub fn display(hotkey: &str) -> String {
    hotkey
        .split('+')
        .map(|token| match token.trim().to_uppercase().as_str() {
            "SUPER" | "CMD" | "COMMAND" => "Win",
            _ => token,
        })
        .collect::<Vec<_>>()
        .join("+")
}

#[cfg(test)]
mod tests {
    use super::*;
    use global_hotkey::hotkey::HotKey;

    use crate::config::{DEFAULT_HOTKEY_FULLSCREEN, DEFAULT_HOTKEY_REGION, DEFAULT_HOTKEY_WINDOW};

    fn all_mods() -> Vec<Mods> {
        (0..16)
            .map(|bits| Mods { ctrl: bits & 1 != 0, shift: bits & 2 != 0, alt: bits & 4 != 0, win: bits & 8 != 0 })
            .collect()
    }

    #[test]
    fn every_string_the_table_produces_parses() {
        let mut produced = 0;
        for vk in 0..=255u16 {
            for mods in all_mods() {
                if let Some(text) = format(mods, VIRTUAL_KEY(vk)) {
                    assert!(text.parse::<HotKey>().is_ok(), "{text:?} (vk {vk:#x}) does not parse");
                    produced += 1;
                }
            }
        }
        assert!(produced > 90 * 15, "only {produced} strings");
    }

    #[test]
    fn defaults_round_trip() {
        let shift_alt = Mods { shift: true, alt: true, ..Mods::default() };
        assert_eq!(format(shift_alt, VK_2).as_deref(), Some(DEFAULT_HOTKEY_REGION));
        assert_eq!(format(shift_alt, VK_1).as_deref(), Some(DEFAULT_HOTKEY_FULLSCREEN));
        assert_eq!(format(shift_alt, VK_3).as_deref(), Some(DEFAULT_HOTKEY_WINDOW));
        for default in [DEFAULT_HOTKEY_REGION, DEFAULT_HOTKEY_FULLSCREEN, DEFAULT_HOTKEY_WINDOW] {
            assert_eq!(display(default), default);
        }
    }

    #[test]
    fn modifier_order_and_names() {
        let mods = Mods { ctrl: true, shift: true, ..Mods::default() };
        assert_eq!(format(mods, VK_F9).as_deref(), Some("Ctrl+Shift+F9"));
        let all = Mods { ctrl: true, shift: true, alt: true, win: true };
        assert_eq!(format(all, VK_OEM_PLUS).as_deref(), Some("Ctrl+Shift+Alt+Super+Equal"));
        assert_eq!(
            "Ctrl+Shift+Alt+Super+Equal".parse::<HotKey>().unwrap(),
            "super+alt+shift+ctrl+=".parse::<HotKey>().unwrap()
        );
    }

    #[test]
    fn a_key_needs_a_modifier_except_function_keys_and_print_screen() {
        let none = Mods::default();
        assert_eq!(format(none, VK_A), None);
        assert_eq!(format(none, VK_SPACE), None);
        assert_eq!(format(none, VK_F9).as_deref(), Some("F9"));
        assert_eq!(format(none, VK_F24).as_deref(), Some("F24"));
        assert_eq!(format(none, VK_SNAPSHOT).as_deref(), Some("PrintScreen"));
        assert!("F9".parse::<HotKey>().is_ok());
    }

    #[test]
    fn unmapped_keys_are_rejected() {
        let ctrl = Mods { ctrl: true, ..Mods::default() };
        assert_eq!(format(ctrl, VK_CAPITAL), None);
        assert_eq!(format(ctrl, VK_VOLUME_UP), None);
        assert_eq!(format(ctrl, VK_SHIFT), None);
        assert!(is_modifier(VK_LWIN) && is_modifier(VK_MENU) && !is_modifier(VK_A));
    }

    #[test]
    fn super_is_shown_as_win() {
        assert_eq!(display("Ctrl+Super+S"), "Ctrl+Win+S");
        assert_eq!(display("super+cmd+S"), "Win+Win+S");
        assert_eq!(display(""), "");
    }
}
