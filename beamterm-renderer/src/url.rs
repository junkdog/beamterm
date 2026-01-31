use compact_str::CompactString;

use crate::{SelectionMode, TerminalGrid, gl::CellQuery, position::CursorPosition, select};

/// Result of URL detection containing the query and extracted URL text.
pub struct UrlMatch {
    /// A `CellQuery` configured with the URL's start and end positions.
    pub query: CellQuery,
    /// The extracted URL string.
    pub url: CompactString,
}

/// Characters that are valid within a URL (RFC 3986 unreserved + reserved).
fn is_url_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
        || matches!(
            ch,
            '-' | '.'
                | '_'
                | '~'
                | ':'
                | '/'
                | '?'
                | '#'
                | '['
                | ']'
                | '@'
                | '!'
                | '$'
                | '&'
                | '\''
                | '('
                | ')'
                | '*'
                | '+'
                | ','
                | ';'
                | '='
                | '%'
        )
}

/// Characters that should be trimmed from the end of a URL.
fn is_trailing_punctuation(ch: char) -> bool {
    matches!(ch, '.' | ',' | ';' | ':' | '!' | '?')
}

/// Detects an HTTP/HTTPS URL at or around the given cursor position.
///
/// Scans left to find a URL scheme (`http://` or `https://`), then scans right
/// to find the URL end. Handles trailing punctuation and unbalanced parentheses.
///
/// Returns `None` if no URL is found at the cursor position.
pub(super) fn find_url_at_cursor(cursor: CursorPosition, grid: &TerminalGrid) -> Option<UrlMatch> {
    let cols = grid.terminal_size().0;

    // Find scheme start by scanning left
    let scheme_start = find_scheme_start(cursor, grid, cols)?;

    // Verify and get scheme length
    let scheme_len = if matches_sequence(grid, scheme_start, "https://", cols) {
        8
    } else if matches_sequence(grid, scheme_start, "http://", cols) {
        7
    } else {
        return None;
    };

    // Scan right from after scheme, tracking paren balance
    let after_scheme = scheme_start.move_right(scheme_len, cols)?;
    let (raw_end, paren_balance) = scan_url_extent(after_scheme, grid, cols);

    // Trim trailing punctuation and unbalanced close parens
    let url_end = trim_url_end(scheme_start, raw_end, paren_balance, grid);

    // Verify cursor is within the URL bounds
    if cursor.col < scheme_start.col || cursor.col > url_end.col {
        return None;
    }

    // Now extract the text
    let query = select(SelectionMode::Linear)
        .start((scheme_start.col, scheme_start.row))
        .end((url_end.col, url_end.row));

    let url = grid.get_text(query);

    Some(UrlMatch { query, url })
}

/// Scans left from the cursor to find the start of a URL scheme.
fn find_scheme_start(
    cursor: CursorPosition,
    grid: &TerminalGrid,
    cols: u16,
) -> Option<CursorPosition> {
    let mut pos = cursor;

    loop {
        // Check if this position starts a valid scheme
        if grid.get_ascii_char_at(pos) == Some('h')
            && (matches_sequence(grid, pos, "https://", cols)
                || matches_sequence(grid, pos, "http://", cols))
        {
            return Some(pos);
        }

        // Move left, stop if we hit the start of the row
        pos = pos.move_left(1)?;
    }
}

/// Checks if a sequence of characters matches starting at the given position.
fn matches_sequence(grid: &TerminalGrid, start: CursorPosition, seq: &str, cols: u16) -> bool {
    let mut pos = start;
    let char_count = seq.chars().count();

    for (i, ch) in seq.chars().enumerate() {
        if grid.get_ascii_char_at(pos) != Some(ch) {
            return false;
        }
        // Move right for next character, but not after the last one
        if i < char_count - 1 {
            match pos.move_right(1, cols) {
                Some(next) => pos = next,
                None => return false, // Can't advance but more chars remain
            }
        }
    }
    true
}

/// Scans right from the starting position to find the extent of a URL.
///
/// Returns the end position and the parenthesis balance (positive means more '(' than ')').
fn scan_url_extent(start: CursorPosition, grid: &TerminalGrid, cols: u16) -> (CursorPosition, i32) {
    let mut pos = start;
    let mut paren_balance: i32 = 0;
    let mut last_valid = start;

    loop {
        match grid.get_ascii_char_at(pos) {
            Some(ch) if is_url_char(ch) => {
                if ch == '(' {
                    paren_balance += 1;
                } else if ch == ')' {
                    paren_balance -= 1;
                }
                last_valid = pos;
            },
            _ => break,
        }

        match pos.move_right(1, cols) {
            Some(next) => pos = next,
            None => break,
        }
    }

    (last_valid, paren_balance)
}

/// Trims trailing punctuation and unbalanced closing parentheses from the URL end.
fn trim_url_end(
    start: CursorPosition,
    mut end: CursorPosition,
    mut paren_balance: i32,
    grid: &TerminalGrid,
) -> CursorPosition {
    // Work backwards, trimming trailing punctuation and unbalanced ')'
    while end.col > start.col {
        let ch = match grid.get_ascii_char_at(end) {
            Some(c) => c,
            None => break,
        };

        if is_trailing_punctuation(ch) {
            // Trim trailing punctuation
            end = end.move_left(1).unwrap_or(end);
        } else if ch == ')' && paren_balance < 0 {
            // Trim unbalanced closing paren
            paren_balance += 1;
            end = end.move_left(1).unwrap_or(end);
        } else {
            break;
        }
    }

    end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_url_char() {
        // Valid URL characters
        assert!(is_url_char('a'));
        assert!(is_url_char('Z'));
        assert!(is_url_char('0'));
        assert!(is_url_char('-'));
        assert!(is_url_char('.'));
        assert!(is_url_char('/'));
        assert!(is_url_char('?'));
        assert!(is_url_char('='));
        assert!(is_url_char('&'));
        assert!(is_url_char('('));
        assert!(is_url_char(')'));

        // Invalid URL characters
        assert!(!is_url_char(' '));
        assert!(!is_url_char('\n'));
        assert!(!is_url_char('<'));
        assert!(!is_url_char('>'));
        assert!(!is_url_char('"'));
    }

    #[test]
    fn test_is_trailing_punctuation() {
        assert!(is_trailing_punctuation('.'));
        assert!(is_trailing_punctuation(','));
        assert!(is_trailing_punctuation(';'));
        assert!(is_trailing_punctuation(':'));
        assert!(is_trailing_punctuation('!'));
        assert!(is_trailing_punctuation('?'));

        assert!(!is_trailing_punctuation('/'));
        assert!(!is_trailing_punctuation('-'));
        assert!(!is_trailing_punctuation('a'));
    }
}
