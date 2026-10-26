//! Reading an attached text file (txt, md, csv, code) into Unicode text
//! (internal plan 38, F1; docs/DECISIONS.md, 2026-10-26).
//!
//! No library: UTF-8 and UTF-16 come from the standard library, and
//! Windows-1251, which many Russian text files are in, is the fixed table
//! below (the WHATWG encoding index). The order of the guesses:
//! 1. a byte-order mark: UTF-8, UTF-16 little-endian or big-endian;
//! 2. UTF-16 without a mark, told by its zero bytes in every other place;
//! 3. bytes that are valid UTF-8 are UTF-8;
//! 4. otherwise Windows-1251.
//!
//! A file with zero bytes or many control characters left after that is
//! not text, and is refused rather than shown as garbage.

/// The encoding a file was read in, for the log and for the person.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Utf8,
    Utf16Le,
    Utf16Be,
    Windows1251,
}

impl Encoding {
    pub fn name(self) -> &'static str {
        match self {
            Encoding::Utf8 => "UTF-8",
            Encoding::Utf16Le => "UTF-16 (little-endian)",
            Encoding::Utf16Be => "UTF-16 (big-endian)",
            Encoding::Windows1251 => "Windows-1251",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Decoded {
    pub text: String,
    pub encoding: Encoding,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum NotText {
    #[error("the file is empty")]
    Empty,
    #[error("the file isn't text")]
    Binary,
}

/// Windows-1251's bytes 0x80–0xBF; 0xC0–0xFF are U+0410–U+044F in order.
const CP1251_HIGH: [char; 64] = [
    '\u{0402}', '\u{0403}', '\u{201A}', '\u{0453}', '\u{201E}', '\u{2026}', '\u{2020}', '\u{2021}',
    '\u{20AC}', '\u{2030}', '\u{0409}', '\u{2039}', '\u{040A}', '\u{040C}', '\u{040B}', '\u{040F}',
    '\u{0452}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}', '\u{2022}', '\u{2013}', '\u{2014}',
    '\u{0098}', '\u{2122}', '\u{0459}', '\u{203A}', '\u{045A}', '\u{045C}', '\u{045B}', '\u{045F}',
    '\u{00A0}', '\u{040E}', '\u{045E}', '\u{0408}', '\u{00A4}', '\u{0490}', '\u{00A6}', '\u{00A7}',
    '\u{0401}', '\u{00A9}', '\u{0404}', '\u{00AB}', '\u{00AC}', '\u{00AD}', '\u{00AE}', '\u{0407}',
    '\u{00B0}', '\u{00B1}', '\u{0406}', '\u{0456}', '\u{0491}', '\u{00B5}', '\u{00B6}', '\u{00B7}',
    '\u{0451}', '\u{2116}', '\u{0454}', '\u{00BB}', '\u{0458}', '\u{0405}', '\u{0455}', '\u{0457}',
];

fn windows_1251(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&b| match b {
            0x00..=0x7F => b as char,
            0x80..=0xBF => CP1251_HIGH[(b - 0x80) as usize],
            // 0xC0..=0xFF: А..я
            _ => char::from_u32(0x0410 + u32::from(b - 0xC0)).unwrap_or('\u{FFFD}'),
        })
        .collect()
}

fn utf16(bytes: &[u8], little_endian: bool) -> String {
    let units = bytes.chunks_exact(2).map(|pair| {
        if little_endian {
            u16::from_le_bytes([pair[0], pair[1]])
        } else {
            u16::from_be_bytes([pair[0], pair[1]])
        }
    });
    char::decode_utf16(units)
        .map(|unit| unit.unwrap_or('\u{FFFD}'))
        .collect()
}

/// UTF-16 without a byte-order mark: mostly-ASCII text leaves a zero in
/// every other byte. Which half holds the zeros says the byte order.
fn utf16_without_mark(bytes: &[u8]) -> Option<bool> {
    let sample = &bytes[..bytes.len().min(4096) & !1];
    if sample.len() < 4 {
        return None;
    }
    let pairs = sample.len() / 2;
    let zeros_odd = sample
        .iter()
        .skip(1)
        .step_by(2)
        .filter(|&&b| b == 0)
        .count();
    let zeros_even = sample.iter().step_by(2).filter(|&&b| b == 0).count();
    // Most pairs have their zero in one half and almost none in the other.
    if zeros_odd * 10 >= pairs * 6 && zeros_even * 10 <= pairs {
        Some(true)
    } else if zeros_even * 10 >= pairs * 6 && zeros_odd * 10 <= pairs {
        Some(false)
    } else {
        None
    }
}

/// Text has no NUL and few control characters besides tab, line feed,
/// carriage return and form feed.
fn looks_like_text(text: &str) -> bool {
    let mut chars = 0usize;
    let mut control = 0usize;
    for c in text.chars().take(65_536) {
        chars += 1;
        if c == '\0' {
            return false;
        }
        if c.is_control() && !matches!(c, '\t' | '\n' | '\r' | '\u{000C}') {
            control += 1;
        }
    }
    control * 100 <= chars
}

/// Read `bytes` as text, or say why it isn't.
pub fn decode(bytes: &[u8]) -> Result<Decoded, NotText> {
    let (text, encoding) = if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        (String::from_utf8_lossy(rest).into_owned(), Encoding::Utf8)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFF, 0xFE]) {
        (utf16(rest, true), Encoding::Utf16Le)
    } else if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        (utf16(rest, false), Encoding::Utf16Be)
    } else if let Some(little_endian) = utf16_without_mark(bytes) {
        // Before UTF-8: ASCII in UTF-16 is also valid UTF-8, zeros and all.
        let encoding = if little_endian {
            Encoding::Utf16Le
        } else {
            Encoding::Utf16Be
        };
        (utf16(bytes, little_endian), encoding)
    } else if let Ok(text) = std::str::from_utf8(bytes) {
        (text.to_owned(), Encoding::Utf8)
    } else {
        (windows_1251(bytes), Encoding::Windows1251)
    };
    if text.trim().is_empty() {
        return Err(NotText::Empty);
    }
    if !looks_like_text(&text) {
        return Err(NotText::Binary);
    }
    Ok(Decoded { text, encoding })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cp1251(text: &str) -> Vec<u8> {
        text.chars()
            .map(|c| {
                let code = c as u32;
                if code < 0x80 {
                    code as u8
                } else if (0x0410..=0x044F).contains(&code) {
                    (code - 0x0410 + 0xC0) as u8
                } else {
                    let at = CP1251_HIGH
                        .iter()
                        .position(|&h| h == c)
                        .expect("in the table");
                    0x80 + at as u8
                }
            })
            .collect()
    }

    #[test]
    fn utf8_with_and_without_a_mark() {
        let plain = decode("Привет, oʻzbek tili! Ёлка.".as_bytes()).unwrap();
        assert_eq!(plain.encoding, Encoding::Utf8);
        assert_eq!(plain.text, "Привет, oʻzbek tili! Ёлка.");
        let mut marked = vec![0xEF, 0xBB, 0xBF];
        marked.extend_from_slice("Hello".as_bytes());
        assert_eq!(decode(&marked).unwrap().text, "Hello");
    }

    #[test]
    fn utf16_both_byte_orders_with_a_mark_and_little_endian_without() {
        let text = "Договор № 15, сумма 2 340 000 сўм";
        let le: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let be: Vec<u8> = text.encode_utf16().flat_map(u16::to_be_bytes).collect();
        let with = |mark: [u8; 2], body: &[u8]| [mark.as_slice(), body].concat();
        let a = decode(&with([0xFF, 0xFE], &le)).unwrap();
        assert_eq!((a.text.as_str(), a.encoding), (text, Encoding::Utf16Le));
        let b = decode(&with([0xFE, 0xFF], &be)).unwrap();
        assert_eq!((b.text.as_str(), b.encoding), (text, Encoding::Utf16Be));
        let ascii = "name,amount\nrent,8000000\nsalaries,45000000\n";
        let bare: Vec<u8> = ascii.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let c = decode(&bare).unwrap();
        assert_eq!((c.text.as_str(), c.encoding), (ascii, Encoding::Utf16Le));
    }

    #[test]
    fn windows_1251_russian_and_its_punctuation() {
        let text = "Счёт № 7 — «Ўзбекистон», 15 ноября, €100 и ½… нет: 100 ₽";
        // ½ and ₽ aren't in Windows-1251; the rest is.
        let text = text.replace(['½', '₽'], "");
        let decoded = decode(&cp1251(&text)).unwrap();
        assert_eq!(decoded.encoding, Encoding::Windows1251);
        assert_eq!(decoded.text, text);
    }

    #[test]
    fn every_windows_1251_byte_has_one_character() {
        let all: Vec<u8> = (0x80..=0xFFu8).collect();
        let text = windows_1251(&all);
        assert_eq!(text.chars().count(), 128);
        assert!(!text.contains('\u{FFFD}'));
        assert!(
            text.contains('Ё') && text.contains('ё') && text.contains('ў') && text.contains('№')
        );
    }

    #[test]
    fn empty_and_binary_files_are_refused() {
        assert_eq!(decode(b""), Err(NotText::Empty));
        assert_eq!(decode(b"  \n\t "), Err(NotText::Empty));
        let png = [
            0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0x0D, b'I', b'H', b'D', b'R',
        ];
        assert_eq!(decode(&png), Err(NotText::Binary));
        let noise: Vec<u8> = (0..400u32).map(|i| (i * 37 % 32) as u8).collect();
        assert_eq!(decode(&noise), Err(NotText::Binary));
    }

    #[test]
    fn code_and_csv_stay_byte_for_byte() {
        let code = "fn main() {\r\n\tprintln!(\"{}\", 1 + 1);\r\n}\r\n";
        assert_eq!(decode(code.as_bytes()).unwrap().text, code);
        let csv = "Item;October;November\nRent;8 000 000;8 000 000\n";
        assert_eq!(decode(csv.as_bytes()).unwrap().text, csv);
    }
}
