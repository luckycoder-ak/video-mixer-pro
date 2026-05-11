@echo off
chcp 65001 >nul
title VideoMixer Pro - 快速启动

echo ================================================
echo   VideoMixer Pro - 快速启动
echo ================================================
echo.

if not exist "node_modules" (
    echo [提示] 首次运行，正在安装依赖...
    echo.
    npm install
    if %errorlevel% neq 0 (
        echo [错误] 依赖安装失败
        pause
        exit /b 1
    )
    echo.
)

echo [1/2] 构建前端代码...
echo.
npm run build
if %errorlevel% neq 0 (
    echo [错误] 前端构建失败
    pause
    exit /b 1
)
echo.

echo [2/2] 启动 Tauri 桌面应用...
echo.
echo 提示：会自动打开 VideoMixer Pro 桌面窗口
echo.

npm run tauri dev

echo.
echo ================================================
echo   应用已退出
echo ================================================
echo.

pause