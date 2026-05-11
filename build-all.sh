#!/bin/bash

# ================================================
#   VideoMixer Pro - macOS 一键构建脚本
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

echo -e "\n${CYAN}================================================${NC}"
echo -e "${CYAN}  VideoMixer Pro - macOS 一键构建${NC}"
echo -e "${CYAN}================================================${NC}\n"

check_command() {
    if ! command -v $1 &> /dev/null; then
        echo -e "${RED}[错误] 未找到 $1${NC}"
        echo "       请先运行 install.sh 安装依赖"
        exit 1
    fi
}

info "[检查依赖]"
check_command "node"
check_command "rustc"
check_command "cargo"
success "依赖检查完成\n"

if ! command -v ffmpeg &> /dev/null; then
    warning "未找到 FFmpeg，视频处理功能可能无法正常工作"
    warning "运行 install.sh 可以自动安装 FFmpeg\n"
fi

info "[1/3] 安装项目依赖..."
npm install
success "依赖安装完成\n"

info "[2/3] 构建前端..."
npm run build
success "前端构建完成\n"

info "[3/3] 构建 Tauri 应用...\n"
npm run tauri build

echo -e "\n${GREEN}================================================${NC}"
echo -e "${GREEN}  构建完成！${NC}"
echo -e "${GREEN}================================================${NC}\n"

success "输出目录: src-tauri/target/release/bundle/"

if [[ "$OSTYPE" == "darwin"* ]]; then
    echo -e "${GREEN}macOS 应用:${NC}"
    echo "  src-tauri/target/release/bundle/macos/"
    echo "  src-tauri/target/release/bundle/dmg/"
    echo ""
    echo -e "${GREEN}DMG 安装包:${NC}"
    echo "  src-tauri/target/release/bundle/dmg/VideoMixer Pro_x.x.x_x64.dmg"
    echo ""
    echo -e "${GREEN}应用包:${NC}"
    echo "  src-tauri/target/release/VideoMixer Pro.app"
else
    echo -e "${GREEN}Linux 应用:${NC}"
    echo "  src-tauri/target/release/bundle/appimage/"
    echo "  src-tauri/target/release/bundle/deb/"
fi

echo ""
