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
/// # Glyph ID Bit Layout (32-bit)
///
/// | Bit(s) | Flag Name     | Hex Mask   | Binary Mask           | Description               |
/// |--------|---------------|------------|-----------------------|---------------------------|
/// | 0-19   | GLYPH_ID      | `0xFFFFF`  | (20 bits)             | Base glyph identifier     |
/// | 20     | BOLD          | `0x100000` | `0001_0000_0000_0000_0000_0000` | Bold font style           |
/// | 21     | ITALIC        | `0x200000` | `0010_0000_0000_0000_0000_0000` | Italic font style         |
/// | 22     | EMOJI         | `0x400000` | `0100_0000_0000_0000_0000_0000` | Emoji character flag      |
/// | 23     | UNDERLINE     | `0x800000` | `1000_0000_0000_0000_0000_0000` | Underline effect          |
/// | 24     | STRIKETHROUGH | `0x1000000`| (bit 24)              | Strikethrough effect      |
/// | 25-31  | RESERVED      |            |                       | Reserved for future use   |
///
/// - The first 20 bits (0-19) represent the base glyph ID, allowing for 1,048,576 unique glyphs.
/// - Emoji glyphs implicitly clear any other font style bits.
/// - The fragment shader uses the glyph ID to decode the texture coordinates and effects.
///
/// ## Glyph ID Encoding Examples
///
/// | Character   | Style            | Binary Representation | Hex Value | Description         |
/// |-------------|------------------|-----------------------|-----------|---------------------|
/// | 'A' (0x41)  | Normal           | (bits 0-19 = 0x41)    | `0x00000041` | Plain 'A'           |
/// | 'A' (0x41)  | Bold             | (bit 20 set)          | `0x00100041` | Bold 'A'            |
/// | 'A' (0x41)  | Bold + Italic    | (bits 20-21 set)      | `0x00300041` | Bold italic 'A'     |
/// | 'A' (0x41)  | Bold + Underline | (bits 20, 23 set)     | `0x00900041` | Bold underlined 'A' |
/// | '🚀' (0x1000)| Emoji            | (bit 22 set)          | `0x00401000` | "rocket" emoji      |
#[derive(Debug, Eq, PartialEq)]
pub struct Glyph {
    /// The glyph ID; encodes the 3d texture coordinates
    pub id: u32,
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
    pub const UNASSIGNED_ID: u32 = 0xFFFFFFFF;

    /// Glyph ID mask - extracts the base glyph identifier (bits 0-19).
    /// Supports 1,048,576 unique base glyphs (0x00000 to 0xFFFFF) in the texture atlas.
    pub const GLYPH_ID_MASK: u32       = 0x000FFFFF; // 20 bits
    /// Glyph ID mask for emoji - extracts the base glyph identifier (bits 0-22).
    /// Supports up to 1,048,576 emoji glyphs occupying two slots each in the texture atlas.
    pub const GLYPH_ID_EMOJI_MASK: u32 = 0x007FFFFF; // 23 bits (20 base + 3 style flags)
    /// Bold flag - selects the bold variant of the glyph from the texture atlas.
    pub const BOLD_FLAG: u32           = 0x00100000; // bit 20
    /// Italic flag - selects the italic variant of the glyph from the texture atlas.
    pub const ITALIC_FLAG: u32         = 0x00200000; // bit 21
    /// Emoji flag - indicates this glyph represents an emoji character requiring special handling.
    pub const EMOJI_FLAG: u32          = 0x00400000; // bit 22
    /// Underline flag - renders a horizontal line below the character baseline.
    pub const UNDERLINE_FLAG: u32      = 0x00800000; // bit 23
    /// Strikethrough flag - renders a horizontal line through the middle of the character.
    pub const STRIKETHROUGH_FLAG: u32  = 0x01000000; // bit 24
}

impl Glyph {
    /// Creates a new glyph with the specified symbol and pixel coordinates.
    pub fn new(symbol: &str, style: FontStyle, pixel_coords: (i32, i32)) -> Self {
        let first_char = symbol.chars().next().unwrap();
        let id = if symbol.len() == 1 && first_char.is_ascii() {
            // Use a different ID for non-ASCII characters
            first_char as u32 | style.style_mask()
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
        base_id: u32,
        symbol: &str,
        style: FontStyle,
        pixel_coords: (i32, i32),
    ) -> Self {
        Self {
            id: base_id | style.style_mask(),
            symbol: symbol.to_compact_string(),
            style,
            pixel_coords,
            is_emoji: (base_id & Self::EMOJI_FLAG) != 0,
        }
    }

    pub fn new_emoji(base_id: u32, symbol: &str, pixel_coords: (i32, i32)) -> Self {
        Self {
            id: base_id | Self::EMOJI_FLAG,
            symbol: symbol.to_compact_string(),
            style: FontStyle::Normal, // Emoji glyphs do not have style variants
            pixel_coords,
            is_emoji: true,
        }
    }

    /// Returns true if this glyph represents a single ASCII character.
    pub fn is_ascii(&self) -> bool {
        self.symbol.len() == 1 && self.symbol.chars().next().unwrap().is_ascii()
    }

    /// Returns the base glyph ID without style flags.
    ///
    /// For non-emoji glyphs, this masks off the style bits (bold/italic) using
    /// [`GLYPH_ID_MASK`](Self::GLYPH_ID_MASK) to extract just the base identifier (bits 0-19).
    /// For emoji glyphs, returns the full ID since emoji don't use style variants.
    ///
    /// # Examples
    ///
    /// ```
    /// use beamterm_data::{Glyph, FontStyle};
    ///
    /// // Bold 'A' (0x00100041) -> base ID 0x41
    /// let bold_a = Glyph::new_with_id(0x41, "A", FontStyle::Bold, (0, 0));
    /// assert_eq!(bold_a.id, 0x00100041);
    /// assert_eq!(bold_a.base_id(), 0x41);
    ///
    /// // Emoji retains full ID
    /// let emoji = Glyph::new_emoji(0x1000, "🚀", (0, 0));
    /// assert_eq!(emoji.base_id(), 0x00401000); // includes EMOJI_FLAG
    /// ```
    pub fn base_id(&self) -> u32 {
        if self.is_emoji {
            self.id & Self::GLYPH_ID_EMOJI_MASK
        } else {
            self.id & Self::GLYPH_ID_MASK
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GlyphEffect {
    /// No special effect applied to the glyph.
    None = 0x0,
    /// Underline effect applied below the glyph.
    Underline = 0x00800000, // bit 23
    /// Strikethrough effect applied through the glyph.
    Strikethrough = 0x01000000, // bit 24
}

impl GlyphEffect {
    pub fn from_u32(v: u32) -> GlyphEffect {
        match v {
            0x00000000 => GlyphEffect::None,
            0x00800000 => GlyphEffect::Underline,
            0x01000000 => GlyphEffect::Strikethrough,
            0x01800000 => GlyphEffect::Strikethrough, // both flags
            _ => GlyphEffect::None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontStyle {
    Normal = 0x00000000,
    Bold = 0x00100000,      // bit 20
    Italic = 0x00200000,   // bit 21
    BoldItalic = 0x00300000, // bits 20-21
}

impl FontStyle {
    pub const ALL: [FontStyle; 4] =
        [FontStyle::Normal, FontStyle::Bold, FontStyle::Italic, FontStyle::BoldItalic];

    pub fn from_u32(v: u32) -> FontStyle {
        match v & 0x00300000 {
            0x00000000 => FontStyle::Normal,
            0x00100000 => FontStyle::Bold,
            0x00200000 => FontStyle::Italic,
            0x00300000 => FontStyle::BoldItalic,
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
    pub const fn style_mask(&self) -> u32 {
        *self as u32
    }
}
