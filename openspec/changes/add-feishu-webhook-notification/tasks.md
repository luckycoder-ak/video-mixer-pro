# 任务清单：飞书 Webhook 通知（Feishu Bot Notification）

> 标记规则：[ ] 待办 / [x] 完成 / [~] 进行中 / [!] 阻塞
> 决策映射：每个任务后括号引用 proposal.md `D1-D14`，便于对照检查

---

## P0 - 依赖与基础设施

- [ ] **T1** 修改 `src-tauri/Cargo.toml`，新增依赖（D4）：
  - `reqwest = { version = "0.12", default-features = false, features = ["json", "rustls-tls"] }`
  - 验证：`cargo check` 通过；macOS 不引入 OpenSSL/native-tls；Windows 不引入 schannel 之外的额外依赖
- [ ] **T2** 确认 `tokio` 已含 `time` feature（用于 `tokio::time::sleep` 退避）；如缺失则补上
- [ ] **T3** 不需要修改 `tauri.conf.json` / `capabilities/default.json` —— webhook 通过 reqwest 直接 HTTPS 请求，不走 `shell` / `http` plugin

## P1 - 数据模型与持久化

- [ ] **T4** 在 `src-tauri/src/storage.rs` 新增结构体（D5、D11）：
  ```rust
  #[derive(Debug, Clone, Default, Serialize, Deserialize)]
  pub struct AppSettings {
      #[serde(default)]
      pub feishu_webhook_url: String,
  }
  ```
- [ ] **T5** 在 `storage.rs::AppData` 增加字段（D11）：
  - `#[serde(default)] pub app_settings: AppSettings`
  - 验证：旧版 `app_data.json` 缺失 `app_settings` 时反序列化为 `AppSettings::default()`（空字符串）
- [ ] **T6** 在 `storage.rs` 新增辅助函数 `pub fn write_app_data_settings_only(app_handle: &AppHandle, settings: &AppSettings) -> Result<(), String>`：
  - 读取现有 `AppData` → 仅替换 `app_settings` 字段 → 原子写回
  - 不影响 `configs / tasks / usage_records` 字段
- [ ] **T7** Rust 单元测试 `storage::tests`：
  - 旧 JSON（无 `app_settings`）反序列化成功，`feishu_webhook_url == ""`
  - `write_app_data_settings_only` 写入后再读取，`feishu_webhook_url` 一致；其他字段不丢失

## P2 - 通知模块（notifier.rs）

- [ ] **T8** 新建 `src-tauri/src/notifier.rs`，模块声明加入 `main.rs`
- [ ] **T9** 定义常量（D6、D12）：
  ```rust
  const MAX_ATTEMPTS: u32 = 3;
  const BACKOFF_SECS: [u64; 2] = [5, 15];
  const REQUEST_TIMEOUT_SECS: u64 = 10;
  const MAX_ERROR_TEXT_LEN: usize = 500;
  ```
- [ ] **T10** 定义枚举：
  ```rust
  pub enum NotificationExtra {
      Normal,
      CooldownTriggered { until: chrono::DateTime<chrono::Utc> },
      Paused,
  }
  ```
- [ ] **T11** 实现 `pub async fn send_feishu_text(webhook: &str, content: &str) -> Result<(), String>`（D2、D3、D6、D12）：
  - 构造 `serde_json::json!({"msg_type":"text","content":{"text":content}})`
  - `reqwest::Client::builder().timeout(10s).build()`
  - 循环 3 次：成功立即返回；失败 sleep BACKOFF_SECS[attempt]
  - 单次失败判定：HTTP 非 2xx，或响应 JSON `code != 0`
- [ ] **T12** 实现内部 `async fn try_post_once(client, webhook, body) -> Result<(), String>`：
  - POST → 解析 JSON → 检查 `code == 0`；非 0 时拼 `code={} msg={}` 返回 Err
- [ ] **T13** 实现 `pub fn redact_webhook(url: &str) -> String`（D2 安全）：
  - 仅保留 host + 末 8 字符（如 `open.feishu.cn/...****abcd1234`）
  - 用于 log message，不通过 emit 暴露
- [ ] **T14** 实现 `fn truncate_for_msg(s: &str) -> String`：
  - 按 `chars()` 截断 500 字符 + `...(已截断)`
- [ ] **T15** 实现 `fn format_duration(seconds: i64) -> String`：
  - `>= 3600` → `Hh Mm`；`>= 60` → `Mm Ss`；其他 → `Ss`
- [ ] **T16** 实现 `fn build_task_text(task: &Task) -> String`（D14）：
  - 模板：合成任务 5-7 行（按 spec.md §3.1）
  - Error 分支追加 `❌ 失败原因：{truncate}`
- [ ] **T17** 实现 `fn build_scheduled_run_text(run: &ScheduledRun, fetcher_name: &str, config_name: &str, extra: &NotificationExtra) -> String`（D14）：
  - 4 状态：✅ 成功 / ❌ 失败 / 🟡 失败并进入冷却 / 🛑 失败并已自动停用
  - 包含 `📥 拉取/🎯 命中/✍ 写入/⏱ 用时/📁 CSV` 多行
- [ ] **T18** 实现 `fn read_webhook(app_handle: &AppHandle) -> String`：
  - `app_handle.state::<AppState>().app_settings.read().await.feishu_webhook_url.clone()`
  - **注**：因 `tauri::State` 在 `spawn` 内访问需 `Arc`，验证 RwLock 跨线程读法
- [ ] **T19** 实现 `pub fn notify_task_async(app_handle: &AppHandle, task: &Task)`（D9、D10、D13）：
  - clone task & app_handle → `tauri::async_runtime::spawn(async move { ... })`
  - webhook 空 → `log::debug!` 静默跳过
  - 失败 → `log::warn!`（含 `redact_webhook`）
- [ ] **T20** 实现 `pub fn notify_scheduled_run_async(app_handle: &AppHandle, run: &ScheduledRun, fetcher_name: String, config_name: String, extra: NotificationExtra)`：
  - 与 T19 同构；clone 全部入参后 spawn
- [ ] **T21** 单元测试（共 7 项）：
  - `truncate_for_msg`：500/501 字符边界、含中文按 char 不破字
  - `format_duration`：1s / 65s / 3700s / 0s
  - `redact_webhook`：长 URL / 短 URL / 非法 URL 都不 panic
  - `build_task_text`：Completed / Error / Partial 三分支字段齐全
  - `build_scheduled_run_text`：Normal / CooldownTriggered / Paused / Failed 四分支
  - send_feishu_text 不做真实 HTTP 单测（依赖外部服务），仅做参数构造单测（封装一个内部 `build_request_body` 便于测）
  - `AppSettings` Default：feishu_webhook_url == ""

## P3 - 后端入口（main.rs）

- [ ] **T22** 在 `main.rs::AppState` 增加字段：
  ```rust
  pub app_settings: Arc<RwLock<AppSettings>>,
  ```
- [ ] **T23** 修改 `load_data` / `setup`：
  - 启动时读取 `AppData.app_settings` → 注入 `AppState.app_settings`
  - 启动 log 打印 webhook 是否配置（调用 `redact_webhook`，不打印明文）
- [ ] **T24** 新增 Tauri 命令 `pub async fn get_app_settings(state: State<'_, AppState>) -> Result<AppSettings, String>`：
  - 直接克隆 `app_settings.read().await.clone()`
- [ ] **T25** 新增 Tauri 命令 `pub async fn save_app_settings(app: AppHandle, state: State<'_, AppState>, settings: AppSettings) -> Result<(), String>`（D8）：
  - 写入持久化 `write_app_data_settings_only`
  - 更新 `state.app_settings.write().await`
  - 校验 webhook URL（非空时必须 `https://open.feishu.cn/open-apis/bot/v2/hook/` 前缀；空字符串允许）
- [ ] **T26** 新增 Tauri 命令 `pub async fn send_feishu_test_message(state: State<'_, AppState>) -> Result<(), String>`（D7）：
  - 读取当前 webhook；空 → `Err("飞书 webhook 未配置".to_string())`
  - 发送固定文案：`🤖 VideoMixer Pro 测试消息：webhook 配置正确，机器人已就绪`
  - 不走 `notify_*_async`，直接 `await send_feishu_text` 把错误暴露给前端
- [ ] **T27** 在 `tauri::Builder::default().invoke_handler(...)` 注册 3 个命令：
  - `get_app_settings`
  - `save_app_settings`
  - `send_feishu_test_message`

## P4 - 触发点接入

- [ ] **T28** 修改 `src-tauri/src/video_processor.rs`，在合成任务终态写入处调用通知（D1）：
  - **Completed 分支**：line ≈ 2949 `t.status = TaskStatus::Completed;` 之后调用 `notifier::notify_task_async(&app_handle, &t)`
  - **Error 分支**：所有 `t.status = TaskStatus::Error;` 终态之后同步触发
  - **Partial 分支**（如有）：同 Error
  - 验证：每条触发路径只发一次（避免重复）
- [ ] **T29** 修改 `src-tauri/src/scheduler.rs::apply_post_run`，在末尾根据状态分支构造 `NotificationExtra` 并调用 `notify_scheduled_run_async`：
  - 4 分支判定逻辑（按 design.md §3.6）：
    - `!updated.enabled` → `Paused`
    - `cooldown_until.is_some() && consecutive_failures == COOLDOWN_FAILURE_THRESHOLD` → `CooldownTriggered { until }`
    - 其他 → `Normal`
  - 提取 `fetcher_name` / `config_name` 在 spawn 之前 clone

## P5 - 前端

- [ ] **T30** 修改 `src/types.ts` 增加：
  ```ts
  export interface AppSettings {
    feishu_webhook_url: string;
  }
  ```
- [ ] **T31** 新建 `src/components/AdvancedSettings.tsx`：
  - useState 装载 settings；mount 时 `invoke('get_app_settings')`
  - 输入框：feishu_webhook_url（placeholder 示例 URL）
  - 「保存」按钮 → `invoke('save_app_settings', { settings })`
  - 「发送测试消息」按钮 → `invoke('send_feishu_test_message')`
    - 成功 alert / toast：`测试消息已发送，请在群中查收`
    - 失败 alert / toast：`发送失败：${err}`
  - 帮助文案：链接飞书自定义机器人官方文档（外链不强制）
- [ ] **T32** 修改 `src/App.tsx`：
  - 顶部 Tabs 增加 `'advanced'` 项 「高级设置」
  - 路由切换时渲染 `<AdvancedSettings />`
- [ ] **T33** 编辑页面布局：
  - 仅一个 webhook URL 输入框 + 测试按钮 + 保存按钮
  - 输入校验：非空时必须 `https://open.feishu.cn/open-apis/bot/v2/hook/` 前缀（前端先校验，后端复核）
- [ ] **T34** 验证 TypeScript 类型：`npx tsc --noEmit` 无报错

## P6 - 联调与测试

- [ ] **T35** Rust 编译：`cd src-tauri && cargo check` 通过；`cargo test --package video-mixer-pro` 全绿
- [ ] **T36** 前端编译：`npm run build` 通过
- [ ] **T37** 手测脚本（开发期）：
  - 启动 `npm run tauri dev`
  - 「高级设置」配置一个真实 webhook → 测试按钮 → 群中收到消息
  - 触发一次合成任务（小样本）→ 群中收到 Completed 消息
  - 触发一次定时任务（小样本）→ 群中收到成功消息
  - 故意填错 webhook（如多写一个字符）→ 测试按钮显示失败 + log warn
  - 清空 webhook 保存 → 触发任务不发通知（log debug）
- [ ] **T38** 兼容性回归：
  - 删除 webhook 配置后老版本 `app_data.json` 启动正常
  - webhook 网络失败时 spawn future 不阻塞主任务（计时验证主任务完成时间不变）

## P7 - 文档

- [ ] **T39** 更新 `AGENTS.md`：
  - 新增「最近修改记录 v1.0.7」条目，列出 4-6 行关键变更
  - 新增「关键代码位置」表行：`notifier.rs` / `AdvancedSettings.tsx`
  - 「核心功能模块」增加 §6 通知模块简述
- [ ] **T40** OpenSpec 4 件套交付：
  - 确认 `proposal.md` / `spec.md` / `design.md` / `tasks.md` 全部就绪
  - 在交付说明中给出 4 件套绝对路径

## P8 - 提案对照检查（D1-D14 全表回填）

- [ ] **T41** 实施完毕后，对照 proposal.md `D1-D14` 表逐条确认实现位置：
  - D1 通知触发条件 → 落地于 video_processor.rs 3 处 + scheduler.rs apply_post_run 4 分支
  - D2 仅 URL 不签名 → notifier.rs::send_feishu_text 不读签名字段；redact_webhook 用于 log
  - D3 msg_type=text → notifier.rs::send_feishu_text body 构造
  - D4 reqwest 0.12 + rustls-tls → Cargo.toml
  - D5 全局唯一 webhook → AppSettings + AppData.app_settings
  - D6 重试 2 次 5s/15s → BACKOFF_SECS 常量 + 主循环
  - D7 测试按钮 → send_feishu_test_message 命令 + AdvancedSettings.tsx 按钮
  - D8 保存即生效 → save_app_settings 命令同步更新 AppState + 持久化
  - D9 webhook 空时静默 → notify_*_async 内 webhook.is_empty() 早返回
  - D10 失败仅 log::warn → notify_*_async 错误分支
  - D11 #[serde(default)] → AppData / AppSettings 字段
  - D12 单次超时 10s → REQUEST_TIMEOUT_SECS 常量
  - D13 spawn 异步 → notify_*_async 主体
  - D14 中文 + emoji → build_task_text / build_scheduled_run_text
- [ ] **T42** 验收标准（spec.md §8）11 条逐项核对，并在交付说明中列出差异点（如有）
- [ ] **T43** 若实现与提案存在差异，在交付说明显式列出差异点 + 原因 + 是否需要更新提案
