# VideoMixer Pro - Agent Instructions

## 项目概述

VideoMixer Pro 是一个面向短视频创作者的跨平台桌面客户端软件，用于视频随机裁剪合成。

**技术栈:**
- 桌面框架: Tauri 2.0
- 前端: React + TypeScript + Tailwind CSS
- 后端: Rust
- 视频处理: FFmpeg
- 状态管理: Zustand

---

## 核心功能模块

### 1. 配置管理 ([config.rs](file:///workspace/video-mixer-pro/src-tauri/src/config.rs))
- 创建、编辑、删除视频合成配置
- 模板片段配置 (1-10个片段)
- 裁剪模式: 单视频/双列/四宫格

### 2. 视频处理 ([video_processor.rs](file:///workspace/video-mixer-pro/src-tauri/src/video_processor.rs))
- FFmpeg 视频裁剪、拼接
- 转场效果处理
- 音频处理 (按最短长度截断)
- 教程视频片段管理
- 字幕处理

### 3. 数据存储 ([storage.rs](file:///workspace/video-mixer-pro/src-tauri/src/storage.rs))
- JSON 文件存储配置和任务数据
- 本地应用目录管理

### 4. 前端界面 ([App.tsx](file:///workspace/video-mixer-pro/src/App.tsx))
- 配置列表和编辑
- 任务队列管理
- 进度展示
- 与 Tauri 后端通信

---

## Agent 工作流程

### 开发流程
1. 前端开发: 修改 `src/` 目录下的 React 组件
2. 后端开发: 修改 `src-tauri/src/` 目录下的 Rust 代码
3. 运行开发模式: `npm run tauri dev`
4. 构建: `npm run tauri build`

### 提交规范
1. 小范围修改直接提交
2. 重大功能更新创建 commit 并打 tag
3. 推送到 main 分支会触发 GitHub Actions 自动构建

---

## 最近修改记录

### v1.0.5
1. ✅ 视频音频拼接时按最短长度截断
2. ✅ 视频生成完成后自动删除教程视频文件
3. ✅ 生成前检查教程视频文件夹是否为空
4. ✅ 删除视频尾部补齐逻辑
5. ✅ 修复 Rust 编译错误

---

## 关键代码位置

| 功能 | 文件 |
|------|------|
| 主入口 | [main.rs](file:///workspace/video-mixer-pro/src-tauri/src/main.rs) |
| 视频处理核心 | [video_processor.rs](file:///workspace/video-mixer-pro/src-tauri/src/video_processor.rs) |
| 前端主组件 | [App.tsx](file:///workspace/video-mixer-pro/src/App.tsx) |
| 类型定义 | [types.ts](file:///workspace/video-mixer-pro/src/types.ts) |
| CI/CD 配置 | [build-windows.yml](file:///workspace/video-mixer-pro/.github/workflows/build-windows.yml) |
