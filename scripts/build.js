#!/usr/bin/env node

/**
 * VideoMixer Pro - 跨平台构建脚本
 * 支持 Windows, macOS, Linux
 */

import { execSync } from 'child_process';
import { existsSync } from 'fs';
import { platform, arch } from 'os';

const colors = {
    red: '\x1b[31m',
    green: '\x1b[32m',
    yellow: '\x1b[33m',
    blue: '\x1b[34m',
    reset: '\x1b[0m'
};

function log(color, prefix, message) {
    console.log(`${color}[${prefix}]${colors.reset} ${message}`);
}

function success(message) { log(colors.green, '✓', message); }
function error(msg) { log(colors.red, '✗', msg); }
function info(message) { log(colors.blue, 'ℹ', message); }
function warn(message) { log(colors.yellow, '⚠', message); }

function exec(command, options = {}) {
    try {
        const result = execSync(command, {
            stdio: 'inherit',
            ...options
        });
        return true;
    } catch (e) {
        return false;
    }
}

function checkCommand(cmd, installUrl) {
    try {
        execSync(cmd.split(' ')[0], { stdio: 'ignore' });
        return true;
    } catch {
        return false;
    }
}

function main() {
    console.log('\n' + colors.green + '================================================' + colors.reset);
    console.log(colors.green + '  VideoMixer Pro - 跨平台构建脚本' + colors.reset);
    console.log(colors.green + '================================================' + colors.reset + '\n');

    const currentPlatform = platform();
    const currentArch = arch();
    info(`检测到平台: ${currentPlatform} (${currentArch})`);

    info('检查依赖...\n');

    if (!checkCommand('node --version', 'https://nodejs.org/')) {
        error('未找到 Node.js，请先安装: https://nodejs.org/');
        process.exit(1);
    }

    if (!checkCommand('rustc --version', 'https://rustup.rs/')) {
        error('未找到 Rust，请先安装: https://rustup.rs/');
        process.exit(1);
    }

    if (!checkCommand('cargo --version', 'https://rustup.rs/')) {
        error('未找到 Cargo，请先安装: https://rustup.rs/');
        process.exit(1);
    }

    if (!checkCommand('ffmpeg -version', 'https://ffmpeg.org/download.html')) {
        warn('未找到 FFmpeg，视频处理功能可能无法正常工作');
        warn('安装地址: https://ffmpeg.org/download.html\n');
    }

    try {
        info('检查 npm 依赖...');
        if (!existsSync('node_modules')) {
            info('安装 npm 依赖...');
            exec('npm install');
        }
        success('依赖检查完成\n');

        info('构建前端...');
        if (!exec('npm run build')) {
            error('前端构建失败');
            process.exit(1);
        }
        success('前端构建完成\n');

        info('构建 Tauri 应用...');
        if (!exec('npm run tauri build')) {
            error('Tauri 构建失败');
            process.exit(1);
        }

        console.log('\n' + colors.green + '================================================' + colors.reset);
        console.log(colors.green + '  构建完成！' + colors.reset);
        console.log(colors.green + '================================================' + colors.reset + '\n');

        success('输出目录: src-tauri/target/release/bundle/');

        if (currentPlatform === 'win32') {
            success('Windows 可执行文件: src-tauri/target/release/VideoMixer Pro.exe');
        } else if (currentPlatform === 'darwin') {
            success('macOS 应用: src-tauri/target/release/video-mixer-pro.app');
        } else {
            success('Linux 可执行文件: src-tauri/target/release/video-mixer-pro');
        }

        console.log('');

    } catch (e) {
        error(`构建失败: ${e.message}`);
        process.exit(1);
    }
}

main();
