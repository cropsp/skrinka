//! An in-memory `DLGTEMPLATEEX` for `DialogBoxIndirectParamW`. The `windows` crate has no struct
//! for it (the format has variable-length strings), so the words are written one by one.
//! Layout: <https://learn.microsoft.com/windows/win32/dlgbox/dlgtemplateex> and
//! `dlgitemtemplateex`. The whole template, and every item in it, is DWORD-aligned.

/// Predefined control classes (by atom) or a registered class by name.
#[derive(Debug, Clone, Copy)]
pub enum Class {
    Button,
    Edit,
    Static,
    ComboBox,
    Named(&'static str),
}

/// x, y, width, height in dialog units.
pub type DlgRect = (i16, i16, i16, i16);

pub struct Template {
    words: Vec<u16>,
    items: u16,
}

/// Word index of `cDlgItems` in the header.
const COUNT_INDEX: usize = 8;

impl Template {
    /// Header with `DS_SETFONT` font info; `style` must include `DS_SETFONT`.
    pub fn new(title: &str, style: u32, ex_style: u32, rect: DlgRect, font: &str, point_size: u16) -> Template {
        let mut t = Template { words: Vec::with_capacity(1024), items: 0 };
        t.words.extend([1, 0xFFFF]); // dlgVer, signature
        t.dword(0); // helpID
        t.dword(ex_style);
        t.dword(style);
        t.words.push(0); // cDlgItems, filled in by `item`
        t.rect(rect);
        t.words.extend([0, 0]); // no menu, default dialog class
        t.text(title);
        t.words.extend([point_size, 400]); // weight
        t.words.push(0x0100); // italic = 0 (low byte), charset = 1 = DEFAULT_CHARSET (high byte)
        t.text(font);
        t
    }

    pub fn item(&mut self, class: Class, text: &str, id: u32, style: u32, ex_style: u32, rect: DlgRect) {
        self.align();
        self.dword(0); // helpID
        self.dword(ex_style);
        self.dword(style);
        self.rect(rect);
        self.dword(id);
        let atom = match class {
            Class::Button => Some(0x0080),
            Class::Edit => Some(0x0081),
            Class::Static => Some(0x0082),
            Class::ComboBox => Some(0x0085),
            Class::Named(name) => {
                self.text(name);
                None
            }
        };
        if let Some(atom) = atom {
            self.words.extend([0xFFFF, atom]);
        }
        self.text(text);
        self.words.push(0); // extraCount
        self.items += 1;
        self.words[COUNT_INDEX] = self.items;
    }

    /// The template as DWORDs, so its start is DWORD-aligned in memory.
    pub fn into_dwords(mut self) -> Vec<u32> {
        self.align();
        let (pairs, _) = self.words.as_chunks::<2>();
        pairs.iter().map(|&[low, high]| u32::from(low) | (u32::from(high) << 16)).collect()
    }

    fn dword(&mut self, value: u32) {
        self.words.extend([value as u16, (value >> 16) as u16]);
    }

    fn rect(&mut self, (x, y, cx, cy): DlgRect) {
        self.words.extend([x as u16, y as u16, cx as u16, cy as u16]);
    }

    fn text(&mut self, text: &str) {
        self.words.extend(text.encode_utf16());
        self.words.push(0);
    }

    fn align(&mut self) {
        if self.words.len() % 2 == 1 {
            self.words.push(0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(dwords: &[u32]) -> Vec<u16> {
        dwords.iter().flat_map(|d| [*d as u16, (*d >> 16) as u16]).collect()
    }

    #[test]
    fn header_layout() {
        let t = Template::new("Ab", 0x80C8_00C0, 0x10, (1, 2, 300, 200), "Segoe UI", 9);
        let w = words(&t.into_dwords());
        assert_eq!(&w[..2], [1, 0xFFFF]);
        assert_eq!(&w[2..4], [0, 0]); // helpID
        assert_eq!(&w[4..6], [0x10, 0]); // exStyle
        assert_eq!(&w[6..8], [0x00C0, 0x80C8]); // style, low word first
        assert_eq!(w[8], 0); // no items
        assert_eq!(&w[9..13], [1, 2, 300, 200]);
        assert_eq!(&w[13..15], [0, 0]); // menu, class
        assert_eq!(&w[15..18], [u16::from(b'A'), u16::from(b'b'), 0]);
        assert_eq!(&w[18..21], [9, 400, 0x0100]);
        let face: Vec<u16> = "Segoe UI".encode_utf16().chain([0]).collect();
        assert_eq!(&w[21..21 + face.len()], &face[..]);
    }

    #[test]
    fn items_are_dword_aligned_and_counted() {
        let mut t = Template::new("Odd", 0, 0, (0, 0, 10, 10), "Segoe UI", 9);
        let starts: Vec<usize> = [
            (Class::Static, "x"),
            (Class::Button, "OK"),
            (Class::Named("SysLink"), "<a>link</a>"),
            (Class::Edit, ""),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (class, text))| {
            t.align();
            let start = t.words.len();
            t.item(class, text, 100 + i as u32, 0x5001_0000, 0, (1, 2, 3, 4));
            start
        })
        .collect();
        assert!(starts.iter().all(|s| s % 2 == 0), "{starts:?}");
        let w = words(&t.into_dwords());
        assert_eq!(w[COUNT_INDEX], 4);
        // Second item (a Button): helpID, exStyle, style, rect, id, then the class atom.
        let s = starts[1];
        assert_eq!(&w[s + 4..s + 6], [0x0000, 0x5001]);
        assert_eq!(&w[s + 6..s + 10], [1, 2, 3, 4]);
        assert_eq!(&w[s + 10..s + 12], [101, 0]);
        assert_eq!(&w[s + 12..s + 14], [0xFFFF, 0x0080]);
        assert_eq!(&w[s + 14..s + 17], [u16::from(b'O'), u16::from(b'K'), 0]);
        assert_eq!(w[s + 17], 0); // extraCount
        // Third item: the class by name instead of an atom.
        let s = starts[2];
        let name: Vec<u16> = "SysLink".encode_utf16().chain([0]).collect();
        assert_eq!(&w[s + 12..s + 12 + name.len()], &name[..]);
    }

    #[test]
    fn length_is_whole_dwords() {
        let mut t = Template::new("A", 0, 0, (0, 0, 1, 1), "B", 9);
        t.item(Class::ComboBox, "", 1, 0, 0, (0, 0, 1, 1));
        let len_words = t.words.len();
        let dwords = t.into_dwords();
        assert_eq!(dwords.len(), len_words.div_ceil(2));
    }
}
