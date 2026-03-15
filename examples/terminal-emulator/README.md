# beamterm terminal emulator

A GPU-accelerated terminal emulator built with [beamterm-core](../../beamterm-core/),
using the dynamic font atlas.

Use Shift+F1 / Shift+F2 to adjust the font size on the fly.

![beamterm terminal emulator](screenshots/beamterm-terminal-emulator.png)

## Features

- **VT100/xterm-256color emulation** via [vt100](https://crates.io/crates/vt100)
- **Native PTY** via [portable-pty](https://crates.io/crates/portable-pty), spawns the user's default shell
- **True color support** — ANSI 16, 256-color cube, and 24-bit RGB
- **Text attributes** — bold, italic, underline, dim, inverse
- **Wide character support** — emoji and CJK characters
- **Application cursor mode** — compatible with TUI apps (e.g., vim, htop)
- **DSR (Device Status Report)** — cursor position queries for ratatui and similar frameworks

Terminal emulation is handled by `vt100` and `portable-pty`; beamterm provides
the GPU rendering layer.

## Running

```bash
cargo run -p terminal-emulator
```
