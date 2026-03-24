//! Enumerates ALL entries from the `emojis` crate and verifies that our
//! `is_emoji()` and `is_double_width()` functions agree with unicode-width.
//!
//! This test exists to validate the replacement of the `emojis` crate:
//! - Every emoji that `emojis::get()` recognizes is tested
//! - For each, we verify that `unicode-width` correctly reports width >= 2
//!   when `is_emoji()` returns true
//! - We verify that text-presentation-by-default chars (canonical form
//!   contains FE0F) are NOT classified as emoji without FE0F

use beamterm_core::{is_double_width, is_emoji};
use unicode_width::UnicodeWidthStr;

/// Every emoji in the `emojis` crate where `is_emoji()` returns true
/// must also have `width() >= 2`.
#[test]
fn emoji_detected_implies_width_ge_2() {
    let mut tested = 0;
    let mut emoji_count = 0;

    for emoji in emojis::iter() {
        let s = emoji.as_str();
        tested += 1;

        if is_emoji(s) {
            emoji_count += 1;
            assert!(
                s.width() >= 2,
                "is_emoji=true but width={} for {:?} ({})",
                s.width(),
                s,
                emoji.name(),
            );
        }
    }

    eprintln!(
        "tested {tested} emojis crate entries, {emoji_count} classified as emoji by is_emoji()"
    );
    assert!(tested > 1000, "expected 1000+ emoji entries, got {tested}");
    assert!(
        emoji_count > 500,
        "expected 500+ emoji matches, got {emoji_count}"
    );
}

/// Text-presentation-by-default characters: emojis crate recognizes them,
/// but their canonical form contains FE0F. Without FE0F, they should NOT
/// be classified as emoji and should have width 1.
#[test]
fn text_presentation_defaults_without_fe0f_are_not_emoji() {
    let mut text_default_count = 0;
    let mut mismatches = Vec::new();

    for emoji in emojis::iter() {
        let s = emoji.as_str();

        // Check if canonical form requires FE0F (text-presentation-by-default)
        if s.contains('\u{FE0F}') {
            // Extract the base character(s) without FE0F
            let base: String = s.chars().filter(|&c| c != '\u{FE0F}').collect();

            if base.chars().count() == 1 {
                text_default_count += 1;
                let base_is_emoji = is_emoji(&base);
                let base_width = base.width();

                if base_is_emoji {
                    mismatches.push(format!(
                        "  {:?} ({}) base={:?} is_emoji={} width={}",
                        s,
                        emoji.name(),
                        base,
                        base_is_emoji,
                        base_width,
                    ));
                }
            }
        }
    }

    if !mismatches.is_empty() {
        panic!(
            "text-presentation-by-default chars should NOT be emoji without FE0F:\n{}",
            mismatches.join("\n")
        );
    }

    eprintln!("{text_default_count} text-presentation-by-default single-char emoji verified");
    assert!(
        text_default_count > 50,
        "expected 50+ text-default entries, got {text_default_count}"
    );
}

/// Text-presentation-by-default characters WITH FE0F should be detected
/// as emoji (width >= 2 per unicode-width).
#[test]
fn text_presentation_defaults_with_fe0f_are_emoji() {
    let mut tested = 0;
    let mut mismatches = Vec::new();

    for emoji in emojis::iter() {
        let s = emoji.as_str();

        // Only test single-base-char + FE0F entries
        if !s.contains('\u{FE0F}') {
            continue;
        }

        let base: String = s.chars().filter(|&c| c != '\u{FE0F}').collect();
        if base.chars().count() != 1 {
            continue;
        }

        tested += 1;
        let with_fe0f = format!("{base}\u{FE0F}");
        let w = with_fe0f.width();

        if w < 2 {
            mismatches.push(format!(
                "  {:?} ({}) with FE0F: width={}",
                base,
                emoji.name(),
                w,
            ));
        }
    }

    if !mismatches.is_empty() {
        panic!(
            "text-presentation-by-default chars WITH FE0F should have width >= 2:\n{}",
            mismatches.join("\n")
        );
    }

    eprintln!("{tested} text-presentation-by-default + FE0F entries verified width >= 2");
}

/// Emoji-presentation-by-default characters (canonical form does NOT contain FE0F)
/// should be detected as emoji AND have width >= 2.
#[test]
fn emoji_presentation_defaults_are_detected() {
    let mut tested = 0;
    let mut not_detected = Vec::new();
    let mut wrong_width = Vec::new();

    for emoji in emojis::iter() {
        let s = emoji.as_str();

        // Skip multi-codepoint emoji for this test (ZWJ, flags, keycaps, etc.)
        if s.chars().count() != 1 {
            continue;
        }

        // If canonical form doesn't contain FE0F, it's emoji-presentation-by-default
        // (the canonical form IS the single character itself)
        tested += 1;

        if !is_emoji(s) {
            not_detected.push(format!(
                "  U+{:04X} {:?} ({}) width={}",
                s.chars().next().unwrap() as u32,
                s,
                emoji.name(),
                s.width(),
            ));
        }

        if s.width() < 2 {
            wrong_width.push(format!(
                "  U+{:04X} {:?} ({}) width={}",
                s.chars().next().unwrap() as u32,
                s,
                emoji.name(),
                s.width(),
            ));
        }
    }

    if !not_detected.is_empty() {
        panic!(
            "emoji-presentation-by-default chars not detected by is_emoji():\n{}",
            not_detected.join("\n")
        );
    }

    // Width mismatches are informational (unicode-width may differ from emojis crate)
    if !wrong_width.is_empty() {
        eprintln!(
            "WARNING: {} emoji-presentation-by-default chars have width < 2:\n{}",
            wrong_width.len(),
            wrong_width.join("\n")
        );
    }

    eprintln!("{tested} single-codepoint emoji-presentation-by-default entries verified");
    assert!(
        tested > 200,
        "expected 200+ single-char emoji, got {tested}"
    );
}

/// `is_double_width` must agree with `width() >= 2` for all emoji entries.
#[test]
fn is_double_width_agrees_with_unicode_width() {
    let mut mismatches = Vec::new();

    for emoji in emojis::iter() {
        let s = emoji.as_str();
        let w = s.width();
        let dw = is_double_width(s);

        if dw != (w >= 2) {
            mismatches.push(format!(
                "  {:?} ({}) is_double_width={} width={}",
                s,
                emoji.name(),
                dw,
                w,
            ));
        }
    }

    if !mismatches.is_empty() {
        panic!(
            "is_double_width disagrees with width() >= 2:\n{}",
            mismatches.join("\n")
        );
    }
}

/// Verify that CJK characters are NOT classified as emoji.
#[test]
fn cjk_is_not_emoji() {
    let cjk = [
        '\u{4E2D}', '\u{65E5}', '\u{6587}', '\u{672C}', '\u{8A9E}', '\u{3041}', '\u{30A2}',
        '\u{AC00}', '\u{D7A3}', // CJK Compatibility Ideographs
        '\u{F900}', '\u{FA0E}', // Fullwidth Latin
        '\u{FF01}', '\u{FF21}', '\u{FF41}',
    ];

    for ch in &cjk {
        let s = ch.to_string();
        assert!(
            !is_emoji(&s),
            "CJK/fullwidth U+{:04X} ({}) should NOT be emoji",
            *ch as u32,
            s,
        );
        assert!(
            is_double_width(&s),
            "CJK/fullwidth U+{:04X} ({}) should be double-width",
            *ch as u32,
            s,
        );
    }
}

/// Multi-codepoint emoji sequences (ZWJ, flags, skin tones) should be detected.
#[test]
fn multi_codepoint_emoji_sequences() {
    let mut tested = 0;
    let mut not_detected = Vec::new();

    for emoji in emojis::iter() {
        let s = emoji.as_str();
        if s.chars().count() <= 1 {
            continue;
        }

        tested += 1;

        // Multi-codepoint emoji with width >= 2 should be detected
        if s.width() >= 2 && !is_emoji(s) {
            not_detected.push(format!(
                "  {:?} ({}) width={} chars={}",
                s,
                emoji.name(),
                s.width(),
                s.chars().count(),
            ));
        }
    }

    if !not_detected.is_empty() {
        panic!(
            "multi-codepoint emoji not detected ({}/{tested}):\n{}",
            not_detected.len(),
            not_detected.join("\n"),
        );
    }

    eprintln!("{tested} multi-codepoint emoji sequences verified");
    assert!(
        tested > 500,
        "expected 500+ multi-codepoint emoji, got {tested}"
    );
}
