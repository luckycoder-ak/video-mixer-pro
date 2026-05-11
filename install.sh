#!/bin/bash

# ================================================
#   VideoMixer Pro - 一键安装 (macOS/Linux)
# ================================================

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m'

info() { echo -e "${BLUE}[INFO]${NC} $1"; }
success() { echo -e "${GREEN}[✓]${NC} $1"; }
warning() { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[✗]${NC} $1"; }

echo -e "\n${CYAN}======================================================${NC}"
echo -e "${CYAN}  VideoMixer Pro - 一键安装脚本${NC}"
echo -e "${CYAN}======================================================${NC}\n"

detect_os() {
    if [[ "$OSTYPE" == "darwin"* ]]; then
        echo "macOS"
    elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
        if [ -f /etc/debian_version ]; then
            echo "Linux (Debian/Ubuntu)"
        elif [ -f /etc/redhat-release ]; then
            echo "Linux (RedHat/CentOS)"
        else
            echo "Linux"
        fi
    else
        echo "Unknown"
    fi
}

OS=$(detect_os)
info "检测到操作系统: $OS"

install_brew() {
    if ! command -v brew &> /dev/null; then
        info "正在安装 Homebrew..."
        /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
        
        if [[ "$OS" == "macOS" ]]; then
            if [[ "$(uname -m)" == "arm64" ]]; then
                echo 'eval "$(/opt/homebrew/bin/brew shellenv)"' >> ~/.zprofile
                eval "$(/opt/homebrew/bin/brew shellenv)"
            fi
        fi
    fi
}

install_node() {
    if ! command -v node &> /dev/null; then
        info "正在安装 Node.js..."
        if [[ "$OS" == "macOS" ]]; then
            install_brew
            brew install node
        else
            curl -fsSL https://deb.nodesource.com/setup_lts.x | sudo -E bash -
            sudo apt-get install -y nodejs
        fi
    fi
}

install_rust() {
    if ! command -v rustc &> /dev/null; then
        info "正在安装 Rust..."
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
        source $HOME/.cargo/env
    fi
}

install_ffmpeg() {
    if ! command -v ffmpeg &> /dev/null; then
        info "正在安装 FFmpeg..."
        if [[ "$OS" == "macOS" ]]; then
            install_brew
            brew install ffmpeg
        else
            sudo apt-get update && sudo apt-get install -y ffmpeg
        fi
    fi
}

install_tauri_cli() {
    if ! command -v tauri &> /dev/null; then
        info "正在安装 Tauri CLI..."
        npm install -g @tauri-apps/cli
    fi
}

echo ""
info "[1/5] 检查 Node.js..."
install_node
success "Node.js - 已就绪"

echo ""
info "[2/5] 检查 Rust..."
install_rust
success "Rust - 已就绪"

echo ""
info "[3/5] 检查 FFmpeg..."
install_ffmpeg
success "FFmpeg - 已就绪"

echo ""
info "[4/5] 安装 Tauri CLI..."
install_tauri_cli
success "Tauri CLI - 已就绪"

echo ""
info "[5/5] 安装项目依赖..."
npm install
success "依赖安装 - 完成"

echo -e "\n${GREEN}======================================================${NC}"
echo -e "${GREEN}  安装完成！${NC}"
echo -e "${GREEN}======================================================${NC}\n"

echo -e "${CYAN}[快速开始]${NC}"
echo -e "  启动桌面应用:  bash run.sh"
echo -e "  开发模式:      npm run tauri dev"
echo -e "  完整构建:      npm run build:all\n"