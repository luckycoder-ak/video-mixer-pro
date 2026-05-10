#!/bin/bash

# ================================================
#   VideoMixer Pro - 快速启动 (macOS/Linux)
# ================================================

set -e

# 颜色输出
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "\n${GREEN}================================================${NC}"
echo -e "${GREEN}  VideoMixer Pro - 快速启动${NC}"
echo -e "${GREEN}================================================${NC}\n"

# 检查 node_modules
if [ ! -d "node_modules" ]; then
    echo -e "${YELLOW}[提示] 首次运行，正在安装依赖...${NC}\n"
    npm install
    echo ""
fi

echo -e "${BLUE}[1/2] 启动开发服务器...${NC}\n"
echo -e "提示：浏览器会自动打开 http://localhost:1420\n"

# 后台启动
npm run dev &
TAURI_PID=$!

echo -e "${BLUE}[2/2] 等待开发服务器启动...${NC}\n"
sleep 3

echo -e "${GREEN}================================================${NC}"
echo -e "${GREEN}  开发服务器已启动！${NC}"
echo -e "${GREEN}================================================${NC}\n"

echo -e "提示：如浏览器未自动打开，请访问："
echo -e "      http://localhost:1420\n"
echo -e "按 Ctrl+C 可停止服务器\n"

# 等待用户按 Ctrl+C
wait $TAURI_PID
