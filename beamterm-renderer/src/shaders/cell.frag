#version 300 es

precision mediump float;

// uniforms
uniform mediump sampler2DArray u_sampler;
layout(std140) uniform FragUbo {
    vec2 u_padding_frac;             // padding as fraction of cell size
    float u_underline_pos;           // underline position (0.0 = top, 1.0 = bottom)
    float u_underline_thickness;     // underline thickness as fraction of cell height
    float u_strikethrough_pos;       // strikethrough position (0.0 = top, 1.0 = bottom)
    float u_strikethrough_thickness; // strikethrough thickness as fraction of cell height
};


// packs 8b: 2b glyph id, 3b fg.rgb, 3b bg.rgb
// ref: https://github.com/junkdog/term-webgl2?tab=readme-ov-file#glyph-id-bit-layout-16-bit
flat in uvec2 v_packed_data;
in vec2 v_tex_coord;

out vec4 FragColor;

float horizontal_line(vec2 tex_coord, float center, float thickness) {
    return 1.0 - smoothstep(0.0, thickness, abs(tex_coord.y - center));
}

float normalize_lsb(uint value) {
    return (float(value & 0xFFu)) * 0.003921568627451; // = 1.0 / 255.0;
}

void main() {
    // extract sequential glyph index from packed data
    uint glyph_index = v_packed_data.x & 0xFFFFu;

    // texture position from sequential index (32 glyphs per layer)
    uint layer = (glyph_index & 0x1FFFu) >> 5; // only keep layer-coding bits
    uint pos_in_layer = glyph_index & 0x1Fu;

    // apply strikethrough or underline if the glyph has either bit set
    // (it's easier to do this before we recalculate the tex_coord)
    float line_alpha = max(
        horizontal_line(v_tex_coord, u_underline_pos, u_underline_thickness) * float((glyph_index >> 13) & 0x1u),
        horizontal_line(v_tex_coord, u_strikethrough_pos, u_strikethrough_thickness) * float((glyph_index >> 14) & 0x1u)
    );

    vec2 inner_tex_coord = v_tex_coord * (1.0 - 2.0 * u_padding_frac) + u_padding_frac;
    vec3 tex_coord = vec3(
        inner_tex_coord.x + 0.001,
        (float(pos_in_layer) + inner_tex_coord.y + 0.001) * 0.03125, // 0.03125 = 1/32
        float(layer)
    );

    // the base foreground color is used for normal glyphs and underlines/strikethroughs
    vec3 base_fg = vec3(
        normalize_lsb(v_packed_data.x >> 16),
        normalize_lsb(v_packed_data.x >> 24),
        normalize_lsb(v_packed_data.y)
    );

    vec4 glyph = texture(u_sampler, tex_coord);

    // 0.0 for normal glyphs, 1.0 for emojis: used for determining color source
    float emoji_factor = float((glyph_index >> 12) & 0x1u);

    // color for normal glyphs are taken from the packed data;
    // emoji colors are sampled from the texture directly
    vec3 fg = mix(base_fg, glyph.rgb, emoji_factor);

    // if we're drawing a line, blend it with the base foreground color.
    // this allows us to do strikethroughs and underlines on emojis with
    // the same color as the base foreground.
    fg = mix(fg, base_fg, line_alpha);

    // make sure to set the alpha when drawing a line
    float a = max(glyph.a, line_alpha);

    vec3 bg = vec3(
        normalize_lsb(v_packed_data.y >> 8),
        normalize_lsb(v_packed_data.y >> 16),
        normalize_lsb(v_packed_data.y >> 24)
    );

    FragColor = vec4(mix(bg, fg, a), 1.0);
}