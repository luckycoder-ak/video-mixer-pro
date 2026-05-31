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

### 5. 定时元数据采集 ([scheduled_fetcher.rs](file:///workspace/video-mixer-pro/src-tauri/src/scheduled_fetcher.rs) / [scheduler.rs](file:///workspace/video-mixer-pro/src-tauri/src/scheduler.rs))
- 通过 yt-dlp sidecar 定期采集 TikTok 视频元数据并写入 CSV
- 任务级 tiktok_iid 配置（覆盖默认 IID）
- 单任务串行调度（全局 run_lock）+ 错峰 5s 防封禁
- 失败状态机：3 次连续失败 → 冷却 1h；6 次 → 自动停用
- CSV 命名：`<run_index>-<YYYYMMDD_HHMMSS>.csv`（冲突追加 UUID 后缀）
- UTF-8 with BOM 编码 + 仅单次运行内去重

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

### v1.0.7
1. ✅ 新增飞书自定义机器人 Webhook 通知能力（合成任务 + 定时任务终态）
2. ✅ 新增「高级设置」Tab：Webhook URL 配置 + 测试发送按钮
3. ✅ AppData 新增 `app_settings` 字段（向后兼容，缺失走 Default）
4. ✅ 通知模块 `notifier.rs`：reqwest 0.12 + rustls-tls，2 次指数退避（5s/15s）
5. ✅ 失败仅 `log::warn` 不阻塞主流程；webhook 未配置时静默跳过
6. ✅ Webhook 持久化使用 `write_app_data_settings_only`，避免触发 scheduler reconcile

### v1.0.6
1. ✅ 新增定时元数据采集功能（Scheduled TikTok Fetcher）
2. ✅ 集成 yt-dlp sidecar（macOS / Windows，CI 自动下载）
3. ✅ 新增 Scheduler 后台调度循环（tick=60s + 全局串行锁 + 错峰 5s）
4. ✅ 新增 ScheduledFetcherCard / Form / TestModal / RunsList 前端组件
5. ✅ 任务列表 tab 改为双栏布局：合成任务 + 定时任务执行
6. ✅ IID 失效全局 Toast 提示并自动跳转编辑页
7. ✅ 失败两阶梯策略（3 次冷却 1h / 6 次停用）

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
| 定时采集逻辑 | [scheduled_fetcher.rs](file:///workspace/video-mixer-pro/src-tauri/src/scheduled_fetcher.rs) |
| 调度器与命令 | [scheduler.rs](file:///workspace/video-mixer-pro/src-tauri/src/scheduler.rs) |
| 飞书通知模块 | [notifier.rs](file:///workspace/video-mixer-pro/src-tauri/src/notifier.rs) |
| 前端主组件 | [App.tsx](file:///workspace/video-mixer-pro/src/App.tsx) |
| 定时任务卡片 | [ScheduledFetcherCard.tsx](file:///workspace/video-mixer-pro/src/components/ScheduledFetcherCard.tsx) |
| 定时任务执行列表 | [ScheduledRunsList.tsx](file:///workspace/video-mixer-pro/src/components/ScheduledRunsList.tsx) |
| 高级设置面板 | [AdvancedSettings.tsx](file:///workspace/video-mixer-pro/src/components/AdvancedSettings.tsx) |
| 类型定义 | [types.ts](file:///workspace/video-mixer-pro/src/types.ts) |
| CI/CD 配置 | [build-windows.yml](file:///workspace/video-mixer-pro/.github/workflows/build-windows.yml) |
