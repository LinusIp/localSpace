//! Reading an attached text file (txt, md, csv, code) into Unicode text
//! (internal plan 38, F1; docs/DECISIONS.md, 2026-10-26).
//!
//! No library: UTF-8 and UTF-16 come from the standard library, and
//! Windows-1251, which many Russian text files are in, is the fixed table
//! below (the WHATWG encoding index). The order of the guesses:
//! 1. a byte-order mark: UTF-8, UTF-16 little-endian or big-endian;
//! 2. UTF-16 without a mark, told by its zero bytes in every other place;
//! 3. bytes that are UTF-8, or nearly (more good characters beyond ASCII
//!    than invalid sequences: a character cut at the end, a stray byte),
//!    are UTF-8;
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
    let units = bytes.as_chunks::<2>().0.iter().map(|&pair| {
        if little_endian {
            u16::from_le_bytes(pair)
        } else {
            u16::from_be_bytes(pair)
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
/// carriage return, form feed and escape (a coloured log's colours).
fn looks_like_text(text: &str) -> bool {
    if text.contains('\0') {
        return false;
    }
    let mut chars = 0usize;
    let mut control = 0usize;
    for c in text.chars().take(65_536) {
        chars += 1;
        if c.is_control() && !matches!(c, '\t' | '\n' | '\r' | '\u{000C}' | '\u{001B}') {
            control += 1;
        }
    }
    control * 100 <= chars
}

/// UTF-8, or nearly: a file cut in the middle of its last character, or
/// with a stray byte, keeps its good characters rather than all of them
/// being read as Windows-1251, when it has more good characters beyond
/// ASCII than invalid sequences. Windows-1251 Cyrillic makes almost no
/// valid UTF-8 beyond ASCII, so it never passes.
fn mostly_utf8(bytes: &[u8]) -> Option<String> {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return Some(text.to_owned());
    }
    let (mut good, mut bad) = (0usize, 0usize);
    for chunk in bytes.utf8_chunks() {
        good += chunk.valid().chars().filter(|c| !c.is_ascii()).count();
        if !chunk.invalid().is_empty() {
            bad += 1;
        }
    }
    (good > bad).then(|| String::from_utf8_lossy(bytes).into_owned())
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
    } else if let Some(text) = mostly_utf8(bytes) {
        (text, Encoding::Utf8)
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

/// How much of a file the model reads, by the prompt's own count
/// ([`crate::prompt::tokens_of`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Read {
    pub chars: u64,
    pub words: u64,
    pub tokens: u64,
}

/// The words in `text`, as a person would count them: Chinese and
/// Japanese, written without spaces, by their characters.
pub fn words(text: &str) -> u64 {
    text.split_whitespace()
        .map(|word| match word.chars().filter(|&c| is_cjk(c)).count() {
            0 => 1,
            n => n as u64,
        })
        .sum()
}

fn is_cjk(c: char) -> bool {
    matches!(
        c,
        '\u{3040}'..='\u{30FF}' | '\u{3400}'..='\u{4DBF}' | '\u{4E00}'..='\u{9FFF}' | '\u{F900}'..='\u{FAFF}'
    )
}

/// The tokens each of one message's files may take out of `budget`: none
/// more than it needs, and otherwise an equal share of what is left, so a
/// long file never leaves the others nothing (internal plan 38, F4).
pub fn share(budget: usize, needs: &[usize]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..needs.len()).collect();
    order.sort_by_key(|&i| needs[i]);
    let mut shares = vec![0; needs.len()];
    let mut left = budget;
    for (done, &i) in order.iter().enumerate() {
        shares[i] = needs[i].min(left / (needs.len() - done));
        left -= shares[i];
    }
    shares
}

/// The part of `text` that fits in `tokens`: all of it when it fits,
/// otherwise its start, cut between characters, at the last line break in
/// the final tenth of what fits when there is one.
pub fn read_within(text: &str, tokens: usize) -> Read {
    let whole = crate::prompt::tokens_of(text);
    if whole <= tokens {
        return Read {
            chars: text.chars().count() as u64,
            words: words(text),
            tokens: whole as u64,
        };
    }
    // The longest start that fits, by the same count.
    let mut counted = crate::prompt::Counted::default();
    let mut end = 0;
    for (at, c) in text.char_indices() {
        counted.add(c);
        if counted.tokens() > tokens {
            break;
        }
        end = at + c.len_utf8();
    }
    if let Some(line) = text[..end].rfind('\n').filter(|&at| at * 10 >= end * 9) {
        end = line + 1;
    }
    let part = &text[..end];
    Read {
        chars: part.chars().count() as u64,
        words: words(part),
        tokens: crate::prompt::tokens_of(part) as u64,
    }
}

/// The first `chars` characters of `text`.
pub fn first_chars(text: &str, chars: u64) -> &str {
    match text.char_indices().nth(chars as usize) {
        Some((at, _)) => &text[..at],
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_that_fits_is_read_whole_and_a_long_one_from_its_start() {
        let short = "one two three\n";
        let r = read_within(short, 100);
        assert_eq!((r.chars, r.words), (14, 3));
        let long = "строка текста\n".repeat(1000);
        let r = read_within(&long, 500);
        assert!(r.tokens <= 500, "{r:?}");
        assert!(r.chars > 0 && r.chars < long.chars().count() as u64);
        let part = first_chars(&long, r.chars);
        assert!(
            part.ends_with('\n'),
            "cut at a line break: {:?}",
            &part[part.len() - 20..]
        );
        assert_eq!(words(part), r.words);
    }

    #[test]
    fn a_cut_never_splits_a_character() {
        let text = "ё".repeat(10_000);
        for tokens in [1, 7, 333, 2_500] {
            let r = read_within(&text, tokens);
            let part = first_chars(&text, r.chars);
            assert!(part.chars().all(|c| c == 'ё'));
            assert!(r.tokens as usize <= tokens);
        }
    }

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
    fn a_utf8_file_with_a_damaged_character_stays_utf8() {
        let text = "Договор аренды № 15. Сумма: 8 000 000 сўм.\n".repeat(50);
        // Cut in the middle of its last character.
        let cut = &text.as_bytes()[..text.len() - 3];
        let read = decode(cut).unwrap();
        assert_eq!(read.encoding, Encoding::Utf8);
        assert!(read.text.starts_with("Договор аренды № 15."));
        // A stray byte among good characters.
        let mut stray = text.as_bytes().to_vec();
        stray.insert(100, 0xA0);
        let read = decode(&stray).unwrap();
        assert_eq!(read.encoding, Encoding::Utf8);
        assert!(read.text.ends_with("сўм.\n"));
        // Windows-1251 is still Windows-1251.
        let plain = text.replace('ў', "у");
        assert_eq!(
            decode(&cp1251(&plain)).unwrap().encoding,
            Encoding::Windows1251
        );
    }

    #[test]
    fn a_short_utf8_note_cut_in_its_last_character_stays_utf8() {
        let note = "Привет";
        let cut = &note.as_bytes()[..note.len() - 1];
        let read = decode(cut).unwrap();
        assert_eq!(read.encoding, Encoding::Utf8);
        assert!(read.text.starts_with("Приве"));
        let header = "Дата;Сумма\n2026-10-01;8 000 000\n";
        let mut stray = header.as_bytes().to_vec();
        stray.push(0xB9);
        assert_eq!(decode(&stray).unwrap().encoding, Encoding::Utf8);
    }

    #[test]
    fn a_zero_byte_late_in_a_file_is_not_text() {
        let mut late = "a".repeat(70_000).into_bytes();
        late.push(0);
        assert_eq!(decode(&late), Err(NotText::Binary));
    }

    #[test]
    fn a_coloured_log_is_text_and_many_control_characters_are_not() {
        let log = "\u{1b}[32m   Compiling\u{1b}[0m localspace-core v0.2.0\n".repeat(200);
        assert_eq!(decode(log.as_bytes()).unwrap().text, log);
        // No zero byte, but more than one character in a hundred is a control.
        let noisy = "abc\u{1}\u{2}\u{3}defghij\n".repeat(100);
        assert_eq!(decode(noisy.as_bytes()), Err(NotText::Binary));
    }

    #[test]
    fn big_endian_utf16_without_a_mark() {
        let ascii = "name,amount\nrent,8000000\n";
        let bare: Vec<u8> = ascii.encode_utf16().flat_map(u16::to_be_bytes).collect();
        let read = decode(&bare).unwrap();
        assert_eq!(
            (read.text.as_str(), read.encoding),
            (ascii, Encoding::Utf16Be)
        );
    }

    #[test]
    fn digits_are_counted_one_by_one_so_a_table_of_figures_fits() {
        let csv = "2026-10-01;1234567.89;4500000.00;12;0.15\n".repeat(2000);
        let r = read_within(&csv, 4000);
        let part = first_chars(&csv, r.chars);
        // Digits and the marks between them are a token each.
        let alone = part
            .chars()
            .filter(|c| c.is_ascii_digit() || c.is_ascii_punctuation())
            .count();
        assert!(
            alone <= 4000,
            "{alone} digits and marks in a 4000-token part"
        );
        assert!(r.tokens <= 4000 && r.tokens as usize >= alone);
        // A labelled table: each row's label and line break count too.
        let rows: String = (1..=5000)
            .map(|i| format!("Row {i};8 000 000;12.5\n"))
            .collect();
        let r = read_within(&rows, 4000);
        let part = first_chars(&rows, r.chars);
        let pieces: usize = part
            .lines()
            .map(|row| crate::prompt::tokens_of(row) + 1)
            .sum();
        assert!(pieces <= 4000 + 1, "{pieces} pieces in a 4000-token part");
        assert!(part.ends_with('\n'));
    }

    #[test]
    fn chinese_is_counted_by_its_characters() {
        assert_eq!(words("今天天气很好"), 6);
        assert_eq!(words("Hello 世界 again"), 4);
    }

    #[test]
    fn files_share_the_budget_and_none_is_left_with_nothing() {
        // A short file gets all it needs; the long one, the rest.
        assert_eq!(share(4000, &[30_000, 12]), vec![3988, 12]);
        // Long files share it equally.
        assert_eq!(share(4000, &[9_000, 9_000, 9_000, 9_000]), vec![1000; 4]);
        // What fits is read whole.
        assert_eq!(share(4000, &[100, 200]), vec![100, 200]);
        assert_eq!(share(0, &[5, 5]), vec![0, 0]);
        let shares = share(4000, &[5_000, 700, 30_000]);
        assert_eq!(shares[1], 700);
        assert!(shares.iter().sum::<usize>() <= 4000);
    }

    #[test]
    fn code_and_csv_stay_byte_for_byte() {
        let code = "fn main() {\r\n\tprintln!(\"{}\", 1 + 1);\r\n}\r\n";
        assert_eq!(decode(code.as_bytes()).unwrap().text, code);
        let csv = "Item;October;November\nRent;8 000 000;8 000 000\n";
        assert_eq!(decode(csv.as_bytes()).unwrap().text, csv);
    }
}
