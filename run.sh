#!/bin/bash

# ================================================
#   VideoMixer Pro - 快速启动 (macOS/Linux)
# ================================================

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "\n${GREEN}================================================${NC}"
echo -e "${GREEN}  VideoMixer Pro - 快速启动${NC}"
echo -e "${GREEN}================================================${NC}\n"

if [ ! -d "node_modules" ]; then
    echo -e "${YELLOW}[提示] 首次运行，正在安装依赖...${NC}\n"
    npm install
    echo ""
fi

echo -e "${BLUE}[1/2] 构建前端代码...${NC}\n"
npm run build
echo ""

echo -e "${BLUE}[2/2] 启动 Tauri 桌面应用...${NC}\n"
echo -e "提示：会自动打开 VideoMixer Pro 桌面窗口\n"

npm run tauri dev

echo -e "\n${GREEN}================================================${NC}"
echo -e "${GREEN}  应用已退出${NC}"
echo -e "${GREEN}================================================${NC}\n"