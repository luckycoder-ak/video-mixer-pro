@echo off
chcp 65001 >nul
title VideoMixer Pro - 一键安装向导
cls

echo ========================================================================
echo   VideoMixer Pro - Windows 一键安装
echo ========================================================================
echo.
echo [提示] 此脚本将自动安装所有必要依赖
echo.
pause

echo.
echo [1/6] 检查管理员权限...
net session >nul 2>&1
if %errorlevel% neq 0 (
    echo [警告] 未获取管理员权限，某些安装可能失败
    echo [提示] 右键点击脚本选择"以管理员身份运行"可能更好
    echo.
    pause
)

echo.
echo [2/6] 检查 Chocolatey 包管理器...
where choco >nul 2>&1
if %errorlevel% neq 0 (
    echo [信息] 正在安装 Chocolatey 包管理器...
    powershell -NoProfile -ExecutionPolicy Bypass -Command "[System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072; iex ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))"
    if %errorlevel% neq 0 (
        echo [错误] Chocolatey 安装失败
        echo.
        echo [手动安装说明]
        echo 1. 以管理员身份打开 PowerShell
        echo 2. 运行: Set-ExecutionPolicy Bypass -Scope Process -Force
        echo 3. 运行: [System.Net.ServicePointManager]::SecurityProtocol = [System.Net.ServicePointManager]::SecurityProtocol -bor 3072; iex ((New-Object System.Net.WebClient).DownloadString('https://community.chocolatey.org/install.ps1'))
        echo.
        pause
        exit /b 1
    )
    call refreshenv
)
echo [✓] Chocolatey 已就绪

echo.
echo [3/6] 安装 Node.js...
where node >nul 2>&1
if %errorlevel% neq 0 (
    choco install nodejs -y
    call refreshenv
)
echo [✓] Node.js 已就绪

echo.
echo [4/6] 安装 Rust...
where rustc >nul 2>&1
if %errorlevel% neq 0 (
    choco install rust -y
    call refreshenv
)
echo [✓] Rust 已就绪

echo.
echo [5/6] 安装 FFmpeg...
where ffmpeg >nul 2>&1
if %errorlevel% neq 0 (
    choco install ffmpeg -y
    call refreshenv
)
echo [✓] FFmpeg 已就绪

echo.
echo [6/6] 安装项目依赖和 Tauri CLI...
echo 安装 npm 依赖...
npm install
if %errorlevel% neq 0 (
    echo [错误] npm 依赖安装失败
    pause
    exit /b 1
)

echo 安装 Tauri CLI...
npm install -g @tauri-apps/cli
echo [✓] Tauri CLI 已就绪

echo.
echo ========================================================================
echo   所有依赖安装完成！
echo ========================================================================
echo.
echo [快速启动]
echo   双击运行 run.bat 启动 VideoMixer Pro 桌面应用
echo.
echo [其他选项]
echo   开发模式:     npm run tauri dev
echo   完整构建:     npm run build:all
echo.
pause