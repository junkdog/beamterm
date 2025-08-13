use compact_str::{CompactString, ToCompactString};

/// Represents a single character glyph in a font atlas texture.
///
/// A `Glyph` contains the metadata needed to locate and identify a character
/// within a font atlas texture. Each glyph has a unique ID that maps
/// to its coordinates in a WebGL `TEXTURE_2D_ARRAY`.
///
/// # ASCII Optimization
/// For ASCII characters, the glyph ID directly corresponds to the character's
/// ASCII value, enabling fast lookups without hash table lookups. Non-ASCII
/// characters are assigned sequential IDs starting from a base value.
///
/// # Glyph ID Bit Layout (16-bit)
///
/// | Bit(s) | Flag Name     | Hex Mask | Binary Mask           | Description               |
/// |--------|---------------|----------|-----------------------|---------------------------|
/// | 0-8    | GLYPH_ID      | `0x01FF` | `0000_0001_1111_1111` | Base glyph identifier     |
/// | 9      | BOLD          | `0x0200` | `0000_0010_0000_0000` | Bold font style           |
/// | 10     | ITALIC        | `0x0400` | `0000_0100_0000_0000` | Italic font style         |
/// | 11     | EMOJI         | `0x0800` | `0000_1000_0000_0000` | Emoji character flag      |
/// | 12     | UNDERLINE     | `0x1000` | `0001_0000_0000_0000` | Underline effect          |
/// | 13     | STRIKETHROUGH | `0x2000` | `0010_0000_0000_0000` | Strikethrough effect      |
/// | 14-15  | RESERVED      | `0xC000` | `1100_0000_0000_0000` | Reserved for future use   |
///
/// - The first 9 bits (0-8) represent the base glyph ID, allowing for 512 unique glyphs.
/// - Emoji glyphs implicitly clear any other font style bits.
/// - The fragment shader uses the glyph ID to decode the texture coordinates and effects.
///
/// ## Glyph ID Encoding Examples
///
/// | Character   | Style            | Binary Representation | Hex Value | Description         |
/// |-------------|------------------|-----------------------|-----------|---------------------|
/// | 'A' (0x41)  | Normal           | `0000_0000_0100_0001` | `0x0041`  | Plain 'A'           |
/// | 'A' (0x41)  | Bold             | `0000_0010_0100_0001` | `0x0241`  | Bold 'A'            |
/// | 'A' (0x41)  | Bold + Italic    | `0000_0110_0100_0001` | `0x0641`  | Bold italic 'A'     |
/// | 'A' (0x41)  | Bold + Underline | `0000_1010_0100_0001` | `0x0A41`  | Bold underlined 'A' |
/// | '🚀' (0x81) | Emoji            | `1000_0000_1000_0001` | `0x0881`  | "rocket" emoji      |
#[derive(Debug, Eq, PartialEq)]
pub struct Glyph {
    /// The glyph ID; encodes the 3d texture coordinates
    pub id: u16,
    /// The style of the glyph, e.g., bold, italic
    pub style: FontStyle,
    /// The character
    pub symbol: CompactString,
    /// The pixel coordinates of the glyph in the texture
    pub pixel_coords: (i32, i32),
    /// Indicates if the glyph is an emoji
    pub is_emoji: bool,
}

#[rustfmt::skip]
impl Glyph {
    /// The ID is used as a short-lived placeholder until the actual ID is assigned.
    pub const UNASSIGNED_ID: u16 = 0xFFFF;

    /// Glyph ID mask - extracts the base glyph identifier (bits 0-8).
    /// Supports 512 unique base glyphs (0x000 to 0x1FF) in the texture atlas.
    pub const GLYPH_ID_MASK: u16      = 0b0000_0001_1111_1111; // 0x01FF
    /// Bold flag - selects the bold variant of the glyph from the texture atlas.
    pub const BOLD_FLAG: u16          = 0b0000_0010_0000_0000; // 0x0200
    /// Italic flag - selects the italic variant of the glyph from the texture atlas.
    pub const ITALIC_FLAG: u16        = 0b0000_0100_0000_0000; // 0x0400
    /// Emoji flag - indicates this glyph represents an emoji character requiring special handling.
    pub const EMOJI_FLAG: u16         = 0b0000_1000_0000_0000; // 0x0800
    /// Underline flag - renders a horizontal line below the character baseline.
    pub const UNDERLINE_FLAG: u16     = 0b0001_0000_0000_0000; // 0x1000
    /// Strikethrough flag - renders a horizontal line through the middle of the character.
    pub const STRIKETHROUGH_FLAG: u16 = 0b0010_0000_0000_0000; // 0x2000
}

impl Glyph {
    /// Creates a new glyph with the specified symbol and pixel coordinates.
    pub fn new(symbol: &str, style: FontStyle, pixel_coords: (i32, i32)) -> Self {
        let first_char = symbol.chars().next().unwrap();
        let id = if symbol.len() == 1 && first_char.is_ascii() {
            // Use a different ID for non-ASCII characters
            first_char as u16 | style.style_mask()
        } else {
            Self::UNASSIGNED_ID
        };

        Self {
            id,
            symbol: symbol.to_compact_string(),
            style,
            pixel_coords,
            is_emoji: false,
        }
    }

    pub fn new_with_id(
        base_id: u16,
        symbol: &str,
        style: FontStyle,
        pixel_coords: (i32, i32),
    ) -> Self {
        Self {
            id: base_id | style.style_mask(),
            symbol: symbol.to_compact_string(),
            style,
            pixel_coords,
            is_emoji: false,
        }
    }

    /// Returns true if this glyph represents a single ASCII character.
    pub fn is_ascii(&self) -> bool {
        self.symbol.len() == 1 && self.symbol.chars().next().unwrap().is_ascii()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphEffect {
    /// No special effect applied to the glyph.
    None = 0x0,
    /// Underline effect applied below the glyph.
    Underline = 0x1000,
    /// Strikethrough effect applied through the glyph.
    Strikethrough = 0x2000,
}

impl GlyphEffect {
    pub fn from_u16(v: u16) -> GlyphEffect {
        match v {
            0x0000 => GlyphEffect::None,
            0x1000 => GlyphEffect::Underline,
            0x2000 => GlyphEffect::Strikethrough,
            0x3000 => GlyphEffect::Strikethrough,
            _ => {
                println!("Unknown glyph effect 0x{v:x}");
                panic!("yolo panic");
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontStyle {
    Normal = 0x0000,
    Bold = 0x0200,
    Italic = 0x0400,
    BoldItalic = 0x0600,
}

impl FontStyle {
    pub const ALL: [FontStyle; 4] =
        [FontStyle::Normal, FontStyle::Bold, FontStyle::Italic, FontStyle::BoldItalic];

    pub fn from_u16(v: u16) -> FontStyle {
        match v {
            0x0000 => FontStyle::Normal,
            0x0200 => FontStyle::Bold,
            0x0400 => FontStyle::Italic,
            0x0600 => FontStyle::BoldItalic,
            _ => panic!("Invalid font style value: {v}"),
        }
    }

    pub(super) fn from_ordinal(ordinal: u8) -> FontStyle {
        match ordinal {
            0 => FontStyle::Normal,
            1 => FontStyle::Bold,
            2 => FontStyle::Italic,
            3 => FontStyle::BoldItalic,
            _ => panic!("Invalid font style ordinal: {ordinal}"),
        }
    }

    pub(super) const fn ordinal(&self) -> usize {
        match self {
            FontStyle::Normal => 0,
            FontStyle::Bold => 1,
            FontStyle::Italic => 2,
            FontStyle::BoldItalic => 3,
        }
    }

    /// Returns the style bits for this font style, used to encode the style in the glyph ID.
    pub const fn style_mask(&self) -> u16 {
        *self as u16
    }
}
