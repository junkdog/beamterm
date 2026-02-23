//! Quake-style drop-down game console with a 3D spinning cube.
//!
//! Demonstrates beamterm-core as a semi-transparent overlay on top of
//! existing OpenGL rendering — the way an in-game debug console would work.
//!
//! Run with:
//! ```sh
//! cargo run -p game-console
//! ```

use std::{num::NonZeroU32, time::Instant};

use beamterm_core::{
    CellData, FontAtlasData, FontStyle, GlState, GlslVersion, GlyphEffect, StaticFontAtlas,
    TerminalGrid,
};
use glow::HasContext;
use glutin::surface::GlSurface;
use winit::{
    application::ApplicationHandler,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, EventLoop},
    keyboard::{Key, NamedKey},
    window::WindowId,
};

// ── color scheme ─────────────────────────────────────────────────────

const BG: u32 = 0x00_0a_14_1e; // dark blue-black
const BORDER: u32 = 0x00_2a_4a_3a; // muted green
const TITLE: u32 = 0x00_4e_d4_8a; // bright green
const PROMPT: u32 = 0x00_4e_d4_8a; // bright green
const TIMESTAMP: u32 = 0x00_2e_7a_5a; // dim green
const DEBUG_FG: u32 = 0x00_6a_6a_6a; // gray
const INFO_FG: u32 = 0x00_5c_9e_d6; // blue
const WARN_FG: u32 = 0x00_d4_a0_4e; // amber
const ERROR_FG: u32 = 0x00_d4_4e_4e; // red
const TEXT_FG: u32 = 0x00_b8_c8_b8; // light gray-green
const INPUT_FG: u32 = 0x00_e0_f0_e0; // bright text
const CURSOR_FG: u32 = 0x00_4e_d4_8a; // bright green

const CONSOLE_RATIO: f32 = 0.55; // console occupies bottom 55% of window
const BG_ALPHA: f32 = 0.75; // console background transparency

// ── log data ─────────────────────────────────────────────────────────

#[derive(Clone, Copy)]
enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    fn label(self) -> &'static str {
        match self {
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO ",
            LogLevel::Warn => "WARN ",
            LogLevel::Error => "ERROR",
        }
    }

    fn color(self) -> u32 {
        match self {
            LogLevel::Debug => DEBUG_FG,
            LogLevel::Info => INFO_FG,
            LogLevel::Warn => WARN_FG,
            LogLevel::Error => ERROR_FG,
        }
    }
}

struct LogEntry {
    level: LogLevel,
    timestamp: String,
    message: String,
}

// ── cube state ───────────────────────────────────────────────────────

struct CubeState {
    speed: f32,
    color: [f32; 3],
    wireframe: bool,
}

impl Default for CubeState {
    fn default() -> Self {
        Self {
            speed: 1.0,
            color: [0.2, 0.8, 0.6], // teal
            wireframe: false,
        }
    }
}

const NAMED_COLORS: &[(&str, [f32; 3])] = &[
    ("red", [0.9, 0.2, 0.2]),
    ("green", [0.2, 0.9, 0.3]),
    ("blue", [0.3, 0.4, 0.9]),
    ("cyan", [0.2, 0.8, 0.8]),
    ("magenta", [0.8, 0.2, 0.8]),
    ("yellow", [0.9, 0.9, 0.2]),
    ("orange", [0.9, 0.5, 0.1]),
    ("white", [0.9, 0.9, 0.9]),
    ("teal", [0.2, 0.8, 0.6]),
];

// ── game console state ───────────────────────────────────────────────

struct GameConsole {
    log: Vec<LogEntry>,
    input: String,
    start_time: Instant,
    cursor_visible: bool,
    last_cursor_toggle: Instant,
}

impl GameConsole {
    fn new() -> Self {
        let now = Instant::now();
        let mut console = Self {
            log: Vec::new(),
            input: String::new(),
            start_time: now,
            cursor_visible: true,
            last_cursor_toggle: now,
        };
        console.push_log(LogLevel::Info, "Ready. Type 'help' for commands.".into());
        console
    }

    fn elapsed_secs(&self) -> f32 {
        self.start_time.elapsed().as_secs_f32()
    }

    fn format_timestamp(&self) -> String {
        let total = self.start_time.elapsed().as_secs();
        let h = total / 3600;
        let m = (total % 3600) / 60;
        let s = total % 60;
        format!("{h:02}:{m:02}:{s:02}")
    }

    fn push_log(&mut self, level: LogLevel, message: String) {
        let timestamp = self.format_timestamp();
        self.log
            .push(LogEntry { level, timestamp, message });
    }

    fn tick(&mut self) -> bool {
        // blink cursor every 500ms
        let now = Instant::now();
        if now
            .duration_since(self.last_cursor_toggle)
            .as_millis()
            >= 500
        {
            self.cursor_visible = !self.cursor_visible;
            self.last_cursor_toggle = now;
            return true;
        }

        false
    }

    fn reset_cursor(&mut self) {
        self.cursor_visible = true;
        self.last_cursor_toggle = Instant::now();
    }

    fn submit_input(&mut self, cube: &mut CubeState) {
        if !self.input.is_empty() {
            let cmd = self.input.clone();
            self.push_log(LogLevel::Info, format!("> {cmd}"));

            match cmd.trim() {
                "help" => {
                    self.push_log(LogLevel::Info, "Available commands:".into());
                    self.push_log(LogLevel::Info, "  help, status, clear".into());
                    self.push_log(LogLevel::Info, "  reverse   - reverse rotation".into());
                    self.push_log(LogLevel::Info, "  speed N   - set speed (0.1-5.0)".into());
                    self.push_log(
                        LogLevel::Info,
                        "  color X   - red/green/blue/cyan/magenta/...".into(),
                    );
                    self.push_log(LogLevel::Info, "  wireframe - toggle wireframe".into());
                    self.push_log(LogLevel::Info, "  reset     - reset cube".into());
                },
                "status" => {
                    let secs = self.elapsed_secs();
                    self.push_log(
                        LogLevel::Info,
                        format!("Uptime: {secs:.1}s | Log entries: {}", self.log.len() + 1),
                    );
                    self.push_log(
                        LogLevel::Debug,
                        format!(
                            "Cube: speed={:.1} wireframe={} color=[{:.1},{:.1},{:.1}]",
                            cube.speed, cube.wireframe, cube.color[0], cube.color[1], cube.color[2]
                        ),
                    );
                },
                "clear" => {
                    self.log.clear();
                },
                "quit" | "exit" => {
                    self.push_log(LogLevel::Warn, "Use Escape to exit.".into());
                },
                "reverse" => {
                    cube.speed = -cube.speed;
                    self.push_log(LogLevel::Info, "Rotation reversed.".into());
                },
                "wireframe" => {
                    cube.wireframe = !cube.wireframe;
                    let state = if cube.wireframe { "on" } else { "off" };
                    self.push_log(LogLevel::Info, format!("Wireframe: {state}"));
                },
                "reset" => {
                    *cube = CubeState::default();
                    self.push_log(LogLevel::Info, "Cube reset to defaults.".into());
                },
                s if s.starts_with("speed ") => {
                    let val = s[6..].trim();
                    match val.parse::<f32>() {
                        Ok(v) if (0.1..=5.0).contains(&v) => {
                            let sign = if cube.speed < 0.0 { -1.0 } else { 1.0 };
                            cube.speed = v * sign;
                            self.push_log(LogLevel::Info, format!("Speed set to {v:.1}"));
                        },
                        Ok(_) => {
                            self.push_log(LogLevel::Warn, "Speed must be 0.1-5.0.".into());
                        },
                        Err(_) => {
                            self.push_log(LogLevel::Error, format!("Invalid number: '{val}'"));
                        },
                    }
                },
                s if s.starts_with("color ") => {
                    let name = s[6..].trim();
                    if name == "random" {
                        let ns = self.start_time.elapsed().subsec_nanos() as usize;
                        let c = NAMED_COLORS[ns % NAMED_COLORS.len()].1;
                        cube.color = c;
                        self.push_log(LogLevel::Info, "Random color applied.".into());
                    } else if let Some((_, c)) = NAMED_COLORS.iter().find(|(n, _)| *n == name) {
                        cube.color = *c;
                        self.push_log(LogLevel::Info, format!("Color set to {name}."));
                    } else {
                        let names: Vec<&str> = NAMED_COLORS.iter().map(|(n, _)| *n).collect();
                        self.push_log(
                            LogLevel::Warn,
                            format!("Unknown color. Try: {}, random", names.join(", ")),
                        );
                    }
                },
                _ => {
                    self.push_log(LogLevel::Warn, format!("Unknown command: '{cmd}'"));
                },
            }

            self.input.clear();
            self.cursor_visible = true;
            self.last_cursor_toggle = Instant::now();
        }
    }
}

// ── 3D cube renderer ─────────────────────────────────────────────────

const CUBE_VERT: &str = r#"#version 330 core
layout(location = 0) in vec3 a_pos;
layout(location = 1) in vec3 a_normal;
uniform mat4 u_mvp;
uniform mat4 u_model;
out vec3 v_normal;
void main() {
    v_normal = mat3(u_model) * a_normal;
    gl_Position = u_mvp * vec4(a_pos, 1.0);
}
"#;

const CUBE_FRAG: &str = r#"#version 330 core
in vec3 v_normal;
uniform vec3 u_color;
out vec4 FragColor;
void main() {
    vec3 n = normalize(v_normal);
    vec3 light_dir = normalize(vec3(0.5, 1.0, 0.8));
    float diffuse = max(dot(n, light_dir), 0.0);
    vec3 color = u_color * (0.15 + 0.85 * diffuse);
    FragColor = vec4(color, 1.0);
}
"#;

// 36 vertices: 6 faces * 2 triangles * 3 verts, each with (pos.xyz, normal.xyz)
#[rustfmt::skip]
const CUBE_VERTICES: &[f32] = &[
    // +Z face
    -0.5, -0.5,  0.5,   0.0,  0.0,  1.0,
     0.5, -0.5,  0.5,   0.0,  0.0,  1.0,
     0.5,  0.5,  0.5,   0.0,  0.0,  1.0,
    -0.5, -0.5,  0.5,   0.0,  0.0,  1.0,
     0.5,  0.5,  0.5,   0.0,  0.0,  1.0,
    -0.5,  0.5,  0.5,   0.0,  0.0,  1.0,
    // -Z face
     0.5, -0.5, -0.5,   0.0,  0.0, -1.0,
    -0.5, -0.5, -0.5,   0.0,  0.0, -1.0,
    -0.5,  0.5, -0.5,   0.0,  0.0, -1.0,
     0.5, -0.5, -0.5,   0.0,  0.0, -1.0,
    -0.5,  0.5, -0.5,   0.0,  0.0, -1.0,
     0.5,  0.5, -0.5,   0.0,  0.0, -1.0,
    // +Y face
    -0.5,  0.5,  0.5,   0.0,  1.0,  0.0,
     0.5,  0.5,  0.5,   0.0,  1.0,  0.0,
     0.5,  0.5, -0.5,   0.0,  1.0,  0.0,
    -0.5,  0.5,  0.5,   0.0,  1.0,  0.0,
     0.5,  0.5, -0.5,   0.0,  1.0,  0.0,
    -0.5,  0.5, -0.5,   0.0,  1.0,  0.0,
    // -Y face
    -0.5, -0.5, -0.5,   0.0, -1.0,  0.0,
     0.5, -0.5, -0.5,   0.0, -1.0,  0.0,
     0.5, -0.5,  0.5,   0.0, -1.0,  0.0,
    -0.5, -0.5, -0.5,   0.0, -1.0,  0.0,
     0.5, -0.5,  0.5,   0.0, -1.0,  0.0,
    -0.5, -0.5,  0.5,   0.0, -1.0,  0.0,
    // +X face
     0.5, -0.5,  0.5,   1.0,  0.0,  0.0,
     0.5, -0.5, -0.5,   1.0,  0.0,  0.0,
     0.5,  0.5, -0.5,   1.0,  0.0,  0.0,
     0.5, -0.5,  0.5,   1.0,  0.0,  0.0,
     0.5,  0.5, -0.5,   1.0,  0.0,  0.0,
     0.5,  0.5,  0.5,   1.0,  0.0,  0.0,
    // -X face
    -0.5, -0.5, -0.5,  -1.0,  0.0,  0.0,
    -0.5, -0.5,  0.5,  -1.0,  0.0,  0.0,
    -0.5,  0.5,  0.5,  -1.0,  0.0,  0.0,
    -0.5, -0.5, -0.5,  -1.0,  0.0,  0.0,
    -0.5,  0.5,  0.5,  -1.0,  0.0,  0.0,
    -0.5,  0.5, -0.5,  -1.0,  0.0,  0.0,
];

struct CubeRenderer {
    program: glow::Program,
    vao: glow::VertexArray,
    _vbo: glow::Buffer,
    mvp_loc: glow::UniformLocation,
    model_loc: glow::UniformLocation,
    color_loc: glow::UniformLocation,
}

impl CubeRenderer {
    fn new(gl: &glow::Context) -> Self {
        unsafe {
            let program = create_shader_program(gl, CUBE_VERT, CUBE_FRAG);

            let mvp_loc = gl
                .get_uniform_location(program, "u_mvp")
                .expect("u_mvp not found");
            let model_loc = gl
                .get_uniform_location(program, "u_model")
                .expect("u_model not found");
            let color_loc = gl
                .get_uniform_location(program, "u_color")
                .expect("u_color not found");

            let vao = gl
                .create_vertex_array()
                .expect("failed to create cube VAO");
            gl.bind_vertex_array(Some(vao));

            let vbo = gl
                .create_buffer()
                .expect("failed to create cube VBO");
            gl.bind_buffer(glow::ARRAY_BUFFER, Some(vbo));
            let bytes: &[u8] = core::slice::from_raw_parts(
                CUBE_VERTICES.as_ptr() as *const u8,
                CUBE_VERTICES.len() * 4,
            );
            gl.buffer_data_u8_slice(glow::ARRAY_BUFFER, bytes, glow::STATIC_DRAW);

            let stride = 6 * 4; // 6 floats * 4 bytes
            gl.enable_vertex_attrib_array(0);
            gl.vertex_attrib_pointer_f32(0, 3, glow::FLOAT, false, stride, 0);
            gl.enable_vertex_attrib_array(1);
            gl.vertex_attrib_pointer_f32(1, 3, glow::FLOAT, false, stride, 3 * 4);

            gl.bind_vertex_array(None);

            Self {
                program,
                vao,
                _vbo: vbo,
                mvp_loc,
                model_loc,
                color_loc,
            }
        }
    }

    fn render(&self, gl: &glow::Context, elapsed: f32, aspect: f32, state: &CubeState) {
        unsafe {
            gl.enable(glow::DEPTH_TEST);
            gl.enable(glow::CULL_FACE);
            gl.cull_face(glow::BACK);
            gl.use_program(Some(self.program));

            let angle_y = elapsed * state.speed;
            let angle_x = elapsed * state.speed * 0.4;

            let rot_y = mat4_rotate_y(angle_y);
            let rot_x = mat4_rotate_x(angle_x);
            let model = mat4_mul(&rot_x, &rot_y);

            // shift the cube up so it's visible above the console
            let view = mat4_translate(0.0, 0.15, -3.0);
            let proj = mat4_perspective(45.0_f32.to_radians(), aspect, 0.1, 100.0);
            let mv = mat4_mul(&view, &model);
            let mvp = mat4_mul(&proj, &mv);

            gl.uniform_matrix_4_f32_slice(Some(&self.mvp_loc), false, &mvp);
            gl.uniform_matrix_4_f32_slice(Some(&self.model_loc), false, &model);
            gl.uniform_3_f32(
                Some(&self.color_loc),
                state.color[0],
                state.color[1],
                state.color[2],
            );

            gl.bind_vertex_array(Some(self.vao));

            if state.wireframe {
                gl.polygon_mode(glow::FRONT_AND_BACK, glow::LINE);
                gl.line_width(2.0);
            }

            gl.draw_arrays(glow::TRIANGLES, 0, 36);

            if state.wireframe {
                gl.polygon_mode(glow::FRONT_AND_BACK, glow::FILL);
            }

            gl.bind_vertex_array(None);
            gl.use_program(None);
            gl.disable(glow::CULL_FACE);
            gl.disable(glow::DEPTH_TEST);
        }
    }
}

unsafe fn create_shader_program(
    gl: &glow::Context,
    vert_src: &str,
    frag_src: &str,
) -> glow::Program {
    unsafe {
        let vert = gl.create_shader(glow::VERTEX_SHADER).unwrap();
        gl.shader_source(vert, vert_src);
        gl.compile_shader(vert);
        assert!(
            gl.get_shader_compile_status(vert),
            "Vertex: {}",
            gl.get_shader_info_log(vert)
        );

        let frag = gl.create_shader(glow::FRAGMENT_SHADER).unwrap();
        gl.shader_source(frag, frag_src);
        gl.compile_shader(frag);
        assert!(
            gl.get_shader_compile_status(frag),
            "Fragment: {}",
            gl.get_shader_info_log(frag)
        );

        let program = gl.create_program().unwrap();
        gl.attach_shader(program, vert);
        gl.attach_shader(program, frag);
        gl.link_program(program);
        assert!(
            gl.get_program_link_status(program),
            "Link: {}",
            gl.get_program_info_log(program)
        );

        gl.detach_shader(program, vert);
        gl.detach_shader(program, frag);
        gl.delete_shader(vert);
        gl.delete_shader(frag);

        program
    }
}

// ── matrix math (column-major) ───────────────────────────────────────

fn mat4_perspective(fov_y: f32, aspect: f32, near: f32, far: f32) -> [f32; 16] {
    let f = 1.0 / (fov_y / 2.0).tan();
    let nf = near - far;
    [
        f / aspect,
        0.0,
        0.0,
        0.0,
        0.0,
        f,
        0.0,
        0.0,
        0.0,
        0.0,
        (far + near) / nf,
        -1.0,
        0.0,
        0.0,
        (2.0 * far * near) / nf,
        0.0,
    ]
}

fn mat4_translate(x: f32, y: f32, z: f32) -> [f32; 16] {
    [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, x, y, z, 1.0]
}

fn mat4_rotate_x(angle: f32) -> [f32; 16] {
    let (s, c) = angle.sin_cos();
    [1.0, 0.0, 0.0, 0.0, 0.0, c, s, 0.0, 0.0, -s, c, 0.0, 0.0, 0.0, 0.0, 1.0]
}

fn mat4_rotate_y(angle: f32) -> [f32; 16] {
    let (s, c) = angle.sin_cos();
    [c, 0.0, -s, 0.0, 0.0, 1.0, 0.0, 0.0, s, 0.0, c, 0.0, 0.0, 0.0, 0.0, 1.0]
}

fn mat4_mul(a: &[f32; 16], b: &[f32; 16]) -> [f32; 16] {
    let mut out = [0.0_f32; 16];
    for col in 0..4 {
        for row in 0..4 {
            out[col * 4 + row] = (0..4)
                .map(|k| a[k * 4 + row] * b[col * 4 + k])
                .sum();
        }
    }
    out
}

// ── console rendering ────────────────────────────────────────────────

fn render_console(grid: &mut TerminalGrid, console: &GameConsole) {
    let (cols, rows) = grid.terminal_size();
    let cols = cols as usize;
    let rows = rows as usize;

    if cols < 4 || rows < 5 {
        return;
    }

    grid.update_cells((0..rows).flat_map(|row| {
        let console_ref = &console;
        (0..cols).map(move |col| render_cell(row, col, cols, rows, console_ref))
    }))
    .expect("failed to update cells");
}

fn render_cell(
    row: usize,
    col: usize,
    cols: usize,
    rows: usize,
    console: &GameConsole,
) -> CellData<'_> {
    let last_row = rows - 1;
    let separator_row = rows - 3;
    let input_row = rows - 2;

    match row {
        0 => render_top_border(col, cols),
        r if r == last_row => hline(col, cols, "└", "┘"),
        r if r == separator_row => hline(col, cols, "├", "┤"),
        r if r == input_row => render_input_line(col, cols, console),
        _ => render_log_line(row, col, cols, rows, console),
    }
}

// ── border rendering ─────────────────────────────────────────────────

fn hline(col: usize, cols: usize, left: &'static str, right: &'static str) -> CellData<'static> {
    match col {
        0 => cell(left, BORDER, BG),
        c if c == cols - 1 => cell(right, BORDER, BG),
        _ => cell("─", BORDER, BG),
    }
}

fn render_top_border(col: usize, cols: usize) -> CellData<'static> {
    let title = " GAME CONSOLE ";
    let title_start = (cols.saturating_sub(title.len())) / 2;

    if let Some(i) = col.checked_sub(title_start)
        && i < title.len()
    {
        return char_cell(title, i, FontStyle::Bold, GlyphEffect::None, TITLE, BG);
    }
    hline(col, cols, "┌", "┐")
}

// ── input line ───────────────────────────────────────────────────────

fn render_input_line<'a>(col: usize, cols: usize, console: &'a GameConsole) -> CellData<'a> {
    if col == 0 || col == cols - 1 {
        return cell("│", BORDER, BG);
    }

    let inner = col - 1;
    let prompt_len = 2; // "> "
    let input_end = prompt_len + console.input.len();

    match inner {
        i if i < prompt_len => text_or_space("> ", i, FontStyle::Bold, PROMPT),
        i if i < input_end => {
            text_or_space(&console.input, i - prompt_len, FontStyle::Normal, INPUT_FG)
        },
        i if i == input_end && console.cursor_visible => cell("█", CURSOR_FG, BG),
        _ => space(),
    }
}

// ── log area ─────────────────────────────────────────────────────────

fn render_log_line<'a>(
    row: usize,
    col: usize,
    cols: usize,
    rows: usize,
    console: &'a GameConsole,
) -> CellData<'a> {
    if col == 0 || col == cols - 1 {
        return cell("│", BORDER, BG);
    }

    let log_area_height = rows - 5;
    let visible_start = console.log.len().saturating_sub(log_area_height);
    let log_index = visible_start + (row - 1);
    if log_index >= console.log.len() {
        return space();
    }

    let entry = &console.log[log_index];
    let inner = col - 1;

    // Layout: [HH:MM:SS] LEVEL message
    //          0123456789...
    match inner {
        0 => cell("[", TIMESTAMP, BG),
        1..=8 => text_or_space(&entry.timestamp, inner - 1, FontStyle::Normal, TIMESTAMP),
        9 => cell("]", TIMESTAMP, BG),
        11..=15 => text_or_space(
            entry.level.label(),
            inner - 11,
            FontStyle::Bold,
            entry.level.color(),
        ),
        17.. => text_or_space(&entry.message, inner - 17, FontStyle::Normal, TEXT_FG),
        _ => space(), // gaps at columns 10, 16
    }
}

// ── cell helpers ─────────────────────────────────────────────────────

fn cell(ch: &'static str, fg: u32, bg: u32) -> CellData<'static> {
    CellData::new(ch, FontStyle::Normal, GlyphEffect::None, fg, bg)
}

fn char_cell(
    text: &str,
    index: usize,
    style: FontStyle,
    effect: GlyphEffect,
    fg: u32,
    bg: u32,
) -> CellData<'_> {
    CellData::new(&text[index..index + 1], style, effect, fg, bg)
}

fn space() -> CellData<'static> {
    CellData::new(" ", FontStyle::Normal, GlyphEffect::None, TEXT_FG, BG)
}

fn text_or_space(text: &str, index: usize, style: FontStyle, fg: u32) -> CellData<'_> {
    if index < text.len() {
        char_cell(text, index, style, GlyphEffect::None, fg, BG)
    } else {
        space()
    }
}

// ── application ──────────────────────────────────────────────────────

fn main() {
    let event_loop = EventLoop::new().expect("failed to create event loop");
    let mut app = App::default();
    event_loop
        .run_app(&mut app)
        .expect("event loop failed");
}

#[derive(Default)]
struct App {
    state: Option<AppState>,
}

struct AppState {
    win: GlWindow,
    gl_state: GlState,
    grid: TerminalGrid,
    console: GameConsole,
    cube_renderer: CubeRenderer,
    cube_state: CubeState,
    window_size: (u32, u32),
}

fn console_height(window_height: u32) -> u32 {
    (window_height as f32 * CONSOLE_RATIO) as u32
}

impl AppState {
    fn refresh_console(&mut self) {
        render_console(&mut self.grid, &self.console);
        self.grid
            .flush_cells(&self.win.gl)
            .expect("failed to flush cells");
        self.win.window.request_redraw();
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let init_start = Instant::now();

        let win = GlWindow::new(event_loop, "beamterm - game console", (960, 600));
        let gl_state = GlState::new(&win.gl);

        let phys = win.physical_size();
        let con_h = console_height(phys.1 as u32);

        let atlas_data = FontAtlasData::default();
        let atlas = StaticFontAtlas::load(&win.gl, atlas_data).expect("failed to load font atlas");

        let mut grid = TerminalGrid::new(
            &win.gl,
            atlas.into(),
            (phys.0, con_h as i32),
            win.pixel_ratio(),
            &GlslVersion::Gl330,
        )
        .expect("failed to create terminal grid");

        grid.set_bg_alpha(&win.gl, BG_ALPHA);

        let cube_renderer = CubeRenderer::new(&win.gl);

        let init_ms = init_start.elapsed().as_secs_f64() * 1000.0;
        let (cols, rows) = grid.terminal_size();

        let mut console = GameConsole::new();
        console.push_log(
            LogLevel::Info,
            format!("OpenGL 3.3 | {cols}x{rows} cells | init: {init_ms:.1}ms"),
        );

        self.state = Some(AppState {
            win,
            gl_state,
            grid,
            console,
            cube_renderer,
            cube_state: CubeState::default(),
            window_size: (phys.0 as u32, phys.1 as u32),
        });

        self.state.as_mut().unwrap().refresh_console();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(state) = self.state.as_mut() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
            },
            WindowEvent::Resized(new_size) => {
                if new_size.width > 0 && new_size.height > 0 {
                    state.win.resize_surface(new_size);
                    state.window_size = (new_size.width, new_size.height);

                    let con_h = console_height(new_size.height);
                    let _ = state.grid.resize(
                        &state.win.gl,
                        (new_size.width as i32, con_h as i32),
                        state.win.pixel_ratio(),
                    );

                    state.refresh_console();
                }
            },
            WindowEvent::KeyboardInput { event, .. } => {
                if !event.state.is_pressed() {
                    return;
                }

                let dirty = match &event.logical_key {
                    Key::Named(NamedKey::Escape) => {
                        event_loop.exit();
                        false
                    },
                    Key::Named(NamedKey::Enter) => {
                        state.console.submit_input(&mut state.cube_state);
                        true
                    },
                    Key::Named(NamedKey::Space) => {
                        state.console.input.push(' ');
                        state.console.reset_cursor();
                        true
                    },
                    Key::Named(NamedKey::Backspace) => {
                        state.console.input.pop();
                        state.console.reset_cursor();
                        true
                    },
                    Key::Character(ch) => {
                        for c in ch.chars() {
                            if c.is_ascii_graphic() || c == ' ' {
                                state.console.input.push(c);
                            }
                        }
                        state.console.reset_cursor();
                        true
                    },
                    _ => false,
                };
                if dirty {
                    state.refresh_console();
                }
            },
            WindowEvent::RedrawRequested => {
                if state.console.tick() {
                    render_console(&mut state.grid, &state.console);
                    state
                        .grid
                        .flush_cells(&state.win.gl)
                        .expect("failed to flush cells");
                }

                let (win_w, win_h) = state.window_size;
                let elapsed = state.console.elapsed_secs();

                // --- clear full window ---
                state
                    .gl_state
                    .viewport(&state.win.gl, 0, 0, win_w as i32, win_h as i32);
                state
                    .gl_state
                    .clear_color(&state.win.gl, 0.04, 0.06, 0.10, 1.0);
                unsafe {
                    state
                        .win
                        .gl
                        .clear(glow::COLOR_BUFFER_BIT | glow::DEPTH_BUFFER_BIT);
                }

                // --- render 3D cube (full viewport) ---
                let aspect = win_w as f32 / win_h.max(1) as f32;
                state
                    .cube_renderer
                    .render(&state.win.gl, elapsed, aspect, &state.cube_state);

                // --- render terminal overlay (bottom portion) ---
                let (grid_w, grid_h) = state.grid.canvas_size();
                state
                    .gl_state
                    .viewport(&state.win.gl, 0, 0, grid_w, grid_h);

                unsafe {
                    state.win.gl.enable(glow::BLEND);
                    state
                        .win
                        .gl
                        .blend_func(glow::SRC_ALPHA, glow::ONE_MINUS_SRC_ALPHA);
                }

                state
                    .grid
                    .render(&state.win.gl, &mut state.gl_state)
                    .expect("failed to render grid");

                unsafe {
                    state.win.gl.disable(glow::BLEND);
                }

                state.win.swap_buffers();
            },
            _ => {},
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        if let Some(state) = self.state.as_ref() {
            state.win.window.request_redraw();
        }
    }
}

// ── glutin / winit boilerplate ───────────────────────────────────────

use glutin::{
    config::{ConfigTemplateBuilder, GlConfig},
    context::{
        ContextApi, ContextAttributesBuilder, NotCurrentGlContext, PossiblyCurrentContext, Version,
    },
    display::{GetGlDisplay, GlDisplay},
    surface::{Surface, SwapInterval, WindowSurface},
};
use glutin_winit::DisplayBuilder;
use raw_window_handle::HasWindowHandle;
use winit::{
    dpi::LogicalSize,
    window::{Window, WindowAttributes},
};

struct GlWindow {
    window: Window,
    gl_context: PossiblyCurrentContext,
    gl_surface: Surface<WindowSurface>,
    gl: glow::Context,
}

impl GlWindow {
    fn new(event_loop: &ActiveEventLoop, title: &str, size: (u32, u32)) -> Self {
        let window_attrs = WindowAttributes::default()
            .with_title(title)
            .with_inner_size(LogicalSize::new(size.0, size.1));

        let config_template = ConfigTemplateBuilder::new().with_alpha_size(8);

        let (window, gl_config) =
            DisplayBuilder::new()
                .with_window_attributes(Some(window_attrs))
                .build(event_loop, config_template, |configs| {
                    configs
                        .reduce(|accum, config| {
                            if config.num_samples() > accum.num_samples() { config } else { accum }
                        })
                        .unwrap()
                })
                .expect("failed to build display");

        let window = window.expect("failed to create window");
        let gl_display = gl_config.display();

        let context_attrs = ContextAttributesBuilder::new()
            .with_context_api(ContextApi::OpenGl(Some(Version::new(3, 3))))
            .build(Some(
                window
                    .window_handle()
                    .expect("failed to get window handle")
                    .into(),
            ));

        let not_current_context = unsafe { gl_display.create_context(&gl_config, &context_attrs) }
            .expect("failed to create GL context");

        let inner = window.inner_size();
        let surface_attrs = glutin::surface::SurfaceAttributesBuilder::<WindowSurface>::new()
            .build(
                window
                    .window_handle()
                    .expect("failed to get window handle")
                    .into(),
                NonZeroU32::new(inner.width).unwrap(),
                NonZeroU32::new(inner.height).unwrap(),
            );

        let gl_surface = unsafe { gl_display.create_window_surface(&gl_config, &surface_attrs) }
            .expect("failed to create GL surface");

        let gl_context = not_current_context
            .make_current(&gl_surface)
            .expect("failed to make GL context current");

        let _ = gl_surface
            .set_swap_interval(&gl_context, SwapInterval::Wait(NonZeroU32::new(1).unwrap()));

        let gl = unsafe {
            glow::Context::from_loader_function_cstr(|name| gl_display.get_proc_address(name))
        };

        Self { window, gl_context, gl_surface, gl }
    }

    fn physical_size(&self) -> (i32, i32) {
        let s = self.window.inner_size();
        (s.width as i32, s.height as i32)
    }

    fn pixel_ratio(&self) -> f32 {
        self.window.scale_factor() as f32
    }

    fn resize_surface(&self, new_size: winit::dpi::PhysicalSize<u32>) {
        self.gl_surface.resize(
            &self.gl_context,
            NonZeroU32::new(new_size.width).unwrap(),
            NonZeroU32::new(new_size.height).unwrap(),
        );
    }

    fn swap_buffers(&self) {
        self.gl_surface
            .swap_buffers(&self.gl_context)
            .expect("failed to swap buffers");
    }
}
