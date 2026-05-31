# 规格：飞书 Webhook 通知（spec.md）

## 1. 数据模型

### 1.1 AppSettings（新增）

```rust
/// 应用全局设置（持久化在 app_data.json 顶层）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AppSettings {
    /// 飞书自定义机器人 webhook URL，为空表示未配置（不发送通知）
    #[serde(default)]
    pub feishu_webhook_url: String,
}
```

### 1.2 AppData 扩展

```rust
pub struct AppData {
    pub configs: Vec<VideoConfig>,
    pub tasks: Vec<Task>,
    pub usage_records: HashMap<String, UsageRecord>,
    /// 全局应用设置（v1.0.7 新增；旧数据文件无此字段时反序列化为 Default）
    #[serde(default)]
    pub app_settings: AppSettings,
}
```

### 1.3 前端类型

```typescript
// types.ts
export interface AppSettings {
  feishu_webhook_url: string;
}
```

## 2. 通知模块（src-tauri/src/notifier.rs）

### 2.1 公共 API

#### `send_feishu_text`
```rust
/// 向飞书自定义机器人发送 text 消息（含重试 2 次、指数退避 5s/15s、单次超时 10s）。
///
/// 参数:
/// - webhook: 飞书 webhook URL（完整含 token）
/// - content: 消息文本（utf-8）
///
/// 返回:
/// - Ok(()): 飞书 API 返回 code=0
/// - Err(String): 重试耗尽后的最终错误描述
pub async fn send_feishu_text(webhook: &str, content: &str) -> Result<(), String>
```

#### `notify_task_async`
```rust
/// 异步派发合成任务完成通知（不阻塞调用方）。
/// 内部调用 spawn(send_feishu_text(...))，失败仅 log::warn。
///
/// 参数:
/// - app_handle: Tauri AppHandle，用于读取 AppState 获取 webhook
/// - task_snapshot: Task 快照（不可变借用必要字段：name / config_name / status / total / completed / failed / started_at / completed_at / error_message）
pub fn notify_task_async(app_handle: &AppHandle, task: &Task)
```

#### `notify_scheduled_run_async`
```rust
/// 异步派发定时任务完成通知；status ∈ { success | failed | cooldown_triggered | paused }
pub fn notify_scheduled_run_async(
    app_handle: &AppHandle,
    run: &ScheduledRun,
    extra: NotificationExtra, // 区分 cooldown / paused 等失败子状态
)
```

### 2.2 文本拼接（中文 + emoji）

#### 合成任务完成
```
🎬 VideoMixer Pro · 视频合成任务完成

📋 配置：{config_name}
🆔 任务：{task_name}
✅ 状态：{Completed | Partial | Error 中文化}
📊 进度：成功 {completed_count} / 失败 {failed_count} / 总计 {total_count}
⏱ 用时：{duration}
📁 输出：{output_folder}

{Error 时附加：}
❌ 失败原因：{error_message 截断 500 字符}
```

#### 定时任务完成
```
⏰ VideoMixer Pro · 定时采集完成

📋 配置：{config_name}
🆔 任务：{fetcher_name} #{run_index}
{✅ 成功 | ❌ 失败 | 🟡 进入冷却 | 🛑 已自动停用}
📥 拉取：{fetched_total}
🎯 命中：{matched_in_window}
✍ 写入：{new_appended}
⏱ 用时：{duration}
📁 CSV：{csv_path or '-'}

{失败时附加：}
❌ 失败原因：{error_message 截断 500 字符}
{cooldown 时附加：}
🟡 冷却到：{cooldown_until 本地时间}
{paused 时附加：}
🛑 已自动停用，请前往配置编辑页查看
```

### 2.3 状态映射

| Task.status | 文案 |
|---|---|
| `Completed` | ✅ 全部成功 |
| `Partial` | ⚠️ 部分成功 |
| `Error` | ❌ 失败 |

| ScheduledRun + extra | 文案 |
|---|---|
| `Success` | ✅ 成功 |
| `Failed` (consecutive < 3) | ❌ 失败 |
| `Failed` + cooldown_triggered | 🟡 失败并进入冷却 |
| `Failed` + paused | 🛑 失败并已自动停用 |

### 2.4 重试与错误处理

```rust
const MAX_ATTEMPTS: u32 = 3;        // 首次 + 2 次重试
const BACKOFF_SECS: [u64; 2] = [5, 15];
const REQUEST_TIMEOUT_SECS: u64 = 10;
const MAX_ERROR_TEXT_LEN: usize = 500;
```

- 首次失败 → sleep 5s → 重试
- 第二次失败 → sleep 15s → 重试
- 第三次失败 → 返回 Err（调用方记录 `log::warn!`）
- 单次请求超时 10s（reqwest 内置）
- 飞书响应 `code != 0` 视为失败（含 token 错误、URL 不存在、限流等）

### 2.5 webhook URL 脱敏

```rust
/// 脱敏 webhook URL 用于日志。仅保留 host + path 末 8 字符。
fn redact_webhook(url: &str) -> String { ... }
// 示例：https://open.feishu.cn/open-apis/bot/v2/hook/12345678abcd → "open.feishu.cn/...12345abcd"
```

## 3. Tauri 命令

### 3.1 `get_app_settings`
```rust
#[tauri::command]
pub fn get_app_settings(state: State<AppState>) -> Result<AppSettings, String>
```
返回当前 `AppState.app_settings` 的副本。

### 3.2 `save_app_settings`
```rust
#[tauri::command]
pub fn save_app_settings(
    app: AppHandle,
    state: State<AppState>,
    settings: AppSettings,
) -> Result<(), String>
```
- trim webhook URL 后写入 `AppState.app_settings`
- 调用 `storage::write_app_data_settings_only(...)` 仅替换 `app_data.json` 中 `app_settings` 字段（避免触发 reconcile）
- 不做格式校验（用户可能粘贴临时 URL 测试）

### 3.3 `send_feishu_test_message`
```rust
#[tauri::command]
pub async fn send_feishu_test_message(state: State<'_, AppState>) -> Result<(), String>
```
- 读 `AppState.app_settings.feishu_webhook_url`
- 若为空 → `Err("尚未配置 webhook URL")`
- 调用 `send_feishu_text(url, "🤖 VideoMixer Pro 测试消息：webhook 配置正确，机器人已就绪").await`
- 错误向上抛给前端 toast

## 4. AppState 扩展

```rust
pub struct AppState {
    pub configs: Arc<RwLock<Vec<VideoConfig>>>,
    pub tasks: Arc<RwLock<Vec<Task>>>,
    pub used_tutorial_videos: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    pub app_data_file: Arc<RwLock<Option<PathBuf>>>,
    pub scheduler: Arc<scheduler::Scheduler>,
    pub yt_dlp_healthy: Arc<RwLock<bool>>,
    /// 全局应用设置（含 feishu webhook）
    pub app_settings: Arc<RwLock<AppSettings>>,
}
```

启动时由 `load_data` 注入；用户保存时同步更新内存 + 落盘。

## 5. 触发点契约

### 5.1 合成任务
- 任务从 `Running` 变为终态 `Completed | Error | Partial` 时**且仅一次**调用 `notifier::notify_task_async(&app, &task)`
- 不在中间步骤（每个 step 完成）发送
- `Paused`、`Pending` 不发送

### 5.2 定时任务
在 `scheduler::apply_post_run` 内调用，按以下分支：

| 状态 | 调用 |
|---|---|
| `outcome.status == Success` | `notify_scheduled_run_async(&app, &run, NotificationExtra::Normal)` |
| `outcome.status == Failed` && `consecutive_failures < 3` | `Failed` |
| `outcome.status == Failed` && `consecutive_failures == 3` | `CooldownTriggered { until }` |
| `outcome.status == Failed` && `consecutive_failures >= 6` | `Paused` |

`NotificationExtra` 是 notifier.rs 内部 enum：

```rust
pub enum NotificationExtra {
    Normal,
    CooldownTriggered { until: chrono::DateTime<Utc> },
    Paused,
}
```

## 6. 前端规格

### 6.1 AdvancedSettings.tsx

字段：
- 标题「⚙️ 高级设置」
- 子标题「飞书机器人通知」
- 输入框 `Webhook URL`（type=text，placeholder=`https://open.feishu.cn/open-apis/bot/v2/hook/xxxxxxxx`）
- 提示文字「填写后，视频合成任务和定时采集任务完成时会自动推送到飞书群。留空则不发送。」
- 按钮组：
  - `[保存]` 主按钮
  - `[发送测试消息]` 次按钮（仅当 webhook 非空时可点）
- 测试结果实时反馈（成功显示绿色 ✅ / 失败显示红色 ❌ + 错误文案）
- 帮助链接小字「如何获取 webhook URL？」（指向飞书官方文档锚点，不开浏览器，仅 title 提示）

### 6.2 App.tsx 接入

```tsx
const [activeTab, setActiveTab] = useState<'configs' | 'tasks' | 'advanced'>('configs');
```

Tabs 增加第三项：
```tsx
<button onClick={() => setActiveTab('advanced')} ...>
  ⚙️ 高级设置
</button>
```

```tsx
{activeTab === 'advanced' && <AdvancedSettings />}
```

## 7. 持久化扩展

### 7.1 `storage.rs::write_app_data_settings_only`

```rust
/// 仅替换 app_data.json 中 app_settings 字段，保留 configs/tasks/usage_records。
/// 不触发 sync_config_store / scheduler.reconcile_after_save。
pub fn write_app_data_settings_only(
    data_file: &Path,
    settings: &AppSettings,
) -> Result<(), String>
```

### 7.2 兼容旧数据

`AppData` 反序列化遇缺失 `app_settings` 字段：`#[serde(default)]` → `AppSettings::default()`（webhook 为空字符串）。

## 8. 验收标准

- [ ] 高级设置 Tab 可见，输入 webhook → 保存 → 重启 App 后值仍存在
- [ ] 「发送测试消息」按钮：填错 URL 时显示红色错误（包含飞书返回的 code 与 msg）
- [ ] 「发送测试消息」按钮：填对 URL 时飞书群收到 `🤖 VideoMixer Pro 测试消息：...`
- [ ] 触发一个合成任务到 Completed → 飞书群收到 `🎬 VideoMixer Pro · 视频合成任务完成 ...`
- [ ] 触发一个合成任务故意失败 → 飞书群收到 `❌ 失败` 文案（含原因）
- [ ] 定时采集 success → 飞书群收到 `⏰ ... ✅ 成功`
- [ ] 定时采集连续 3 次失败 → 飞书群收到 `🟡 失败并进入冷却`
- [ ] 定时采集连续 6 次失败 → 飞书群收到 `🛑 失败并已自动停用`
- [ ] webhook 留空时所有任务正常完成且不发送任何通知（仅 debug 日志）
- [ ] 飞书 webhook 不可达（断网）→ 主任务正常返回，仅 `log::warn` 重试 2 次后放弃
- [ ] 旧版 app_data.json（无 app_settings）首次启动加载不报错，自动初始化空 settings
