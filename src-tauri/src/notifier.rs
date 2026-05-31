/*!
 飞书自定义机器人 Webhook 通知模块。

 - 当 `AppState.app_settings.feishu_webhook_url` 非空时，向飞书机器人推送 `msg_type=text` 消息。
 - 重试策略：最多 3 次（首发 + 2 次退避 5s/15s）；单次请求超时 10s。
 - 失败仅记录 `log::warn` 不阻塞主任务；webhook 未配置时静默 `log::debug`。
 - 使用 `tauri::async_runtime::spawn` 异步派发，主任务不等待通知 future。
*/

use chrono::{DateTime, Utc};
use log::{debug, info, warn};
use serde_json::Value;
use std::time::Duration;
use tauri::{AppHandle, Manager};

use crate::scheduled_fetcher::{RunStatus, ScheduledRun};
use crate::video_processor::{Task, TaskStatus};

/// 最大尝试次数（包含首次发送）。
const MAX_ATTEMPTS: u32 = 3;
/// 失败重试退避（秒）。第 0 次失败后等 5s，第 1 次失败后等 15s。
const BACKOFF_SECS: [u64; 2] = [5, 15];
/// 单次 HTTP 请求超时（秒）。
const REQUEST_TIMEOUT_SECS: u64 = 10;
/// 错误信息文本截断长度（字符数）。
const MAX_ERROR_TEXT_LEN: usize = 500;

/// 定时任务通知的额外语义，用于在 build_scheduled_run_text 中切换分支。
#[derive(Debug, Clone)]
pub enum NotificationExtra {
    /// 普通成功 / 普通失败（未跨过冷却阈值）。
    Normal,
    /// 该次失败刚好把 fetcher 推入冷却（连续失败达到 COOLDOWN_FAILURE_THRESHOLD）。
    CooldownTriggered { until: DateTime<Utc> },
    /// 该次失败导致 fetcher 被自动停用（连续失败达到 PAUSE_FAILURE_THRESHOLD）。
    Paused,
}

/**
 构造飞书 text 消息的请求体。

 参数:
 - `content`: 文本正文（已包含 emoji / 换行）。

 返回:
 - `serde_json::Value`: 形如 `{"msg_type":"text","content":{"text":content}}`。
*/
fn build_request_body(content: &str) -> Value {
    serde_json::json!({
        "msg_type": "text",
        "content": { "text": content }
    })
}

/**
 单次 POST：发送一次请求并解析飞书返回。
 飞书机器人成功响应：HTTP 200 + body `{ "code": 0, ... }` 或 `{ "StatusCode": 0, ... }`。

 参数:
 - `client`: 复用的 reqwest Client。
 - `webhook`: 完整 webhook URL。
 - `body`: 已构造好的 JSON body。

 返回:
 - `Ok(())`: 飞书侧返回 code==0。
 - `Err(String)`: HTTP 失败、解析失败、或 code != 0。
*/
async fn try_post_once(
    client: &reqwest::Client,
    webhook: &str,
    body: &Value,
) -> Result<(), String> {
    let resp = client
        .post(webhook)
        .json(body)
        .send()
        .await
        .map_err(|e| format!("HTTP 请求失败: {}", e))?;
    let status = resp.status();
    let text = resp
        .text()
        .await
        .map_err(|e| format!("读取响应体失败: {}", e))?;
    if !status.is_success() {
        return Err(format!("HTTP 状态码非 2xx：{} body={}", status, text));
    }
    let json: Value = serde_json::from_str(&text)
        .map_err(|e| format!("响应非合法 JSON: {} body={}", e, text))?;
    // 飞书返回字段：code（旧）/ StatusCode（v2 自定义机器人）
    let code = json
        .get("code")
        .and_then(|v| v.as_i64())
        .or_else(|| json.get("StatusCode").and_then(|v| v.as_i64()))
        .unwrap_or(-1);
    if code == 0 {
        Ok(())
    } else {
        let msg = json
            .get("msg")
            .or_else(|| json.get("StatusMessage"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        Err(format!("飞书 API 返回 code={} msg={}", code, msg))
    }
}

/**
 发送一条飞书 text 消息，带指数退避重试。

 参数:
 - `webhook`: 完整 webhook URL（必须以 `https://open.feishu.cn/open-apis/bot/v2/hook/` 开头，由调用方校验）。
 - `content`: 文本正文。

 返回:
 - `Ok(())`: 任意一次尝试成功。
 - `Err(String)`: 所有尝试均失败时的最后一条错误。
*/
pub async fn send_feishu_text(webhook: &str, content: &str) -> Result<(), String> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(REQUEST_TIMEOUT_SECS))
        .build()
        .map_err(|e| format!("构造 HTTP client 失败: {}", e))?;
    let body = build_request_body(content);

    let mut last_err = String::new();
    for attempt in 0..MAX_ATTEMPTS {
        match try_post_once(&client, webhook, &body).await {
            Ok(()) => {
                if attempt > 0 {
                    info!(
                        "飞书通知第 {} 次重试发送成功 (webhook={})",
                        attempt + 1,
                        redact_webhook(webhook)
                    );
                }
                return Ok(());
            }
            Err(e) => {
                last_err = e;
                if attempt + 1 < MAX_ATTEMPTS {
                    let backoff = BACKOFF_SECS[attempt as usize];
                    warn!(
                        "飞书通知发送失败（第 {}/{} 次），{}s 后重试：{}",
                        attempt + 1,
                        MAX_ATTEMPTS,
                        backoff,
                        last_err
                    );
                    tokio::time::sleep(Duration::from_secs(backoff)).await;
                }
            }
        }
    }
    Err(last_err)
}

/**
 对 webhook URL 做日志脱敏：保留 host 段和末 8 字符，中间用省略号代替。

 参数:
 - `url`: 完整 webhook URL（任意合法/非法字符串都不应 panic）。

 返回:
 - `String`: 脱敏后的字符串，便于打 log。
*/
pub fn redact_webhook(url: &str) -> String {
    if url.len() <= 16 {
        return "<empty-or-too-short>".to_string();
    }
    let host = if let Some(rest) = url.strip_prefix("https://") {
        rest.split('/').next().unwrap_or("")
    } else if let Some(rest) = url.strip_prefix("http://") {
        rest.split('/').next().unwrap_or("")
    } else {
        ""
    };
    let tail_len = url.chars().count().min(8);
    let tail: String = url.chars().rev().take(tail_len).collect::<String>().chars().rev().collect();
    if host.is_empty() {
        format!("...{}", tail)
    } else {
        format!("{}/...{}", host, tail)
    }
}

/**
 按字符（非字节）截断长文本，避免破坏多字节中文。

 参数:
 - `s`: 原始文本。

 返回:
 - `String`: 不超过 MAX_ERROR_TEXT_LEN 字符；超出时附加 `...(已截断)`。
*/
fn truncate_for_msg(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= MAX_ERROR_TEXT_LEN {
        s.to_string()
    } else {
        let head: String = chars.iter().take(MAX_ERROR_TEXT_LEN).collect();
        format!("{}...(已截断)", head)
    }
}

/**
 把秒数格式化为人类可读的中文用时。

 参数:
 - `seconds`: 非负秒数（负数会按 0 处理）。

 返回:
 - `String`: `Hh Mm` / `Mm Ss` / `Ss` 三档之一。
*/
fn format_duration(seconds: i64) -> String {
    let s = seconds.max(0);
    if s >= 3600 {
        let h = s / 3600;
        let m = (s % 3600) / 60;
        format!("{}h {}m", h, m)
    } else if s >= 60 {
        let m = s / 60;
        let sec = s % 60;
        format!("{}m {}s", m, sec)
    } else {
        format!("{}s", s)
    }
}

/**
 计算合成任务用时（秒）。优先 completed_at - started_at；缺失任一字段时返回 0。
*/
fn task_duration_secs(task: &Task) -> i64 {
    match (task.started_at, task.completed_at) {
        (Some(s), Some(e)) => (e - s).num_seconds(),
        _ => 0,
    }
}

/**
 计算定时任务用时（秒）。优先 finished_at - started_at；缺失则 Utc::now() - started_at。
*/
fn run_duration_secs(run: &ScheduledRun) -> i64 {
    match run.finished_at {
        Some(f) => (f - run.started_at).num_seconds(),
        None => (Utc::now() - run.started_at).num_seconds(),
    }
}

/**
 把 TaskStatus 映射为带 emoji 的中文文本。
*/
fn task_status_label(status: &TaskStatus) -> &'static str {
    match status {
        TaskStatus::Completed => "✅ 已完成",
        TaskStatus::Error => "❌ 失败",
        TaskStatus::Partial => "⚠️ 部分成功",
        TaskStatus::Running => "▶️ 运行中",
        TaskStatus::Pending => "⏳ 等待中",
        TaskStatus::Paused => "⏸ 已暂停",
    }
}

/**
 拼装合成任务通知文本。

 参数:
 - `task`: 终态任务快照。

 返回:
 - `String`: 多行飞书消息正文。
*/
fn build_task_text(task: &Task) -> String {
    let duration = format_duration(task_duration_secs(task));
    let mut lines = vec![
        "🎬 VideoMixer Pro · 视频合成任务完成".to_string(),
        format!("📋 配置：{}", task.name),
        format!("🆔 任务：{}", task.task_name),
        format!("✅ 状态：{}", task_status_label(&task.status)),
        format!(
            "📊 进度：成功 {} / 失败 {} / 总计 {}",
            task.completed_count, task.failed_count, task.total_count
        ),
        format!("⏱ 用时：{}", duration),
        format!("📁 输出：{}", task.output_folder),
    ];
    if matches!(task.status, TaskStatus::Error | TaskStatus::Partial) {
        if let Some(err) = task.error_message.as_ref() {
            if !err.trim().is_empty() {
                lines.push(format!("❌ 失败原因：{}", truncate_for_msg(err)));
            }
        }
    }
    lines.join("\n")
}

/**
 拼装定时任务通知文本。

 参数:
 - `run`: 已结束的 ScheduledRun 快照。
 - `extra`: 额外语义（Normal / CooldownTriggered / Paused）。

 返回:
 - `String`: 多行飞书消息正文。
*/
fn build_scheduled_run_text(run: &ScheduledRun, extra: &NotificationExtra) -> String {
    let header = match (&run.status, extra) {
        (RunStatus::Success, _) => "⏰ VideoMixer Pro · 定时采集完成（✅ 成功）",
        (RunStatus::Failed, NotificationExtra::CooldownTriggered { .. }) => {
            "⏰ VideoMixer Pro · 定时采集完成（🟡 失败并进入冷却）"
        }
        (RunStatus::Failed, NotificationExtra::Paused) => {
            "⏰ VideoMixer Pro · 定时采集完成（🛑 失败并已自动停用）"
        }
        (RunStatus::Failed, _) => "⏰ VideoMixer Pro · 定时采集完成（❌ 失败）",
        _ => "⏰ VideoMixer Pro · 定时采集完成",
    };
    let duration = format_duration(run_duration_secs(run));
    let mut lines = vec![
        header.to_string(),
        format!("📋 配置：{}", run.config_name),
        format!("🆔 任务：{} #{}", run.fetcher_name, run.run_index),
        format!(
            "📥 拉取 {} / 🎯 命中 {} / ✍ 写入 {}",
            run.fetched_total, run.matched_in_window, run.new_appended
        ),
        format!("⏱ 用时：{}", duration),
    ];
    if let Some(csv) = run.csv_path.as_ref() {
        if !csv.is_empty() {
            lines.push(format!("📁 CSV：{}", csv));
        }
    }
    if matches!(run.status, RunStatus::Failed) {
        if let Some(err) = run.error_message.as_ref() {
            if !err.trim().is_empty() {
                lines.push(format!("❌ 失败原因：{}", truncate_for_msg(err)));
            }
        }
    }
    if let NotificationExtra::CooldownTriggered { until } = extra {
        lines.push(format!(
            "🟡 已触发冷却，恢复时间：{}",
            until.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S")
        ));
    }
    if matches!(extra, NotificationExtra::Paused) {
        lines.push("🛑 该任务已自动停用，请前往配置编辑页排查并重新启用".to_string());
    }
    lines.join("\n")
}

/**
 从 AppState 异步读取当前 webhook URL（克隆字符串）。

 参数:
 - `app_handle`: Tauri AppHandle。

 返回:
 - `String`: webhook URL；未配置或读锁失败时返回空字符串。
*/
fn read_webhook(app_handle: &AppHandle) -> String {
    let state = match app_handle.try_state::<crate::AppState>() {
        Some(s) => s,
        None => return String::new(),
    };
    match state.app_settings.read() {
        Ok(g) => g.feishu_webhook_url.clone(),
        Err(_) => String::new(),
    }
}

/**
 异步发送合成任务终态通知。
 - webhook 为空则 `debug` 跳过；失败仅 `warn`，不阻塞主任务。
 - 内部使用 `tauri::async_runtime::spawn`；调用方无需 await。

 参数:
 - `app_handle`: Tauri AppHandle。
 - `task`: 终态任务快照（会被 clone 进入 spawn）。
*/
pub fn notify_task_async(app_handle: &AppHandle, task: &Task) {
    let snapshot = task.clone();
    let app_handle_cloned = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        let webhook = read_webhook(&app_handle_cloned);
        if webhook.is_empty() {
            debug!("飞书 webhook 未配置，跳过任务通知 task={}", snapshot.task_name);
            return;
        }
        let content = build_task_text(&snapshot);
        match send_feishu_text(&webhook, &content).await {
            Ok(()) => info!("飞书通知发送成功 task={}", snapshot.task_name),
            Err(e) => warn!(
                "飞书通知发送失败 task={} webhook={} err={}",
                snapshot.task_name,
                redact_webhook(&webhook),
                e
