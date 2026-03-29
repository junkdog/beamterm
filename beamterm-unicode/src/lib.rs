//! Unicode character classification utilities for beamterm.
//!
//! Provides emoji detection and double-width character classification
//! shared across the beamterm workspace crates.

use unicode_width::UnicodeWidthStr;

/// Checks if a grapheme is an emoji that should use color font rendering.
///
/// Uses UTF-8 byte-level checks and a codepoint table to avoid calling
/// `unicode-width` for single-codepoint strings (the common case). Only
/// multi-codepoint sequences (ZWJ, flags, keycaps, text + FE0F) fall
/// through to a `width()` check.
#[must_use]
pub fn is_emoji(s: &str) -> bool {
    let bytes = s.as_bytes();
    let Some(&first_byte) = bytes.first() else {
        return false;
    };

    // ASCII (1 byte, U+0000–U+007F): single ASCII is never emoji, but
    // multi-codepoint sequences starting with ASCII can be (e.g. keycap "1️⃣").
    if first_byte < 0x80 {
        return s.len() > 1 && s.width() >= 2;
    }

    // 2-byte UTF-8 (U+0080–U+07FF): no emoji exist in this range.
    if first_byte < 0xE0 {
        return s.len() > 2 && s.width() >= 2;
    }

    // 3+ byte UTF-8: decode the first codepoint.
    // SAFETY: we verified the string is non-empty and starts with a 3+ byte sequence.
    let first = unsafe { s.chars().next().unwrap_unchecked() };
    let first_len = first.len_utf8();

    // Single codepoint
    if s.len() == first_len {
        // 3-byte (BMP, U+0800–U+FFFF): emoji table is exact — skip width().
        // 4-byte (SMP, U+10000+): range check is broad, verify with width().
        return if first_len == 3 {
            is_emoji_presentation(first)
        } else {
            s.width() >= 2 && is_emoji_presentation(first)
        };
    }

    // Multi-codepoint: emoji if wide (ZWJ, flags, skin tones, text + FE0F).
    s.width() >= 2
}

/// Checks if a grapheme is double-width (emoji or fullwidth character).
#[must_use]
pub fn is_double_width(grapheme: &str) -> bool {
    grapheme.width() >= 2
}

/// Returns `true` for characters with emoji-presentation-by-default that
/// `unicode-width` reports as width 2. This covers BMP emoji (60 code
/// points) and SMP emoji (U+1F000–U+1FFFF), excluding CJK Enclosed
/// Ideographic Supplement characters that are wide but not emoji.
///
/// Derived from cross-referencing every entry in the `emojis` 0.8 crate
/// against `unicode-width` 0.2 — see `tests/enumerate_emojis_crate.rs`.
fn is_emoji_presentation(c: char) -> bool {
    let cp = c as u32;

    match cp {
        // BMP emoji with default emoji presentation (60 code points, U+231A–U+2B55).
        0x231A..=0x2B55 => matches!(
            cp,
            0x231A..=0x231B   // ⌚⌛
            | 0x23E9..=0x23EC // ⏩⏪⏫⏬
            | 0x23F0           // ⏰
            | 0x23F3           // ⏳
            | 0x25FD..=0x25FE // ◽◾
            | 0x2614..=0x2615 // ☔☕
            | 0x2648..=0x2653 // ♈..♓
            | 0x267F           // ♿
            | 0x2693           // ⚓
            | 0x26A1           // ⚡
            | 0x26AA..=0x26AB // ⚪⚫
            | 0x26BD..=0x26BE // ⚽⚾
            | 0x26C4..=0x26C5 // ⛄⛅
            | 0x26CE           // ⛎
            | 0x26D4           // ⛔
            | 0x26EA           // ⛪
            | 0x26F2..=0x26F3 // ⛲⛳
            | 0x26F5           // ⛵
            | 0x26FA           // ⛺
            | 0x26FD           // ⛽
            | 0x2705           // ✅
            | 0x270A..=0x270B // ✊✋
            | 0x2728           // ✨
            | 0x274C           // ❌
            | 0x274E           // ❎
            | 0x2753..=0x2755 // ❓❔❕
            | 0x2757           // ❗
            | 0x2795..=0x2797 // ➕➖➗
            | 0x27B0           // ➰
            | 0x27BF           // ➿
            | 0x2B1B..=0x2B1C // ⬛⬜
            | 0x2B50           // ⭐
            | 0x2B55           // ⭕
        ),
        // SMP emoji: nearly all characters in U+1F000–U+1FFFF are emoji.
        // Exclude CJK Enclosed Ideographic Supplement (EAW=W text symbols).
        0x1F000..=0x1FFFF => !matches!(
            cp,
            0x1F200
                | 0x1F202..=0x1F219
                | 0x1F21B..=0x1F22E
                | 0x1F230..=0x1F231
                | 0x1F237
                | 0x1F23B..=0x1F24F
                | 0x1F260..=0x1F265
        ),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_emoji() {
        // Emoji-presentation-by-default: always emoji
        assert!(is_emoji("\u{1F680}"));
        assert!(is_emoji("\u{1F600}"));
        assert!(is_emoji("\u{23E9}"));
        assert!(is_emoji("\u{23EA}"));

        // Text-presentation-by-default with FE0F: emoji
        assert!(is_emoji("\u{25B6}\u{FE0F}"));

        // Text-presentation-by-default without FE0F: NOT emoji
        assert!(!is_emoji("\u{25B6}"));
        assert!(!is_emoji("\u{25C0}"));
        assert!(!is_emoji("\u{23ED}"));
        assert!(!is_emoji("\u{23F9}"));
        assert!(!is_emoji("\u{23EE}"));
        assert!(!is_emoji("\u{25AA}"));
        assert!(!is_emoji("\u{25AB}"));
        assert!(!is_emoji("\u{25FC}"));

        // Not emoji
        assert!(!is_emoji("A"));
        assert!(!is_emoji("\u{2588}"));
    }

    #[test]
    fn test_is_double_width() {
        // emoji-presentation-by-default
        assert!(is_double_width("\u{1F600}"));
        assert!(is_double_width(
            "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}"
        )); // ZWJ sequence

        [
            "\u{231A}", "\u{231B}", "\u{23E9}", "\u{23F3}", "\u{2614}", "\u{2615}", "\u{2648}",
            "\u{2653}", "\u{267F}", "\u{2693}", "\u{26A1}", "\u{26AA}", "\u{26AB}", "\u{26BD}",
            "\u{26BE}", "\u{26C4}", "\u{26C5}", "\u{26CE}", "\u{26D4}", "\u{26EA}", "\u{26F2}",
            "\u{26F3}", "\u{26F5}", "\u{26FA}", "\u{26FD}", "\u{25FE}", "\u{2B1B}", "\u{2B1C}",
            "\u{2B50}", "\u{2B55}", "\u{3030}", "\u{303D}", "\u{3297}", "\u{3299}",
        ]
        .iter()
        .for_each(|s| {
            assert!(is_double_width(s), "Failed for emoji: {s}");
        });

        // text-presentation-by-default with FE0F: double-width
        assert!(is_double_width("\u{25B6}\u{FE0F}"));
        assert!(is_double_width("\u{25C0}\u{FE0F}"));

        // text-presentation-by-default without FE0F: single-width
        assert!(!is_double_width("\u{23F8}"));
        assert!(!is_double_width("\u{23FA}"));
        assert!(!is_double_width("\u{25AA}"));
        assert!(!is_double_width("\u{25AB}"));
        assert!(!is_double_width("\u{25B6}"));
        assert!(!is_double_width("\u{25C0}"));
        assert!(!is_double_width("\u{25FB}"));
        assert!(!is_double_width("\u{2934}"));
        assert!(!is_double_width("\u{2935}"));
        assert!(!is_double_width("\u{2B05}"));
        assert!(!is_double_width("\u{2B07}"));
        assert!(!is_double_width("\u{26C8}"));

        // CJK
        assert!(is_double_width("\u{4E2D}"));
        assert!(is_double_width("\u{65E5}"));

        // single-width
        assert!(!is_double_width("A"));
        assert!(!is_double_width("\u{2192}"));
    }
}
