# VideoMixer Pro

视频随机裁剪合成工具 - 面向短视频创作者的Windows桌面客户端软件

## 功能特性

- **配置管理**: 创建、编辑、删除视频合成配置
- **模板片段**: 支持配置1-10个片段，每个片段可设置不同的裁剪模式
- **裁剪模式**: 
  - 单视频模式：直接截取指定时长
  - 双列模式：两视频左右拼接，等比例缩放
  - 四宫格模式：四视频田字格排列，等比例缩放
- **虚化背景**: 双列/四宫格模式下自动填充虚化背景，避免黑边
- **教程片段**: 支持教程片段配置，全局去重使用
- **任务队列**: 后台批量生成任务，支持暂停/继续
- **本地存储**: 配置和任务数据本地JSON文件存储

## 技术栈

- **桌面框架**: Tauri 2.0
- **前端**: React + TypeScript + Tailwind CSS
- **后端**: Rust
- **视频处理**: FFmpeg
- **状态管理**: Zustand

## 开发

### 前置要求

- Node.js 18+
- Rust 1.70+
- FFmpeg (需要添加到系统PATH)

### 安装依赖

```bash
npm install
```

### 开发模式

```bash
npm run tauri dev
```

### 构建

```bash
npm run tauri build
```

## 使用说明

### 1. 创建配置

1. 点击"新建配置"按钮
2. 填写配置名称（要求不重复）
3. 选择视频比例（9:16/16:9/1:1）
4. 选择音频文件（必填）
5. 配置模板片段：
   - 设置片段总时长
   - 设置片段数量
   - 为每个片段选择来源文件夹和裁剪模式
6. 选择教程片段文件夹
7. 点击"保存"

### 2. 生成视频

1. 在配置列表中点击"生成"按钮
2. 输入生成数量
3. 点击"确认生成"
4. 在任务列表中查看进度

## 项目结构

```
video-mixer-pro/
├── src/                    # React前端源码
│   ├── components/        # React组件
│   ├── App.tsx           # 主应用组件
│   ├── main.tsx          # 入口文件
│   └── types.ts          # TypeScript类型定义
├── src-tauri/            # Tauri/Rust后端源码
│   ├── src/
│   │   ├── main.rs       # 主入口
│   │   ├── config.rs     # 配置管理模块
│   │   ├── storage.rs    # 数据存储模块
│   │   └── video_processor.rs  # 视频处理模块
│   └── Cargo.toml        # Rust依赖
├── package.json          # Node.js依赖
├── tsconfig.json         # TypeScript配置
├── tailwind.config.js    # Tailwind CSS配置
└── vite.config.ts        # Vite配置
```

## 视频合成逻辑

### 单视频模式
- 直接截取指定时长
- 保持原始宽高比

### 双列模式（等比例缩放 + 虚化背景）
1. 将两个视频等比例缩放至宽度的一半
2. 居中放置在输出画布上
3. 上下黑边使用视频内容的模糊版本填充

### 四宫格模式（等比例缩放 + 虚化背景）
1. 将四个视频等比例缩放至1/4宽度
2. 田字格排列
3. 上下黑边使用虚化背景填充

### 最终合成流程
1. 处理所有模板片段
2. 处理教程片段（全局去重）
3. 按顺序拼接片段，添加随机转场
4. 替换为配置的音频
5. 以音频长度为准截断视频

## 数据存储

- 配置数据: `%APPDATA%/VideoMixerPro/configs/`
- 任务记录: `%APPDATA%/VideoMixerPro/tasks/`
- 使用记录: `%APPDATA%/VideoMixerPro/usage_records/`
- 输出文件: `%APPDATA%/VideoMixerPro/output/`

## License

MIT
