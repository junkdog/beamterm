//! Verify that unicode-width alone can replace the `emojis` crate for width detection.
//!
//! The current code calls `is_emoji(s)` then `s.width()` separately — double work
//! if unicode-width already returns 2 for emoji-presentation-by-default characters
//! and 1 for text-presentation-by-default characters (without FE0F).

use unicode_width::UnicodeWidthStr;

/// Emoji_Presentation=Yes characters must report width 2.
#[test]
fn emoji_presentation_default_is_width_2() {
    let cases = [
        ("\u{1F680}", "ROCKET"),
        ("\u{1F600}", "GRINNING FACE"),
        ("\u{23E9}", "FAST-FORWARD"),
        ("\u{23EA}", "REWIND"),
        ("\u{231A}", "WATCH"),
        ("\u{231B}", "HOURGLASS"),
        ("\u{23F3}", "HOURGLASS WITH FLOWING SAND"),
        ("\u{2614}", "UMBRELLA WITH RAIN"),
        ("\u{2615}", "HOT BEVERAGE"),
        ("\u{2648}", "ARIES"),
        ("\u{2653}", "PISCES"),
        ("\u{267F}", "WHEELCHAIR"),
        ("\u{2693}", "ANCHOR"),
        ("\u{26A1}", "HIGH VOLTAGE"),
        ("\u{26AA}", "MEDIUM WHITE CIRCLE"),
        ("\u{26AB}", "MEDIUM BLACK CIRCLE"),
        ("\u{26BD}", "SOCCER BALL"),
        ("\u{26BE}", "BASEBALL"),
        ("\u{26C4}", "SNOWMAN WITHOUT SNOW"),
        ("\u{26C5}", "SUN BEHIND CLOUD"),
        ("\u{26CE}", "OPHIUCHUS"),
        ("\u{26D4}", "NO ENTRY"),
        ("\u{26EA}", "CHURCH"),
        ("\u{26F2}", "FOUNTAIN"),
        ("\u{26F3}", "FLAG IN HOLE"),
        ("\u{26F5}", "SAILBOAT"),
        ("\u{26FA}", "TENT"),
        ("\u{26FD}", "FUEL PUMP"),
        ("\u{25FE}", "BLACK MEDIUM SMALL SQUARE"),
        ("\u{2B1B}", "BLACK LARGE SQUARE"),
        ("\u{2B1C}", "WHITE LARGE SQUARE"),
        ("\u{2B50}", "STAR"),
        ("\u{2B55}", "HOLLOW RED CIRCLE"),
        ("\u{3030}", "WAVY DASH"),
        ("\u{303D}", "PART ALTERNATION MARK"),
        ("\u{3297}", "CIRCLED IDEOGRAPH CONGRATULATION"),
        ("\u{3299}", "CIRCLED IDEOGRAPH SECRET"),
    ];

    for (s, name) in &cases {
        assert_eq!(s.width(), 2, "{name} ({s}) should be width 2");
    }
}

/// Text-presentation-by-default characters WITHOUT FE0F must be width 1.
/// These are recognized by the `emojis` crate but should NOT be treated as emoji.
#[test]
fn text_presentation_default_without_fe0f_is_width_1() {
    let cases = [
        ("\u{25B6}", "BLACK RIGHT-POINTING TRIANGLE"),
        ("\u{25C0}", "BLACK LEFT-POINTING TRIANGLE"),
        ("\u{23ED}", "NEXT TRACK"),
        ("\u{23F9}", "STOP"),
        ("\u{23EE}", "PREVIOUS TRACK"),
        ("\u{25AA}", "BLACK SMALL SQUARE"),
        ("\u{25AB}", "WHITE SMALL SQUARE"),
        ("\u{25FC}", "BLACK MEDIUM SQUARE"),
        ("\u{23F8}", "PAUSE"),
        ("\u{23FA}", "RECORD"),
        ("\u{2934}", "ARROW CURVING UP"),
        ("\u{2935}", "ARROW CURVING DOWN"),
        ("\u{2B05}", "LEFT ARROW"),
        ("\u{2B07}", "DOWN ARROW"),
        ("\u{26C8}", "THUNDER CLOUD AND RAIN"),
    ];

    for (s, name) in &cases {
        assert_eq!(s.width(), 1, "{name} ({s}) should be width 1 without FE0F");
    }
}

/// Text-presentation-by-default characters WITH FE0F should be width 2.
#[test]
fn text_presentation_with_fe0f_is_width_2() {
    let cases = [
        ("\u{25B6}\u{FE0F}", "BLACK RIGHT-POINTING TRIANGLE + FE0F"),
        ("\u{25C0}\u{FE0F}", "BLACK LEFT-POINTING TRIANGLE + FE0F"),
        ("\u{23ED}\u{FE0F}", "NEXT TRACK + FE0F"),
        ("\u{23F9}\u{FE0F}", "STOP + FE0F"),
    ];

    for (s, name) in &cases {
        assert_eq!(s.width(), 2, "{name} should be width 2 with FE0F");
    }
}

/// ZWJ sequences should be width 2.
#[test]
fn zwj_sequences_are_width_2() {
    let family = "\u{1F468}\u{200D}\u{1F469}\u{200D}\u{1F467}";
    assert!(
        family.width() >= 2,
        "ZWJ family sequence should be width >= 2"
    );
}

/// CJK characters are width 2 but NOT emoji.
#[test]
fn cjk_is_width_2() {
    let cases = [("\u{4E2D}", "CJK zhong"), ("\u{65E5}", "CJK ri"), ("\u{6587}", "CJK wen")];

    for (s, name) in &cases {
        assert_eq!(s.width(), 2, "{name} ({s}) should be width 2");
    }
}

/// Single-width non-emoji characters.
#[test]
fn single_width_non_emoji() {
    let cases = [("A", "LATIN A"), ("\u{2192}", "RIGHTWARDS ARROW"), ("\u{2588}", "FULL BLOCK")];

    for (s, name) in &cases {
        assert_eq!(s.width(), 1, "{name} ({s}) should be width 1");
    }
}
