# 设计：飞书 Webhook 通知（design.md）

## 1. 整体架构

```
┌──────────────────────────────────────────────────────────────┐
│  Frontend (React)                                            │
│  ┌──────────────────┐    ┌────────────────────────────┐      │
│  │ App.tsx          │    │ AdvancedSettings.tsx       │      │
│  │  tabs: configs/  │───▶│  - 输入 webhook URL        │      │
│  │   tasks/advanced │    │  - 保存按钮                │      │
│  └──────────────────┘    │  - 发送测试消息按钮        │      │
│                          └────────────────────────────┘      │
│                                    │ invoke                  │
│                                    ▼                         │
│   get_app_settings / save_app_settings / send_test_message   │
└──────────────────────────────────────────────────────────────┘
                                    │
                                    ▼
┌──────────────────────────────────────────────────────────────┐
│  Backend (Rust)                                              │
│  ┌──────────────────┐    ┌────────────────────────────┐      │
│  │ AppState         │    │ commands.rs (new)          │      │
│  │  app_settings:   │◀──▶│  get_app_settings          │      │
│  │   Arc<RwLock<…>> │    │  save_app_settings         │      │
│  └──────────────────┘    │  send_feishu_test_message  │      │
│           │              └────────────────────────────┘      │
│           │ read snapshot                                    │
│           ▼                                                  │
│  ┌──────────────────────────────────────────────────────┐    │
│  │ notifier.rs (new)                                    │    │
│  │  send_feishu_text(webhook, content) -> Result        │    │
│  │  notify_task_async(app, task)                        │    │
│  │  notify_scheduled_run_async(app, run, extra)         │    │
│  │   ── reqwest POST + retry 2 (5s/15s) + timeout 10s   │    │
│  └──────────────────────────────────────────────────────┘    │
│           ▲                ▲                                 │
│           │                │                                 │
│   video_processor.rs    scheduler.rs                         │
│   (合成任务终态)        (apply_post_run)                     │
└──────────────────────────────────────────────────────────────┘
```

## 2. 关键设计点

### 2.1 异步派发不阻塞主任务（D13）

通知必须**完全异步**，主任务（合成 / 定时）的返回路径不应等待飞书 API 响应。

```rust
// notifier.rs
pub fn notify_task_async(app_handle: &AppHandle, task: &Task) {
    let snapshot = task.clone();          // 复制必要字段
    let app_handle_cloned = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        let webhook = read_webhook(&app_handle_cloned);
        if webhook.is_empty() {
            log::debug!("飞书 webhook 未配置，跳过通知");
            return;
        }
        let content = build_task_text(&snapshot);
        match send_feishu_text(&webhook, &content).await {
            Ok(()) => log::info!("飞书通知发送成功 task={}", snapshot.task_name),
            Err(e) => log::warn!("飞书通知发送失败 task={} err={}", snapshot.task_name, e),
        }
    });
}
```

要点：
- `task.clone()` 避免长生命周期借用
- 不返回 future，调用方一行调用即用即抛
- webhook 为空在 spawn 后立即检查并 early return（轻量；避免提前触发不必要的 spawn 也行，但放在内部更易测试）

### 2.2 重试逻辑（D6）

```rust
async fn send_feishu_text(webhook: &str, content: &str) -> Result<(), String> {
    let body = serde_json::json!({
        "msg_type": "text",
        "content": { "text": content },
    });
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("构造 HTTP client 失败: {e}"))?;

    let mut last_err: String = String::new();
    for attempt in 0..MAX_ATTEMPTS {
        match try_post_once(&client, webhook, &body).await {
            Ok(()) => return Ok(()),
            Err(e) => {
                last_err = e;
                if attempt + 1 < MAX_ATTEMPTS {
                    let backoff = BACKOFF_SECS[attempt as usize];
                    log::warn!(
                        "飞书通知第 {} 次失败，{}s 后重试 webhook={} err={}",
                        attempt + 1, backoff, redact_webhook(webhook), last_err
                    );
                    tokio::time::sleep(Duration::from_secs(backoff)).await;
                }
            }
        }
    }
    Err(last_err)
}

async fn try_post_once(
    client: &reqwest::Client,
    webhook: &str,
    body: &serde_json::Value,
) -> Result<(), String> {
    let resp = client
        .post(webhook)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("HTTP 请求失败: {e}"))?;
    let status = resp.status();
    let json: serde_json::Value = resp.json().await
        .map_err(|e| format!("响应解析失败 status={status} err={e}"))?;
    let code = json.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code == 0 {
        Ok(())
    } else {
        let msg = json.get("msg").and_then(|v| v.as_str()).unwrap_or("");
        Err(format!("飞书 API 返回 code={code} msg={msg}"))
    }
}
```

### 2.3 webhook 读取契约

```rust
fn read_webhook(app_handle: &AppHandle) -> String {
    let state = app_handle.state::<AppState>();
    let guard = state.app_settings.read().expect("app_settings 锁中毒");
    guard.feishu_webhook_url.trim().to_string()
}
```

调用线程为 spawn 出来的 tokio 任务，使用 `RwLock` 同步读取（飞书通知发送频率远低于读锁，无性能问题）。

### 2.4 错误文本截断

```rust
const MAX_ERROR_TEXT_LEN: usize = 500;

fn truncate_for_msg(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= MAX_ERROR_TEXT_LEN {
        s.to_string()
    } else {
        let head: String = chars.iter().take(MAX_ERROR_TEXT_LEN).collect();
        format!("{head}...(已截断)")
    }
}
```

按字符数截断（而非字节）以避免中文字符切碎。

### 2.5 用时计算（duration）

```rust
fn format_duration(start: &Option<String>, end: &Option<String>) -> String {
    match (start.as_deref(), end.as_deref()) {
        (Some(s), Some(e)) => {
            // 都是 ISO 8601 字符串
            if let (Ok(s_dt), Ok(e_dt)) = (
                chrono::DateTime::parse_from_rfc3339(s),
                chrono::DateTime::parse_from_rfc3339(e),
            ) {
                let secs = (e_dt - s_dt).num_seconds();
                if secs < 60 { format!("{secs}秒") }
                else if secs < 3600 { format!("{}分{}秒", secs / 60, secs % 60) }
                else { format!("{}时{}分", secs / 3600, (secs % 3600) / 60) }
            } else { "-".to_string() }
        }
        _ => "-".to_string(),
    }
}
```

### 2.6 触发点改造

#### 合成任务（video_processor.rs）

在 task.status 写入终态后立刻触发：

```rust
// 现有代码：
t.status = TaskStatus::Completed;
// 新增：
notifier::notify_task_async(&app_handle, t);
```

需要传入 `app_handle: AppHandle`。video_processor.rs 已有 AppHandle 上下文（emit 进度事件用），直接复用。

类似地处理 Error / Partial 分支。

⚠ **关键约束：仅在状态变迁的瞬间触发**——通过将 notifier 调用放在 `t.status = ...;` 紧后一行，且只出现在原本就是终态写入处的 3 个分支（Completed / Error / Partial），保证不会重复发送。

#### 定时任务（scheduler.rs）

`apply_post_run` 末尾按状态分支调用：

```rust
async fn apply_post_run(self: &Arc<Self>, /* ... */ updated: ScheduledFetcher, run: ScheduledRun) {
    // ... 现有失败计数 / cooldown / pause 处理 ...

    let extra = if !updated.enabled {
        notifier::NotificationExtra::Paused
    } else if let Some(until) = updated.cooldown_until {
        if updated.consecutive_failures == COOLDOWN_FAILURE_THRESHOLD {
            // 刚刚进入冷却（首次触发）
            notifier::NotificationExtra::CooldownTriggered { until }
        } else {
            notifier::NotificationExtra::Normal
        }
    } else {
        notifier::NotificationExtra::Normal
    };

    notifier::notify_scheduled_run_async(&app_handle, &run, extra);
}
```

**Idempotence**：`apply_post_run` 在每次 run 完成后只被调用一次，因此通知不会重复。冷却 / 停用边界恰好在 `consecutive_failures` 跨过阈值的那一次发送一次（与 emit 事件时机一致）。

### 2.7 配置生效（D8）

`save_app_settings` 命令直接更新 `Arc<RwLock<AppSettings>>`，下一次通知任务读取时即可获得新值。**运行中的任务不打断**——重试中的请求继续使用旧 webhook 直到 future 完成，符合"保存即生效"语义但避免抖动。

### 2.8 配置 Webhook 校验（保存时）

`save_app_settings` 不做严格 URL 校验（用户可能粘贴 https / 完整 / 不完整 URL 进行测试），只做最小 trim。前端在「发送测试消息」按钮触发前可做客户端基础校验：
- 非空（必填）
- 以 `https://` 开头
- 包含 `feishu` 或 `larksuite`（提示但不阻拦）

## 3. 前端组件设计

### 3.1 AdvancedSettings.tsx 状态机

```typescript
const [webhook, setWebhook] = useState('');
const [savedWebhook, setSavedWebhook] = useState(''); // 用于显示"未保存"提示
const [testStatus, setTestStatus] = useState<'idle' | 'sending' | 'success' | 'error'>('idle');
const [testError, setTestError] = useState<string | null>(null);
```

挂载时：`invoke('get_app_settings')` → 设 `webhook` + `savedWebhook`。

保存：`invoke('save_app_settings', { settings: { feishu_webhook_url: webhook } })` → 更新 `savedWebhook` + Toast。

测试：`testStatus='sending'` → `invoke('send_feishu_test_message')` → 成功 `success` / 失败 `error` + 错误文案 → 5s 后回 idle。

测试按钮在「未保存修改」状态下提示用户「请先保存再测试」。

### 3.2 暴露 webhook 编辑入口（IID 失效 Toast 联动）

不依赖。但保持 App.tsx 的 IID 失效 Toast 自动跳转的现有逻辑不变。

## 4. 边界情况与决策

| 情况 | 处理 |
|---|---|
| webhook URL 含 query param（标准格式） | 按 URL 原样传递；reqwest 会处理 |
| webhook URL 末尾带 `/` 或 `?` | 不做规范化（让飞书侧返回错误用户自行修正）|
| 用户在测试期间快速点多次按钮 | 前端 `testStatus === 'sending'` 时禁用按钮 |
| 测试时间长（>10s） | 前端不做额外超时；reqwest 内置 10s 超时 |
| 任务在通知重试期间被前端取消 | 通知 future 仍跑完（不影响主流程）|
| 用户保存空字符串 | 视为"取消通知"，下次任务完成静默跳过 |
| 同一任务多次进入终态写入分支（理论不应发生） | 通过代码审计保证：每个 status 字段写入终态后只调用一次 notify_task_async |
| reqwest 在 macOS 触发 root cert 问题 | 选用 `rustls-tls` feature 而非默认 native-tls |
| `tauri::async_runtime::spawn` 在 setup 之前调用 | 通知触发点都在 App 已 manage(state) 之后，不存在；notifier 内 read_webhook 用 `app_handle.state::<AppState>()` 时 state 必然已注入 |

## 5. 测试矩阵

### 5.1 单元测试（src-tauri/src/notifier.rs::tests）

- `truncate_for_msg_should_handle_chinese_chars` — 中文截断 500 字符不破字
- `truncate_for_msg_should_keep_short_text_unchanged` — 短文本原样返回
- `redact_webhook_should_keep_host_and_tail` — 脱敏：保留 host + 末 8 字符
- `format_duration_should_handle_seconds_and_minutes_and_hours` — 三档边界
- `format_duration_should_return_dash_when_missing` — 缺字段
- `build_task_text_should_include_completed_emoji` — 任务文案完整性（Completed/Partial/Error 三状态）
- `build_scheduled_run_text_should_handle_cooldown_and_paused` — 定时任务文案（4 种 extra）

### 5.2 集成验收（手测）

按 spec.md §8 验收清单。

## 6. 性能与资源

- reqwest Client 每次 send 重新构造（开销忽略，单次请求）
- 重试期间最长阻塞通知 future ≈ 10 + 5 + 10 + 15 + 10 = 50s（最坏情况）
- 主任务不感知，无性能影响
- 无连接复用（不在乎，频率极低）

## 7. 安全考虑

- webhook URL 含 token，不写入 console.log；后端日志使用 `redact_webhook`
- 测试消息文案固定、不含敏感数据
- 不在 emit 事件中携带 webhook URL
- 前端输入框 type=text 而非 password（用户需要肉眼校对）；如需进一步可加切换显示/隐藏（本提案不实现）
