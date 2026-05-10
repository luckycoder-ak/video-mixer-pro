@echo off
chcp 65001 >nul
echo ================================================
echo   VideoMixer Pro - Windows 构建脚本
echo ================================================
echo.

REM 检查 Node.js
where node >nul 2>&1
if %errorlevel% neq 0 (
    echo [错误] 未找到 Node.js，请先安装: https://nodejs.org/
    exit /b 1
)

REM 检查 Rust
where rustc >nul 2>&1
if %errorlevel% neq 0 (
    echo [错误] 未找到 Rust，请先安装: https://rustup.rs/
    exit /b 1
)

REM 检查 FFmpeg
where ffmpeg >nul 2>&1
if %errorlevel% neq 0 (
    echo [警告] 未找到 FFmpeg，视频处理功能可能无法正常工作
    echo         安装地址: https://ffmpeg.org/download.html
    echo.
)

echo [1/3] 检查依赖...
call npm install
if %errorlevel% neq 0 (
    echo [错误] npm install 失败
    exit /b 1
)

echo.
echo [2/3] 构建前端...
call npm run build
if %errorlevel% neq 0 (
    echo [错误] 前端构建失败
    exit /b 1
)

echo.
echo [3/3] 构建 Tauri 应用...
call npm run tauri build
if %errorlevel% neq 0 (
    echo [错误] Tauri 构建失败
    exit /b 1
)

echo.
echo ================================================
echo   构建完成！
echo ================================================
echo.
echo 输出目录: src-tauri\target\release\bundle\
echo.
echo 可执行文件: src-tauri\target\release\VideoMixer Pro.exe
echo.

pause
