@echo off
chcp 65001 >nul
title VideoMixer Pro - 快速启动

echo ================================================
echo   VideoMixer Pro - 快速启动
echo ================================================
echo.

REM 检查 node_modules
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

echo [1/2] 启动开发服务器...
echo.
echo 提示：浏览器会自动打开 http://localhost:1420
echo.

start npm run dev

echo.
echo [2/2] 等待开发服务器启动...
echo.
timeout /t 3 >nul

echo ================================================
echo   开发服务器已启动！
echo ================================================
echo.
echo 提示：如浏览器未自动打开，请访问：
echo       http://localhost:1420
echo.
echo 按 Ctrl+C 可停止服务器
echo.

pause
