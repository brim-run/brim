#!/usr/bin/env bash
# BRIM Installation Script
# Usage: curl -fsSL https://raw.githubusercontent.com/brim-run/brim/main/install.sh | bash

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Configuration
REPO="brim-run/brim"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# Functions
print_info() {
    echo -e "${CYAN}→${NC} $1"
}

print_success() {
    echo -e "${GREEN}✓${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

detect_platform() {
    local os=$(uname -s)
    local arch=$(uname -m)
    
    case "$os" in
        Darwin)
            case "$arch" in
                x86_64)
                    echo "x86_64-macos"
                    ;;
                arm64)
                    echo "aarch64-macos"
                    ;;
                *)
                    print_error "Unsupported macOS architecture: $arch"
                    exit 1
                    ;;
            esac
            ;;
        Linux)
            case "$arch" in
                x86_64)
                    echo "x86_64-linux"
                    ;;
                aarch64|arm64)
                    echo "aarch64-linux"
                    ;;
                *)
                    print_error "Unsupported Linux architecture: $arch"
                    exit 1
                    ;;
            esac
            ;;
        *)
            print_error "Unsupported operating system: $os"
            exit 1
            ;;
    esac
}

get_latest_release() {
    curl -sL "https://api.github.com/repos/$REPO/releases/latest" | \
        grep '"tag_name":' | \
        sed -E 's/.*"([^"]+)".*/\1/'
}

download_and_install() {
    local platform=$1
    local version=$2
    local asset="brim-${platform}.tar.gz"
    local url="https://github.com/$REPO/releases/download/${version}/${asset}"
    local tmp_dir=$(mktemp -d)
    
    print_info "Downloading BRIM ${version} for ${platform}..."
    
    if ! curl -fsSL "$url" -o "$tmp_dir/$asset"; then
        print_error "Failed to download BRIM"
        rm -rf "$tmp_dir"
        exit 1
    fi
    
    print_info "Extracting..."
    tar -xzf "$tmp_dir/$asset" -C "$tmp_dir"
    
    mkdir -p "$INSTALL_DIR"
    
    if [ -f "$INSTALL_DIR/brim" ]; then
        print_warning "Existing installation found, replacing..."
    fi
    
    mv "$tmp_dir/brim" "$INSTALL_DIR/brim"
    chmod +x "$INSTALL_DIR/brim"
    
    if [ -f "$tmp_dir/brim-mcp" ]; then
        mv "$tmp_dir/brim-mcp" "$INSTALL_DIR/brim-mcp"
        chmod +x "$INSTALL_DIR/brim-mcp"
        print_success "BRIM and brim-mcp installed to $INSTALL_DIR"
    else
        print_success "BRIM installed to $INSTALL_DIR/brim"
    fi
    
    rm -rf "$tmp_dir"
}

verify_installation() {
    if [ ! -f "$INSTALL_DIR/brim" ]; then
        print_error "Installation verification failed"
        exit 1
    fi
    
    if ! command -v brim &> /dev/null; then
        print_warning "BRIM installed but not in PATH"
        print_info "Add $INSTALL_DIR to your PATH:"
        echo ""
        echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
        echo ""
        print_info "Add this line to your shell configuration:"
        if [ -f "$HOME/.zshrc" ]; then
            echo "  echo 'export PATH=\"\$PATH:$INSTALL_DIR\"' >> ~/.zshrc"
        elif [ -f "$HOME/.bashrc" ]; then
            echo "  echo 'export PATH=\"\$PATH:$INSTALL_DIR\"' >> ~/.bashrc"
        else
            echo "  echo 'export PATH=\"\$PATH:$INSTALL_DIR\"' >> ~/.profile"
        fi
        echo ""
    else
        print_success "BRIM is ready to use!"
        if [ -f "$INSTALL_DIR/brim-mcp" ]; then
            print_info "brim-mcp is available for MCP (Cursor/Claude). Add it in your MCP client config."
        fi
        echo ""
        "$INSTALL_DIR/brim" --help
    fi
}

# Main execution
main() {
    echo ""
    echo "╔═══════════════════════════════════════════════════════════════════╗"
    echo "║         BRIM - Brew Recipe Install Manager                        ║"
    echo "╚═══════════════════════════════════════════════════════════════════╝"
    echo ""
    
    # Check dependencies
    if ! command -v curl &> /dev/null; then
        print_error "curl is required but not installed"
        exit 1
    fi
    
    if ! command -v tar &> /dev/null; then
        print_error "tar is required but not installed"
        exit 1
    fi
    
    # Detect platform
    print_info "Detecting platform..."
    PLATFORM=$(detect_platform)
    print_success "Platform: $PLATFORM"
    
    # Get latest version
    print_info "Fetching latest release..."
    VERSION=$(get_latest_release)
    
    if [ -z "$VERSION" ]; then
        print_error "Could not determine latest version"
        exit 1
    fi
    
    print_success "Latest version: $VERSION"
    
    # Download and install
    download_and_install "$PLATFORM" "$VERSION"
    
    # Verify
    verify_installation
    
    echo ""
    print_success "Installation complete!"
    echo ""
}

main "$@"
