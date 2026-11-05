//! Word's `w:sym` characters. Word stores a symbol as a font and a code in that
//! font rather than as text, so a Symbol font Greek letter holds no character
//! until it is mapped to Unicode. Both Word readers use this mapping, because a
//! comment's quote must match the extracted text.

use quick_xml::events::BytesStart;

/// Stands in for a symbol that cannot be mapped, so its place stays visible.
pub(crate) const UNREADABLE_SYMBOL: char = '\u{FFFD}';

/// The Symbol font from 0x20 to 0x7E.
const SYMBOL_LOW: &str = " !∀#∃%&∋()∗+,−./0123456789:;<=>?≅ΑΒΧΔΕΦΓΗΙϑΚΛΜΝΟΠΘΡΣΤΥςΩΞΨΖ[∴]⊥_‾αβχδεφγηιϕκλμνοπθρστυϖωξψζ{|}∼";
/// The Symbol font from 0xA0 to 0xFE. 0xF0 has no character.
const SYMBOL_HIGH: &str = "€ϒ′≤⁄∞ƒ♣♦♥♠↔←↑→↓°±″≥×∝∂•÷≠≡≈…⏐⎯↵ℵℑℜ℘⊗⊕∅∩∪⊃⊇⊄⊂⊆∈∉∠∇®©™∏√⋅¬∧∨⇔⇐⇑⇒⇓◊〈®©™∑⎛⎜⎝⎡⎢⎣⎧⎨⎩⎪\u{FFFD}〉∫⌠⎮⌡⎞⎟⎠⎤⎥⎦⎫⎬⎭";

/// The Wingdings characters manuscripts use: bullets, boxes, arrows, and marks.
const WINGDINGS: &[(u32, char)] = &[
    (0x4A, '☺'),
    (0x4C, '☹'),
    (0x6C, '●'),
    (0x6E, '■'),
    (0x6F, '□'),
    (0x71, '❑'),
    (0x75, '◆'),
    (0x76, '❖'),
    (0x9F, '•'),
    (0xA7, '▪'),
    (0xA8, '◻'),
    (0xD8, '➢'),
    (0xDF, '←'),
    (0xE0, '→'),
    (0xE1, '↑'),
    (0xE2, '↓'),
    (0xE8, '➔'),
    (0xF0, '⇨'),
    (0xFB, '✗'),
    (0xFC, '✓'),
    (0xFD, '☒'),
    (0xFE, '☑'),
];

fn symbol_font(code: u32) -> Option<char> {
    let index = |first: u32| (code - first) as usize;
    match code {
        0x20..=0x7E => SYMBOL_LOW.chars().nth(index(0x20)),
        0xA0..=0xFE => SYMBOL_HIGH
            .chars()
            .nth(index(0xA0))
            .filter(|character| *character != UNREADABLE_SYMBOL),
        _ => None,
    }
}

fn wingdings(code: u32) -> Option<char> {
    WINGDINGS
        .iter()
        .find(|(position, _)| *position == code)
        .map(|(_, character)| *character)
}

/// The Unicode character for a font and hexadecimal code, when it is known.
/// Symbol fonts store a one-byte position, usually in the 0xF000 private range.
fn map_symbol(font: &str, code: &str) -> Option<char> {
    let code = u32::from_str_radix(code.trim(), 16).ok()?;
    let position = match code {
        0xF000..=0xF0FF => code - 0xF000,
        _ => code,
    };
    match font.trim().to_ascii_lowercase().as_str() {
        "symbol" => symbol_font(position),
        "wingdings" => wingdings(position),
        _ if (0xE000..=0xF8FF).contains(&code) => None,
        _ => char::from_u32(code),
    }
}

/// The character a `w:sym` element shows, or `UNREADABLE_SYMBOL`.
pub(crate) fn symbol_char(element: &BytesStart<'_>) -> char {
    let attribute = |name: &str| {
        element
            .try_get_attribute(name)
            .ok()
            .flatten()
            .map(|value| String::from_utf8_lossy(&value.value).into_owned())
    };
    attribute("w:font")
        .zip(attribute("w:char"))
        .and_then(|(font, code)| map_symbol(&font, &code))
        .unwrap_or(UNREADABLE_SYMBOL)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_symbol_tables_cover_every_position() {
        assert_eq!(SYMBOL_LOW.chars().count(), 0x7E - 0x20 + 1);
        assert_eq!(SYMBOL_HIGH.chars().count(), 0xFE - 0xA0 + 1);
    }

    #[test]
    fn word_codes_reach_the_characters_they_show() {
        for (font, code, character) in [
            ("Symbol", "F062", 'β'),
            ("Symbol", "0062", 'β'),
            ("symbol", "f061", 'α'),
            ("Symbol", "F0A3", '≤'),
            ("Symbol", "F0B4", '×'),
            ("Symbol", "F0E5", '∑'),
            ("Symbol", "F0F2", '∫'),
            ("Wingdings", "F0FC", '✓'),
            ("Times New Roman", "03B2", 'β'),
        ] {
            assert_eq!(map_symbol(font, code), Some(character), "{font} {code}");
        }
    }

    #[test]
    fn an_unknown_symbol_is_not_guessed() {
        for (font, code) in [
            ("Webdings", "F062"),
            ("Wingdings", "F021"),
            ("Symbol", "F0F0"),
            ("Symbol", "F010"),
            ("Symbol", "not hex"),
        ] {
            assert_eq!(map_symbol(font, code), None, "{font} {code}");
        }
    }
}
