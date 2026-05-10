#!/bin/bash

# ================================================
#   VideoMixer Pro - macOS/Linux 构建脚本
# ================================================

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}================================================${NC}"
echo -e "${GREEN}  VideoMixer Pro - 构建脚本${NC}"
echo -e "${GREEN}================================================${NC}"
echo ""

# 检查函数
check_command() {
    if ! command -v $1 &> /dev/null; then
        echo -e "${RED}[错误] 未找到 $1${NC}"
        echo "       请先安装: $2"
        exit 1
    fi
}

# 检查依赖
echo -e "${YELLOW}[检查依赖]${NC}"

check_command "node" "Node.js: https://nodejs.org/"
check_command "rustc" "Rust: https://rustup.rs/"
check_command "cargo" "Rust: https://rustup.rs/"

# 检查 FFmpeg (警告但不退出)
if ! command -v ffmpeg &> /dev/null; then
    echo -e "${YELLOW}[警告] 未找到 FFmpeg，视频处理功能可能无法正常工作${NC}"
    echo -e "        安装地址: https://ffmpeg.org/download.html"
    echo ""
fi

# 检查 macOS 特有依赖
if [[ "$OSTYPE" == "darwin"* ]]; then
    if command -v brew &> /dev/null; then
        echo -e "${YELLOW}[提示] 使用 Homebrew 安装 FFmpeg:${NC}"
        echo "       brew install ffmpeg"
        echo ""
    fi
fi

# 清理函数
cleanup() {
    if [ -d "node_modules" ]; then
        echo -e "${YELLOW}[清理] 移除旧的 node_modules...${NC}"
        rm -rf node_modules
    fi
}

# 主构建流程
echo -e "${YELLOW}[1/3] 安装依赖...${NC}"
cleanup
npm install

echo ""
echo -e "${YELLOW}[2/3] 构建前端...${NC}"
npm run build

echo ""
echo -e "${YELLOW}[3/3] 构建 Tauri 应用...${NC}"
npm run tauri build

echo ""
echo -e "${GREEN}================================================${NC}"
echo -e "${GREEN}  构建完成！${NC}"
echo -e "${GREEN}================================================${NC}"
echo ""
echo "输出目录: src-tauri/target/release/bundle/"
echo ""

# 根据操作系统显示结果
if [[ "$OSTYPE" == "darwin"* ]]; then
    echo -e "${GREEN}macOS 应用:${NC}"
    echo "  src-tauri/target/release/bundle/macos/"
    echo "  src-tauri/target/release/bundle/dmg/"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    echo -e "${GREEN}Linux 应用:${NC}"
    echo "  src-tauri/target/release/bundle/appimage/"
    echo "  src-tauri/target/release/bundle/deb/"
fi

echo ""
echo -e "${GREEN}可执行文件:${NC}"
if [[ "$OSTYPE" == "darwin"* ]]; then
    echo "  src-tauri/target/release/video-mixer-pro.app"
elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
    echo "  src-tauri/target/release/video-mixer-pro"
else
    echo "  src-tauri/target/release/VideoMixer Pro.exe"
fi
echo ""
