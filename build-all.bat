@echo off
chcp 65001 >nul
title VideoMixer Pro - 一键构建安装包
cls

echo ================================================
echo   VideoMixer Pro - Windows 一键构建
echo ================================================
echo.

REM 检查 Node.js
where node >nul 2>&1
if %errorlevel% neq 0 (
    echo [信息] 正在安装 Node.js...
    if not exist "C:\ProgramData\chocolatey\bin\node.exe" (
        echo [错误] 未安装 Node.js，请先运行 install.bat 安装依赖
        pause
        exit /b 1
    )
)

REM 检查 Rust
where rustc >nul 2>&1
if %errorlevel% neq 0 (
    echo [错误] 未安装 Rust，请先运行 install.bat 安装依赖
    pause
    exit /b 1
)

REM 检查 FFmpeg
where ffmpeg >nul 2>&1
if %errorlevel% neq 0 (
    echo [警告] 未找到 FFmpeg，视频处理功能可能无法正常工作
    echo         运行 install.bat 可以自动安装 FFmpeg
    echo.
)

echo [1/3] 安装项目依赖...
call npm install
if %errorlevel% neq 0 (
    echo [错误] npm install 失败
    pause
    exit /b 1
)
echo.

echo [2/3] 构建前端...
call npm run build
if %errorlevel% neq 0 (
    echo [错误] 前端构建失败
    pause
    exit /b 1
)
echo.

echo [3/3] 构建 Tauri 应用...
echo.
call npm run tauri build
if %errorlevel% neq 0 (
    echo [错误] Tauri 构建失败
    pause
    exit /b 1
)

echo.
echo ================================================
echo   构建完成！
echo ================================================
echo.
echo 输出目录: src-tauri\target\release\bundle\
echo.
echo 安装包:   src-tauri\target\release\bundle\msi\VideoMixer Pro_x.x.x_x64-setup.exe
echo.
echo 便携版:   src-tauri\target\release\VideoMixer Pro.exe
echo.
pause
