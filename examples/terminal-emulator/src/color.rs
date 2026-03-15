//  color palette (dracula)

pub const DEFAULT_FG: u32 = 0x00_f8_f8_f2;
pub const DEFAULT_BG: u32 = 0x00_28_2a_36;

#[rustfmt::skip]
const ANSI_COLORS: [u32; 16] = [
    0x21_22_2c, // 0  black
    0xff_55_55, // 1  red
    0x50_fa_7b, // 2  green
    0xf1_fa_8c, // 3  yellow
    0xbd_93_f9, // 4  blue
    0xff_79_c6, // 5  magenta
    0x8b_e9_fd, // 6  cyan
    0xf8_f8_f2, // 7  white
    0x62_72_a4, // 8  bright black
    0xff_6e_6e, // 9  bright red
    0x69_ff_94, // 10 bright green
    0xff_ff_a5, // 11 bright yellow
    0xd6_ac_ff, // 12 bright blue
    0xff_92_df, // 13 bright magenta
    0xa4_ff_ff, // 14 bright cyan
    0xff_ff_ff, // 15 bright white
];

const COLOR_CUBE_VALUES: [u8; 6] = [0x00, 0x5f, 0x87, 0xaf, 0xd7, 0xff];

pub fn color_to_rgb(color: vt100::Color, default: u32) -> u32 {
    match color {
        vt100::Color::Default => default,
        vt100::Color::Idx(i) => match i {
            0..=15 => ANSI_COLORS[i as usize],
            16..=231 => {
                let i = i - 16;
                let r = COLOR_CUBE_VALUES[(i / 36) as usize] as u32;
                let g = COLOR_CUBE_VALUES[((i / 6) % 6) as usize] as u32;
                let b = COLOR_CUBE_VALUES[(i % 6) as usize] as u32;
                (r << 16) | (g << 8) | b
            },
            232..=255 => {
                let v = 8 + 10 * (i - 232) as u32;
                (v << 16) | (v << 8) | v
            },
        },
        vt100::Color::Rgb(r, g, b) => ((r as u32) << 16) | ((g as u32) << 8) | b as u32,
    }
}

pub fn dim_color(c: u32) -> u32 {
    let r = ((c >> 16) & 0xff) / 2;
    let g = ((c >> 8) & 0xff) / 2;
    let b = (c & 0xff) / 2;
    (r << 16) | (g << 8) | b
}
