#version 300 es
// use highp to avoid possible ANGLE precision issues causing gaps between cells
precision highp float;

// cell geometry attributes
layout(location = 0) in vec2 a_pos;
layout(location = 1) in vec2 a_tex_coord;

// instance attributes
layout(location = 2) in uvec2 a_instance_pos;
layout(location = 3) in uvec2 a_packed_data; // glyph_id + fg RGB
layout(location = 4) in uvec2 a_packed_bg;   // bg RGB

// uniforms
layout(std140) uniform VertUbo {
    mat4 u_projection;
    vec2 u_cell_size; // unpadded cell size in pixels
};

// pass glyph index and pre-extracted colors to fragment shader
flat out uint v_glyph_index;
flat out vec3 v_fg_color;
flat out vec3 v_bg_color;
out vec2 v_tex_coord;

// extract byte at position (0=low, 3=high)
float extract_byte(uint value, uint byte_pos) {
    uint mask = 0xFFu << (byte_pos * 8u);
    uint masked = value & mask;
    return float(masked >> (byte_pos * 8u)) / 255.0;
}

void main() {
    v_tex_coord = a_tex_coord;
    // glyph_id is now full u32 in packed_data.x
    v_glyph_index = a_packed_data.x;

    // extract colors in vertex shader to avoid ANGLE fragment shader bugs
    // Layout:
    // a_packed_data.x: glyph_id (4 bytes)
    // a_packed_data.y: fg R (byte 0), fg G (byte 1), fg B (byte 2), padding (byte 3)
    // a_packed_bg.x: bg R (byte 0), bg G (byte 1), bg B (byte 2), padding (byte 3)
    v_fg_color = vec3(
        extract_byte(a_packed_data.y, 0u), // fg R
        extract_byte(a_packed_data.y, 1u), // fg G
        extract_byte(a_packed_data.y, 2u)  // fg B
    );
    v_bg_color = vec3(
        extract_byte(a_packed_bg.x, 0u), // bg R
        extract_byte(a_packed_bg.x, 1u), // bg G
        extract_byte(a_packed_bg.x, 2u)  // bg B
    );

    vec2 offset = vec2(
        floor(float(a_instance_pos.x) * u_cell_size.x + 0.5), // pixel-snapped
        floor(float(a_instance_pos.y) * u_cell_size.y + 0.5)  // pixel-snapped
    );

    gl_Position = u_projection * vec4(a_pos + offset, 0.0, 1.0);
}