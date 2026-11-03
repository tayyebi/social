//! Terminal text pipeline shared by the CLI and the TUI.
//!
//! Order (PLAN.md §17): logical text → line segmentation → wrapping by display
//! width → bidi reordering → rendering. Glyph shaping is **not** implemented;
//! terminals that lack Arabic/Persian shaping will show disconnected letters.
//! That limitation is documented rather than hidden.

use unicode_bidi::{BidiInfo, Direction};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

/// Wrap logical text to `width` display columns, preserving explicit newlines.
/// Never splits a UTF-8 code point or a grapheme cluster boundary in the middle
/// of a character.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![String::new()];
    }
    let mut out = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.is_empty() {
            out.push(String::new());
            continue;
        }
        let mut current = String::new();
        let mut current_width = 0usize;
        for ch in paragraph.chars() {
            let ch_width = ch.width().unwrap_or(0);
            if current_width + ch_width > width && !current.is_empty() {
                // Prefer breaking at the last space so words stay whole.
                if let Some(index) = current.rfind(' ') {
                    let tail = current[index + 1..].to_string();
                    current.truncate(index);
                    out.push(current);
                    current = tail;
                    current_width = UnicodeWidthStr::width(current.as_str());
                } else {
                    out.push(std::mem::take(&mut current));
                    current_width = 0;
                }
            }
            current.push(ch);
            current_width += ch_width;
        }
        out.push(current);
    }
    out
}

/// Base direction of a paragraph: first strong character wins.
pub fn base_direction(text: &str) -> Direction {
    for ch in text.chars() {
        if ch.is_alphabetic() {
            return if is_rtl_char(ch) {
                Direction::Rtl
            } else {
                Direction::Ltr
            };
        }
    }
    Direction::Ltr
}

fn is_rtl_char(ch: char) -> bool {
    matches!(ch as u32,
        0x0590..=0x05FF      // Hebrew
        | 0x0600..=0x06FF    // Arabic
        | 0x0700..=0x074F    // Syriac
        | 0x0750..=0x077F    // Arabic supplement
        | 0x0780..=0x07BF    // Thaana
        | 0x08A0..=0x08FF    // Arabic extended-A
        | 0xFB1D..=0xFDFF    // Hebrew/Arabic presentation forms
        | 0xFE70..=0xFEFF) // Arabic presentation forms-B
}

/// Reorder one logical line into terminal visual order.
pub fn visual_line(line: &str) -> String {
    let bidi = BidiInfo::new(line, Some(base_direction(line)));
    if bidi.paragraphs.is_empty() {
        return line.to_string();
    }
    let paragraph = &bidi.paragraphs[0];
    let range = paragraph.range.clone();
    bidi.reorder_line(paragraph, range).to_string()
}

/// Wrap, then reorder each line for the terminal.
pub fn layout(text: &str, width: usize) -> Vec<String> {
    wrap(text, width)
        .into_iter()
        .map(|line| visual_line(&line))
        .collect()
}

/// Display width of a string, in terminal columns.
pub fn width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

/// Truncate to `width` columns without splitting a code point.
pub fn truncate(text: &str, width: usize) -> String {
    if UnicodeWidthStr::width(text) <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0usize;
    for ch in text.chars() {
        let ch_width = ch.width().unwrap_or(0);
        if used + ch_width > width.saturating_sub(1) {
            break;
        }
        out.push(ch);
        used += ch_width;
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PERSIAN: [&str; 6] = [
        "سلام دنیا",
        "سلام، این یک آزمایش است.",
        "Hello سلام world",
        "این پیام از social-mail است.",
        "شناسه: ABCD1234",
        "متن فارسی، با نیم‌فاصله و اعداد ۱۲۳۴۵.",
    ];

    #[test]
    fn persian_fixtures_survive_layout() {
        for fixture in PERSIAN {
            let lines = layout(fixture, 40);
            let joined: String = lines.join("");
            // Same characters, same count: nothing dropped or corrupted.
            assert_eq!(joined.chars().count(), fixture.chars().count());
            assert!(joined.contains('\u{200c}') == fixture.contains('\u{200c}'));
        }
    }

    #[test]
    fn mixed_direction_lines_stay_within_width() {
        for fixture in PERSIAN {
            for width in [10usize, 20, 40, 80] {
                for line in layout(fixture, width) {
                    assert!(
                        UnicodeWidthStr::width(line.as_str()) <= width,
                        "line {line:?} exceeds {width}"
                    );
                }
            }
        }
    }

    #[test]
    fn empty_and_control_input_is_safe() {
        assert_eq!(layout("", 10), vec![String::new()]);
        assert_eq!(wrap("a\n\nb", 10), vec!["a", "", "b"]);
        // Zero-width joiners and combining marks must not panic.
        let tricky = "می‌خواهم\u{200d}\u{0301}";
        let _ = layout(tricky, 5);
        let _ = layout(tricky, 1);
        let _ = layout(tricky, 0);
    }

    #[test]
    fn very_long_unbroken_text_is_broken() {
        let long = "x".repeat(200);
        let lines = layout(&long, 20);
        assert!(lines.len() >= 10);
        assert!(lines
            .iter()
            .all(|line| UnicodeWidthStr::width(line.as_str()) <= 20));
    }

    #[test]
    fn identifiers_stay_latin_and_readable() {
        let line = visual_line("شناسه: ABCD1234");
        assert!(line.contains("ABCD1234"));
        assert_eq!(base_direction("ABC"), Direction::Ltr);
        assert_eq!(base_direction("سلام"), Direction::Rtl);
        assert_eq!(base_direction("1234"), Direction::Ltr);
    }

    #[test]
    fn truncation_respects_width_and_utf8() {
        assert_eq!(truncate("سلام دنیا", 5), "سلا…");
        assert_eq!(truncate("abc", 10), "abc");
        assert_eq!(width("سلام"), 8);
    }
}
