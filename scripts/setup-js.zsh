#!/usr/bin/env zsh
# Sets up the JS directory structure for the first time

set -euo pipefail

# Colors
autoload -U colors && colors

# Directories
SCRIPT_DIR=${0:a:h}
ROOT_DIR=${SCRIPT_DIR:h}
JS_DIR=$ROOT_DIR/js

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

# Create directory structure
create_directories() {
    log info "Creating JS directory structure..."

    local dirs=(
        $JS_DIR
        $JS_DIR/dist
        $JS_DIR/test/e2e
        $JS_DIR/examples/cdn
        $JS_DIR/examples/webpack/src
        $JS_DIR/examples/vite/src
    )

    for dir in $dirs; do
        mkdir -p $dir
        log ok "Created $dir"
    done
}

# Check if Node.js is installed
check_node() {
    if ! command -v node &>/dev/null; then
        log error "Node.js is not installed"
        log info "Please install Node.js from https://nodejs.org"
        exit 1
    fi

    local node_version=$(node -v)
    log ok "Node.js $node_version found"

    # Check version (need 16+)
    local major_version=${node_version:1:2}
    if [[ $major_version -lt 16 ]]; then
        log error "Node.js 16+ required (found $node_version)"
        exit 1
    fi
}

# Install dependencies
install_deps() {
    log info "Installing JS dependencies..."

    cd $JS_DIR

    if [[ ! -f package.json ]]; then
        log error "package.json not found!"
        log info "This script should be run from the beamterm root directory"
        exit 1
    fi

    # Install with npm
    npm ci

    log ok "Dependencies installed"
}


# Main setup process
main() {
    log info "Setting up Beamterm JS environment..."

    # Check prerequisites
    check_node

    # Create directories
    create_directories

    # Install dependencies
    install_deps

    log ok "JS setup complete!"

    # Show next steps
    echo
    log info "Next steps:"
    echo "  1. Generate font atlas:  ./build.zsh atlas"
    echo "  2. Build WASM packages:  ./build.zsh build-wasm"
    echo "  3. Run tests:            ./build.zsh test"
    echo
    log info "For all commands:        ./build.zsh help"
}

main