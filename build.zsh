#!/usr/bin/env zsh
# Main build script for beamterm

set -euo pipefail

# Colors for output
autoload -U colors && colors
OK="$fg_bold[green]✓$reset_color"
ERROR="$fg_bold[red]✗$reset_color"
INFO="$fg_bold[blue]→$reset_color"
WARN="$fg_bold[yellow]⚠$reset_color"

SCRIPT_NAME=$(basename "$0")

# Directory detection:
# ${0:a:h} means:
#   0   = this script's path
#   :a  = absolute path (resolve symlinks)
#   :h  = head (directory part, like dirname)
SCRIPT_DIR=${0:a:h}
ROOT_DIR=$SCRIPT_DIR

# Available commands
typeset -A COMMANDS=(
    all          "Build everything (Rust + WASM)"
    clean        "Clean all build artifacts"
    build-rust   "Build Rust crates"
    build-wasm   "Build WASM packages"
    build-examples "Build all examples for deployment"
    test         "Run all tests"
    test-native  "Run native Rust tests"
    test-wasm    "Run WASM/JS tests"
    test-browser "Test WASM in browser with test page"
    atlas        "Generate font atlas"
    publish-npm  "Publish to NPM"
    dev          "Run development server"
    fmt          "Format code"
    clippy       "Run clippy lints"
    setup        "Initial JS setup"
    help         "Show this help"
)

# Print colored message
print_msg() {
    local level=$1
    shift
    case $level in
        ok)    echo "$OK $@" ;;
        error) echo "$ERROR $@" >&2 ;;
        info)  echo "$INFO $@" ;;
        warn)  echo "$WARN $@" ;;
    esac
}

# Clean build artifacts
cmd_clean() {
    print_msg info "Cleaning build artifacts..."

    cargo clean
    rm -rf $ROOT_DIR/target/wasm-pack
    rm -rf $ROOT_DIR/js/dist
    rm -rf $ROOT_DIR/js/examples-dist
    rm -rf $ROOT_DIR/js/node_modules
    rm -f $ROOT_DIR/data/*.atlas

    print_msg ok "Clean complete"
}

# Build Rust crates
cmd_build-rust() {
    print_msg info "Building Rust crates..."

    cargo build --release

    print_msg ok "Rust build complete"
}

# Build WASM packages
cmd_build-wasm() {
    print_msg info "Building WASM packages..."

    # Ensure Rust is built first
    if [[ ! -f "$ROOT_DIR/target/release/beamterm-atlas" ]]; then
        cmd_build-rust
    fi

    # Generate atlas if missing
    if [[ ! -f "$ROOT_DIR/data/bitmap_font.atlas" ]]; then
        cmd_atlas
    fi

    # Run WASM build script
    $ROOT_DIR/scripts/build-wasm.zsh

    print_msg ok "WASM build complete"
}

# Build examples for deployment
cmd_build-examples() {
    print_msg info "Building examples for deployment..."

    # Ensure WASM is built first
    if [[ ! -d "$ROOT_DIR/js/dist" ]]; then
        cmd_build-wasm
    fi

    cd $ROOT_DIR/js
    npm run build:examples

    print_msg ok "Examples build complete"
    print_msg info "Output directory: $ROOT_DIR/js/examples-dist"
}

# Build everything
cmd_all() {
    cmd_build-rust
    cmd_build-wasm
}

# Run all tests
cmd_test() {
    cmd_test-native
    cmd_test-wasm
}

# Run native tests
cmd_test-native() {
    print_msg info "Running native tests..."

    cargo test --workspace --exclude beamterm-renderer

    print_msg ok "Native tests passed"
}

# Run WASM tests
cmd_test-wasm() {
    print_msg info "Running WASM tests..."

    # Build if needed
    if [[ ! -d "$ROOT_DIR/js/dist" ]]; then
        cmd_build-wasm
    fi

    cd $ROOT_DIR/js
    npm test

    print_msg ok "WASM tests passed"
}

# Test WASM in browser
cmd_test-browser() {
    print_msg info "Starting test server for browser testing..."

    # Build if needed
    if [[ ! -d "$ROOT_DIR/js/dist" ]]; then
        cmd_build-wasm
    fi

    cd $ROOT_DIR/js

    print_msg info "Starting server at http://localhost:8080/test/test-wasm.html"
    print_msg info "Press Ctrl+C to stop"

    # Try different servers in order of preference
    if command -v serve &>/dev/null; then
        npx serve . -l 8080
    elif command -v http-server &>/dev/null; then
        npx http-server . -c-1 -p 8080
    elif command -v live-server &>/dev/null; then
        npx live-server . --port=8080
    else
        print_msg warn "No suitable server found. Installing 'serve'..."
        npm install -g serve
        npx serve . -l 8080
    fi
}

# Generate font atlas
cmd_atlas() {
    print_msg info "Generating font atlas..."

    # If no args provided, use defaults
    if [[ $# -eq 0 ]]; then
        cargo run --release --bin beamterm-atlas -- Hack \
            --line-height 1.0 \
            --font-size 15.0 \
            --emoji-font "Noto Color Emoji" \
            --output beamterm-data/atlas/bitmap_font.atlas \
            --check-missing \
            --symbols-file beamterm-data/atlas/additional-symbols.txt
    else
        cargo run --release --bin beamterm-atlas -- "$@"
    fi

    print_msg ok "Font atlas generated"
}

# Publish to NPM
cmd_publish-npm() {
    print_msg info "Publishing to NPM..."

    # Ensure everything is built
    cmd_build-wasm

    $ROOT_DIR/scripts/publish-npm.zsh

    print_msg ok "Published to NPM"
}

# Run development server
cmd_dev() {
    print_msg info "Starting development server..."

    cd $ROOT_DIR/beamterm-renderer
    trunk serve
}

# Format code
cmd_fmt() {
    print_msg info "Formatting code..."

    cargo fmt --all "$@"

    print_msg ok "Formatting complete"
}

# Run clippy
cmd_clippy() {
    print_msg info "Running clippy..."

    # If additional args are provided, use them; otherwise use default flags
    if [[ $# -eq 0 ]]; then
        cargo clippy --all-targets --all-features -- -D warnings
    else
        cargo clippy --all-targets --all-features "$@"
    fi

    print_msg ok "Clippy complete"
}

# Initial setup
cmd_setup() {
    print_msg info "Setting up JS environment..."

    $ROOT_DIR/scripts/setup-js.zsh

    print_msg ok "Setup complete"
}

# Show help
cmd_help() {
    echo "Beamterm Build Script"
    echo
    echo "Usage: $0 <command>"
    echo
    echo "Commands:"

    # Sort commands by name
    local sorted_commands=()
    for cmd in ${(ko)COMMANDS}; do
        sorted_commands+=($cmd)
    done

    for cmd in $sorted_commands; do
        printf "  %-14s %s\n" "$cmd" "$COMMANDS[$cmd]"
    done

    echo
    echo "Examples:"
    echo "  ./$SCRIPT_NAME all              # Build everything"
    echo "  ./$SCRIPT_NAME build-examples   # Build examples for GitHub Pages"
    echo "  ./$SCRIPT_NAME atlas jetbrains  # Generate atlas with JetBrains Mono"
}

# Main entry point
main() {
    local cmd=${1:-help}

    # Check if command exists
    if (( ! $+COMMANDS[$cmd] )); then
        print_msg error "Unknown command: $cmd"
        echo
        cmd_help
        exit 1
    fi

    # Change to root directory
    cd $ROOT_DIR

    # Execute command (shift away the command name)
    shift
    cmd_$cmd "$@"
}

# Run main with all arguments
main "$@"