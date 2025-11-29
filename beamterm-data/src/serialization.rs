use compact_str::{format_compact, CompactString};

use crate::{FontAtlasData, FontStyle, Glyph, LineDecoration};

const ATLAS_HEADER: [u8; 4] = [0xBA, 0xB1, 0xF0, 0xA7];
const ATLAS_VERSION: u8 = 0x03; // dictates the format of the serialized data

#[derive(Debug)]
pub struct SerializationError {
    pub message: CompactString,
}

pub(crate) trait Serializable {
    fn serialize(&self) -> Vec<u8>;

    fn deserialize(deser: &mut Deserializer) -> Result<Self, SerializationError>
    where
        Self: Sized;
}

pub(crate) struct Deserializer<'a> {
    data: &'a [u8],
    position: usize,
}

struct Serializer {
    data: Vec<u8>,
}

impl Serializer {
    pub fn new() -> Self {
        Self { data: Vec::new() }
    }

    pub fn write_u8(&mut self, value: u8) {
        self.data.push(value);
    }

    pub fn write_u16(&mut self, value: u16) {
        self.data.extend(&value.to_le_bytes());
    }

    pub fn write_u32(&mut self, value: u32) {
        self.data.extend(&value.to_le_bytes());
    }

    pub fn write_f32(&mut self, value: f32) {
        self.data.extend(&value.to_le_bytes());
    }

    pub fn write_i32(&mut self, value: i32) {
        self.data.extend(&value.to_le_bytes());
    }

    pub fn write_u8_slice(&mut self, value: &[u8]) {
        self.write_u32(value.len() as u32);
        for &v in value {
            self.write_u8(v);
        }
    }

    pub fn write_string(&mut self, value: &str) {
        let length = value.len() as u8;
        self.write_u8(length);
        self.data.extend(value.as_bytes());
    }
}

impl<'a> Deserializer<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Self { data, position: 0 }
    }

    pub fn read_u8(&mut self) -> Result<u8, SerializationError> {
        self.verify_offset_in_bounds(1)?;

        let byte = self.data[self.position];
        self.position += 1;

        Ok(byte)
    }

    pub fn read_u16(&mut self) -> Result<u16, SerializationError> {
        self.verify_offset_in_bounds(2)?;

        let bytes = &self.data[self.position..self.position + 2];
        self.position += 2;

        Ok(u16::from_le_bytes(bytes.try_into().unwrap()))
    }

    pub fn read_f32(&mut self) -> Result<f32, SerializationError> {
        self.verify_offset_in_bounds(4)?;

        let bytes = &self.data[self.position..self.position + 4];
        self.position += 4;

        Ok(f32::from_le_bytes(bytes.try_into().unwrap()))
    }

    pub fn read_u32(&mut self) -> Result<u32, SerializationError> {
        self.verify_offset_in_bounds(4)?;

        let bytes = &self.data[self.position..self.position + 4];
        self.position += 4;

        Ok(u32::from_le_bytes(bytes.try_into().unwrap()))
    }

    pub fn read_u8_slice(&mut self) -> Result<Vec<u8>, SerializationError> {
        let length = self.read_u32()? as usize;
        self.verify_offset_in_bounds(length)?;

        let mut values = Vec::with_capacity(length);
        for _ in 0..length {
            values.push(self.read_u8()?);
        }

        Ok(values)
    }

    pub fn read_i32(&mut self) -> Result<i32, SerializationError> {
        self.verify_offset_in_bounds(4)?;

        let bytes = &self.data[self.position..self.position + 4];
        self.position += 4;

        Ok(i32::from_le_bytes(bytes.try_into().unwrap()))
    }

    pub fn read_string(&mut self) -> Result<CompactString, SerializationError> {
        let length = self.read_u8()? as usize;
        self.verify_offset_in_bounds(length)?;

        let bytes = &self.data[self.position..self.position + length];
        self.position += length;

        Ok(CompactString::from_utf8_lossy(bytes))
    }

    fn verify_offset_in_bounds(&self, length: usize) -> Result<(), SerializationError> {
        if (self.position + length) > self.data.len() {
            return Err(SerializationError { message: CompactString::from("Out of bounds read") });
        }
        Ok(())
    }
}

impl Serializable for CompactString {
    fn serialize(&self) -> Vec<u8> {
        let mut ser = Serializer::new();
        ser.write_string(self);
        ser.data
    }

    fn deserialize(serialized: &mut Deserializer) -> Result<Self, SerializationError> {
        serialized.read_string()
    }
}

impl Serializable for Glyph {
    fn serialize(&self) -> Vec<u8> {
        let mut ser = Serializer::new();
        ser.write_u16(self.id);
        ser.write_u8(self.style.ordinal() as u8);
        ser.write_u8(self.is_emoji as u8);
        ser.write_i32(self.pixel_coords.0);
        ser.write_i32(self.pixel_coords.1);
        ser.write_string(&self.symbol);
        ser.data
    }

    fn deserialize(serialized: &mut Deserializer) -> Result<Self, SerializationError> {
        let id = serialized.read_u16()?;
        let style = serialized.read_u8()?;
        let is_emoji = serialized.read_u8()? != 0;
        let x = serialized.read_i32()?;
        let y = serialized.read_i32()?;
        let symbol = serialized.read_string()?;

        Ok(Glyph {
            id,
            style: FontStyle::from_ordinal(style),
            is_emoji,
            pixel_coords: (x, y),
            symbol,
        })
    }
}

impl Serializable for FontAtlasData {
    fn serialize(&self) -> Vec<u8> {
        let mut ser = Serializer::new();
        ser.write_u8(ATLAS_HEADER[0]);
        ser.write_u8(ATLAS_HEADER[1]);
        ser.write_u8(ATLAS_HEADER[2]);
        ser.write_u8(ATLAS_HEADER[3]);

        ser.write_u8(ATLAS_VERSION);

        ser.write_string(&self.font_name);
        ser.write_f32(self.font_size);
        ser.write_u32(self.halfwidth_glyphs_per_layer);

        ser.write_i32(self.texture_dimensions.0);
        ser.write_i32(self.texture_dimensions.1);
        ser.write_i32(self.texture_dimensions.2);

        ser.write_i32(self.cell_size.0);
        ser.write_i32(self.cell_size.1);

        ser.write_f32(self.underline.position);
        ser.write_f32(self.underline.thickness);
        ser.write_f32(self.strikethrough.position);
        ser.write_f32(self.strikethrough.thickness);

        // serialize the glyphs
        ser.write_u16(self.glyphs.len() as u16);
        ser.data
            .extend(self.glyphs.iter().flat_map(Glyph::serialize));

        // serialize 3d texture data
        let packed_texture_data = miniz_oxide::deflate::compress_to_vec(&self.texture_data, 9);
        ser.write_u8_slice(&packed_texture_data);

        ser.data
    }

    fn deserialize(deser: &mut Deserializer) -> Result<Self, SerializationError> {
        let header = [deser.read_u8()?, deser.read_u8()?, deser.read_u8()?, deser.read_u8()?];
        if header != ATLAS_HEADER {
            return Err(SerializationError {
                message: CompactString::const_new("Invalid font atlas header (wrong file format?)"),
            });
        }

        let version = deser.read_u8()?;
        if version != ATLAS_VERSION {
            return Err(SerializationError {
                message: format_compact!(
                    "Atlas version mismatch: expected v{}, found v{}. \
                     Please regenerate atlas with current beamterm-atlas version.",
                    ATLAS_VERSION,
                    version
                ),
            });
        }

        let font_name = deser.read_string()?;
        let font_size = deser.read_f32()?;
        let halfwidth_glyphs_per_layer = deser.read_u32()?;

        let texture_dimensions = (deser.read_i32()?, deser.read_i32()?, deser.read_i32()?);
        let cell_size = (deser.read_i32()?, deser.read_i32()?);

        let underline = LineDecoration::new(deser.read_f32()?, deser.read_f32()?);
        let strikethrough = LineDecoration::new(deser.read_f32()?, deser.read_f32()?);

        // deserialize the glyphs
        let glyph_count = deser.read_u16()? as usize;
        let mut glyphs = Vec::with_capacity(glyph_count);
        for _ in 0..glyph_count {
            glyphs.push(Glyph::deserialize(deser)?);
        }

        // deserialize texture data
        let packed_texture_data = deser.read_u8_slice()?;
        let texture_data =
            miniz_oxide::inflate::decompress_to_vec(&packed_texture_data).map_err(|_| {
                SerializationError {
                    message: CompactString::const_new("Failed to decompress texture data"),
                }
            })?;

        Ok(FontAtlasData {
            font_name,
            font_size,
            halfwidth_glyphs_per_layer,
            texture_dimensions,
            cell_size,
            underline,
            strikethrough,
            glyphs,
            texture_data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialized_read_u8() {
        let data = [42, 100, 255];
        let mut serialized = Deserializer::new(&data);

        assert_eq!(serialized.read_u8().unwrap(), 42);
        assert_eq!(serialized.read_u8().unwrap(), 100);
        assert_eq!(serialized.read_u8().unwrap(), 255);
    }

    #[test]
    fn test_serialized_read_u8_bounds_error() {
        let data = [42];
        let mut serialized = Deserializer::new(&data);

        // First read should succeed
        assert_eq!(serialized.read_u8().unwrap(), 42);

        // Second read should fail
        let result = serialized.read_u8();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().message, "Out of bounds read");
    }

    #[test]
    fn test_serialized_read_u16() {
        // Little endian: 0x1234 = [0x34, 0x12]
        let data = [0x34, 0x12, 0xFF, 0x00];
        let mut serialized = Deserializer::new(&data);

        assert_eq!(serialized.read_u16().unwrap(), 0x1234);
        assert_eq!(serialized.read_u16().unwrap(), 0x00FF);
    }

    #[test]
    fn test_serialized_read_u16_bounds_error() {
        let data = [0x34]; // Only 1 byte, need 2
        let mut serialized = Deserializer::new(&data);

        let result = serialized.read_u16();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().message, "Out of bounds read");
    }

    #[test]
    fn test_serialized_read_u32() {
        // Little endian: 0x12345678 = [0x78, 0x56, 0x34, 0x12]
        let data = [0x78, 0x56, 0x34, 0x12, 0xFF, 0xFF, 0xFF, 0xFF];
        let mut serialized = Deserializer::new(&data);

        assert_eq!(serialized.read_u32().unwrap(), 0x12345678);
        assert_eq!(serialized.read_u32().unwrap(), 0xFFFFFFFF);
    }

    #[test]
    fn test_serialized_read_u32_bounds_error() {
        let data = [0x78, 0x56, 0x34]; // Only 3 bytes, need 4
        let mut serialized = Deserializer::new(&data);

        let result = serialized.read_u32();
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().message, "Out of bounds read");
    }

    #[test]
    fn test_compact_string_serialize_empty() {
        let s = CompactString::new("");
        let serialized = s.serialize();

        assert_eq!(serialized, vec![0]); // Length 0, no data
    }

    #[test]
    fn test_compact_string_serialize_short() {
        let s = CompactString::from("Hello");
        let serialized = s.serialize();

        let mut expected = vec![5]; // Length 5
        expected.extend(b"Hello");
        assert_eq!(serialized, expected);
    }

    #[test]
    fn test_compact_string_serialize_max_length() {
        let s = CompactString::from("A".repeat(255));
        let serialized = s.serialize();

        assert_eq!(serialized[0], 255); // Length byte
        assert_eq!(serialized.len(), 256); // 1 byte for length + 255 bytes for data
        assert_eq!(&serialized[1..], "A".repeat(255).as_bytes());
    }

    #[test]
    fn test_compact_string_deserialize_empty() {
        let data = [0]; // Length 0
        let mut serialized = Deserializer::new(&data);

        let result = CompactString::deserialize(&mut serialized).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_compact_string_deserialize_short() {
        let mut data = vec![5]; // Length 5
        data.extend(b"Hello");
        let mut serialized = Deserializer::new(&data);

        let result = CompactString::deserialize(&mut serialized).unwrap();
        assert_eq!(result, "Hello");
    }

    #[test]
    fn test_compact_string_round_trip() {
        let aaaaaa = "A".repeat(255);
        let test_cases = ["", "Hello", "World!", "🚀 Unicode works! 🎉", aaaaaa.as_str()];

        for original in &test_cases {
            let compact_str = CompactString::from(*original);
            let serialized = compact_str.serialize();
            let mut serialized_reader = Deserializer::new(&serialized);
            let deserialized = CompactString::deserialize(&mut serialized_reader).unwrap();

            assert_eq!(compact_str, deserialized);
            assert_eq!(*original, deserialized.as_str());
        }
    }

    #[test]
    fn test_serialized_position_tracking() {
        let data = [1, 2, 3, 4, 5, 6, 7, 8];
        let mut serialized = Deserializer::new(&data);

        // Read u8 (position: 0 -> 1)
        assert_eq!(serialized.read_u8().unwrap(), 1);

        // Read u16 (position: 1 -> 3)
        assert_eq!(serialized.read_u16().unwrap(), 0x0302); // little endian [2,3]

        // Read u32 (position: 3 -> 7)
        assert_eq!(serialized.read_u32().unwrap(), 0x07060504); // little endian [4,5,6,7]

        // Read final u8 (position: 7 -> 8)
        assert_eq!(serialized.read_u8().unwrap(), 8);

        // Should be at end now
        assert!(serialized.read_u8().is_err());
    }

    #[test]
    fn test_mixed_serialization() {
        // Test reading different types in sequence
        let mut data = Vec::new();
        data.push(42u8); // u8
        data.extend(&100u16.to_le_bytes()); // u16
        data.extend(&0x12345678u32.to_le_bytes()); // u32
        data.push(5); // string length
        data.extend(b"Hello"); // string data

        let mut serialized = Deserializer::new(&data);

        assert_eq!(serialized.read_u8().unwrap(), 42);
        assert_eq!(serialized.read_u16().unwrap(), 100);
        assert_eq!(serialized.read_u32().unwrap(), 0x12345678);
        assert_eq!(serialized.read_string().unwrap(), "Hello");
    }

    #[test]
    fn test_font_atlas_config_round_trip() {
        // Create test glyphs
        let glyphs = vec![
            Glyph {
                id: 65, // 'A'
                style: FontStyle::Normal,
                symbol: CompactString::from("A"),
                pixel_coords: (0, 0),
                is_emoji: false,
            },
            Glyph {
                id: 66, // 'B'
                style: FontStyle::Normal,
                symbol: CompactString::from("B"),
                pixel_coords: (16, 0),
                is_emoji: false,
            },
            Glyph {
                id: 8364, // '€' (Euro symbol)
                style: FontStyle::Normal,
                symbol: CompactString::from("€"),
                pixel_coords: (32, 0),
                is_emoji: false,
            },
            Glyph {
                id: 10000, // '🚀' (Rocket emoji)
                style: FontStyle::Normal,
                symbol: CompactString::from("🚀"),
                pixel_coords: (48, 0),
                is_emoji: true,
            },
        ];

        // Create original FontAtlasConfig
        let original = FontAtlasData {
            font_name: CompactString::from("TestFont"),
            font_size: 16.5,
            halfwidth_glyphs_per_layer: 328,
            texture_dimensions: (512, 256, 256),
            cell_size: (12, 18),
            underline: LineDecoration::new(0.85, 5.0 / 100.0),
            strikethrough: LineDecoration::new(0.5, 5.0 / 100.0),
            glyphs,
            texture_data: Vec::new(),
        };

        // Serialize
        let serialized = original.serialize();

        // Deserialize
        let mut deserializer = Deserializer::new(&serialized);
        let deserialized = FontAtlasData::deserialize(&mut deserializer).unwrap();

        // Assert all fields match
        assert_eq!(original.font_size, deserialized.font_size);
        assert_eq!(
            original.halfwidth_glyphs_per_layer,
            deserialized.halfwidth_glyphs_per_layer
        );
        assert_eq!(original.texture_dimensions, deserialized.texture_dimensions);
        assert_eq!(original.cell_size, deserialized.cell_size);
        assert_eq!(original.underline, deserialized.underline);
        assert_eq!(original.strikethrough, deserialized.strikethrough);
        assert_eq!(original.glyphs.len(), deserialized.glyphs.len());

        // Assert each glyph matches
        for (orig_glyph, deser_glyph) in original
            .glyphs
            .iter()
            .zip(deserialized.glyphs.iter())
        {
            assert_eq!(orig_glyph.id, deser_glyph.id);
            assert_eq!(orig_glyph.symbol, deser_glyph.symbol);
            assert_eq!(orig_glyph.pixel_coords, deser_glyph.pixel_coords);
        }
    }
}
