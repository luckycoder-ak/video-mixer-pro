# 提案：飞书 Webhook 通知（Feishu Webhook Notification）

## 1. 背景与动机

VideoMixer Pro 目前承担两类长耗时任务：
1. **合成任务**：随机裁剪 + 拼接 + 转场，单任务可能耗时数十分钟
2. **定时任务（CronFetcher）**：周期性 yt-dlp 拉取 TikTok 元数据

这两类任务完成时用户无法实时感知（尤其是后台运行 / 用户切到其他工作时），需要主动打开 App 才能看到结果。本提案引入「**飞书自定义机器人 Webhook 通知**」能力，把任务关键状态变迁实时推送到指定飞书群聊。

## 2. 目标

- 在主页面新增「**高级设置**」Tab，提供 Webhook URL 配置 + 「发送测试消息」按钮
- 用户填写 webhook 后，**两类任务完成时**（合成任务 Completed/Error/Partial、定时任务 success/failed/cooldown/paused）自动推送 `msg_type=text` 飞书消息到群聊
- 用户**未配置 webhook 时静默**（不发送、不阻塞、仅打 debug 日志）
- 全局唯一 webhook（不做配置级覆盖）
- 失败时按指数退避重试 2 次（5s/15s），仍失败则只记日志不阻塞主任务

## 3. 非目标

- ❌ 不支持飞书机器人签名校验（仅 URL，安全性由 webhook URL 自身的随机 token + 用户群权限保证）
- ❌ 不支持飞书富文本 / 卡片 / post 格式（仅 text）
- ❌ 不支持企业微信 / 钉钉 / Slack 等其他 IM
- ❌ 不做配置级覆盖（每个 VideoConfig 都共享全局 webhook）
- ❌ 不持久化通知历史 / 重发队列；失败仅记日志
- ❌ 不做通知频率限流（用户对发送频率有充分控制权——任务粒度本身就低频）

## 4. 关键决策摘要（已与研发对齐）

| # | 决策 | 终值 |
|---|---|---|
| D1 | 通知触发条件 | 两类任务**全部状态都通知**：合成任务 Completed/Error/Partial；定时任务 success/failed/cooldown/paused |
| D2 | 安全策略 | **仅 webhook URL，不支持签名校验**（用户自行控制 webhook 不外泄）|
| D3 | 消息格式 | `msg_type=text`，多行字符串拼接（任务名/状态/耗时/CSV 路径或失败原因）|
| D4 | HTTP 库 | `reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }`；async + rustls 无 OpenSSL 依赖 |
| D5 | 配置作用域 | **全局唯一 webhook**，存于 `AppData.app_settings.feishu_webhook_url`（新增 `AppSettings` 顶层字段）|
| D6 | 重试策略 | **失败重试 2 次**，间隔 5s / 15s（指数退避），仍失败仅 `log::warn` 不抛回 |
| D7 | 测试按钮 | 高级设置 Tab 内提供「发送测试消息」按钮，固定文案 `🤖 VideoMixer Pro 测试消息：webhook 配置正确，机器人已就绪` |
| D8 | 配置生效 | 保存即生效（修改 webhook 后下一个任务完成时使用新 URL；运行中任务不打断）|
| D9 | 未配置时行为 | webhook 为空 / 仅空白字符 → **静默跳过**（不抛错、不打 warn，仅 `log::debug`）|
| D10 | 失败日志 | 重试耗尽后 `log::warn!("飞书 webhook 通知失败: {}", err)`，不发 emit 也不阻塞主任务返回 |
| D11 | 默认行为 | 升级用户的 `app_data.json` 缺失 `app_settings` 字段时反序列化为默认空对象（向后兼容）|
| D12 | 超时 | reqwest 单次请求超时 10s（避免飞书侧偶发慢响应阻塞通知任务）|
| D13 | 通知发送时机 | 通知任务以 `tauri::async_runtime::spawn` 异步触发，**不阻塞**主任务返回结果给前端 |
| D14 | 文案语言 | 中文（emoji + 多行 text）|

## 5. 总体方案

### 5.1 配置存储扩展

`storage.rs::AppData` 顶层增加 `app_settings: AppSettings`：

```rust
#[derive(Default)]
pub struct AppSettings {
    pub feishu_webhook_url: String,  // 全局唯一；为空表示未配置
}
```

`AppData` 反序列化加 `#[serde(default)]`，向后兼容老版本数据文件。

### 5.2 通知模块

新增 `src-tauri/src/notifier.rs`：
- `pub async fn send_feishu_text(webhook: &str, content: &str) -> Result<(), String>`
- 内部 reqwest POST `{"msg_type":"text","content":{"text":"..."}}`
- 重试 2 次（5s/15s）；超时 10s
- 检查响应 JSON 的 `code` 字段判定成功（飞书约定 `code==0`）

新增 `notifier::notify_task_async(...)` / `notifier::notify_scheduled_run_async(...)` 异步派发函数，封装"读取 webhook → 拼接文本 → spawn 调用 send_feishu_text"流程。

### 5.3 触发点

- **合成任务**：[video_processor.rs:2949](file:///workspace/video-mixer-pro/src-tauri/src/video_processor.rs#L2949) 等任务终态写入处增加 `notify_task_async(...)` 调用
- **定时任务**：[scheduler.rs](file:///workspace/video-mixer-pro/src-tauri/src/scheduler.rs) `apply_post_run` 内三个分支（success / failed / cooldown / paused）补 `notify_scheduled_run_async(...)`

### 5.4 前端 UI

- `src/components/AdvancedSettings.tsx`：webhook URL 输入框 + 保存按钮 + 测试按钮 + 状态文案
- `App.tsx`：tabs 数组追加 `'advanced'` 项；activeTab 联动；保存后刷新 settings 状态

### 5.5 Tauri 命令

- `get_app_settings() -> AppSettings`
- `save_app_settings(settings: AppSettings) -> ()`
- `send_feishu_test_message() -> ()`（读 settings → 调 notifier）

## 6. 风险与缓解

| 风险 | 缓解 |
|---|---|
| 用户填错 URL 导致重试 2 次后仍失败，污染日志 | 失败仅 `log::warn`，不 emit 给前端；测试按钮鼓励用户先验证 URL |
| 飞书侧限流 / 暂时不可用 | 主任务不阻塞，业务返回正常结果；通知失败仅日志 |
| 大量并发通知（连续多任务完成） | spawn 派发，每个独立 future；reqwest 复用底层连接池 |
| 通知文本过长（错误堆栈过大） | 错误消息截断至 500 字符，附 `...(已截断)` 提示 |
| webhook URL 含敏感 token，不应日志泄露 | 日志中只记 host + path 末 8 字符，不记完整 URL |
| 升级用户 `app_data.json` 无 app_settings 字段 | `#[serde(default)]` 兼容反序列化；首次保存时正常写入 |

## 7. 影响面

- **新增依赖**：`reqwest = "0.12"`（rustls-tls + json）
- **新增文件**：`src-tauri/src/notifier.rs`、`src/components/AdvancedSettings.tsx`
- **修改文件**：`storage.rs`（AppData + AppSettings + load/save）、`main.rs`（注册 commands、AppState）、`video_processor.rs`（任务终态触发）、`scheduler.rs`（定时任务终态触发）、`App.tsx`（tabs + 路由）、`types.ts`（AppSettings type）
- **测试**：notifier 文本拼接 / URL 校验 / 截断逻辑 单元测试
- **不影响现有功能**：webhook 未配置时所有路径完全静默
