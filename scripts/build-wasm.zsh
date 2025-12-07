#!/usr/bin/env zsh
# Builds WASM packages in target directory to keep crates clean

set -euo pipefail

# Colors
autoload -U colors && colors

# Directories
SCRIPT_DIR=${0:a:h}
ROOT_DIR=${SCRIPT_DIR:h}
RENDERER_DIR=$ROOT_DIR/beamterm-renderer
TARGET_DIR=$ROOT_DIR/target/wasm-pack
JS_DIR=$ROOT_DIR/js

# Target configurations
typeset -a TARGETS=(bundler web nodejs)

# Build mode
BUILD_MODE="debug"
WASM_PACK_FLAGS=""

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --release)
            BUILD_MODE="release"
            WASM_PACK_FLAGS="--release"
            shift
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Print colored message
log() {
    local level=$1
    shift
    case $level in
        info)  echo "$fg_bold[blue]→$reset_color $@" ;;
        ok)    echo "$fg_bold[green]✓$reset_color $@" ;;
        error) echo "$fg_bold[red]✗$reset_color $@" >&2 ;;
    esac
}

# Check dependencies
check_deps() {
    local missing=()

    if ! command -v wasm-pack &>/dev/null; then
        missing+=(wasm-pack)
    fi

    if ! command -v npm &>/dev/null; then
        missing+=(npm)
    fi

    if [[ ${#missing[@]} -gt 0 ]]; then
        log error "Missing dependencies: ${missing[*]}"
        log info "Install with:"
        [[ " ${missing[*]} " =~ " wasm-pack " ]] && \
            echo "  curl https://rustwasm.github.io/wasm-pack/installer/init.sh -sSf | sh"
        [[ " ${missing[*]} " =~ " npm " ]] && \
            echo "  Install Node.js from https://nodejs.org"
        exit 1
    fi
}

# Build WASM for a specific target
build_target() {
    local target=$1
    log info "Building $target package in $BUILD_MODE mode..."

    local out_dir=$TARGET_DIR/$target

    # Run wasm-pack
    wasm-pack build $RENDERER_DIR \
        --target $target \
        --out-dir $out_dir \
        --out-name beamterm_renderer \
        --no-pack \
        --features js-api \
        $WASM_PACK_FLAGS

    log ok "$target package built"
}

# Main build process
main() {
    log info "Starting WASM build in $BUILD_MODE mode..."

    # Check dependencies
    check_deps

    # Clean previous builds
    log info "Cleaning previous builds..."
    rm -rf $TARGET_DIR
    mkdir -p $TARGET_DIR

    # Build sequentially
    for target in $TARGETS; do
        build_target $target
    done

    # Copy to js/dist
    log info "Copying to js/dist..."
    cd $JS_DIR

    # Ensure package.json exists
    if [[ ! -f package.json ]]; then
        log error "package.json not found in js/"
        log info "Run './build.zsh setup' first"
        exit 1
    fi

    # Install dependencies if needed
    if [[ ! -d node_modules ]]; then
        log info "Installing JS dependencies..."
        npm ci
    fi

    # Run build script
    npm run build

    log ok "WASM build complete!"

    # Show output summary
    echo
    log info "Build outputs ($BUILD_MODE mode):"
    echo "  NPM package:  $JS_DIR/dist/bundler/"
    echo "  Web package:  $JS_DIR/dist/web/"
    echo "  CDN bundle:   $JS_DIR/dist/cdn/beamterm.min.js"
}

main "$@"