# VideoMixer Pro - 快速开始指南

## 🚀 最简单的方式：直接下载预编译版本（推荐！）

1. 访问 GitHub Releases 页面：  
   https://github.com/luckycoder-ak/video-mixer-pro/releases

2. 下载适合你系统的安装包：
   - **Windows**: `.msi` 或 `.exe` 安装文件
   - **macOS**: `.dmg` 或 `.app` 文件
   - **Linux**: `.AppImage` 或 `.deb` 安装包

3. 双击安装，直接使用！

---

## 🛠️ 如果要自己编译（稍麻烦但可控）

### 方式 1：使用一键安装脚本（最推荐！）

#### Windows
```
双击运行：install.bat
```

#### macOS/Linux
```bash
bash install.sh
```

### 方式 2：手动安装

#### 步骤 1：安装基础工具

| 工具 | Windows | macOS | Linux |
|------|---------|-------|-------|
| **Node.js** | 下载: https://nodejs.org/ | `brew install node` | `sudo apt install nodejs` |
| **Rust** | 下载: https://rustup.rs/ | `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \| sh` | 同 macOS |
| **FFmpeg** | 下载: https://ffmpeg.org/download.html | `brew install ffmpeg` | `sudo apt install ffmpeg` |

#### 步骤 2：安装项目和运行

```bash
# 1. 安装依赖
npm install

# 2. 开发模式运行（直接预览）
npm run dev

# 3. 完整构建（生成安装包）
npm run build:all
```

---

## 📁 目录说明

```
video-mixer-pro/
├── install.bat            # Windows 一键安装
├── install.sh             # macOS/Linux 一键安装
├── scripts/
│   ├── build.bat          # Windows 构建脚本
│   ├── build.sh           # macOS/Linux 构建脚本
│   └── build.js           # 跨平台构建脚本
├── package.json           # npm 配置
└── README.md              # 本文件
```

---

## 🎯 常用命令

| 命令 | 说明 |
|------|------|
| `npm run dev` | 开发模式，快速预览 |
| `npm run build:all` | 完整构建 |
| `npm run build:win` | Windows 构建 |
| `npm run build:mac` | macOS 构建 |
| `npm run build:linux` | Linux 构建 |

---

## ⚠️ 注意事项

1. **FFmpeg 是必须的**：视频处理依赖 FFmpeg，没有它无法处理视频
2. **首次构建较慢**：Rust 首次编译需要下载依赖，后续会很快
3. **网络问题**：如果 npm 下载慢，可以使用国内镜像

---

## 📞 获取帮助

如遇问题，请在 GitHub Issues 反馈：
https://github.com/luckycoder-ak/video-mixer-pro/issues
