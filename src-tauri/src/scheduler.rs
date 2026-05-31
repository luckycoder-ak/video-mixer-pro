/*!
 定时 TikTok 元数据采集（CronFetcher）调度器。

 - 每个 ScheduledFetcher 由独立的 tokio 任务持有，仅在 App 运行期生效。
 - 全局 `run_lock` 强制所有 fetcher 串行执行（提案 D8 防封禁策略）。
 - 失败状态机：连续 3 次失败 → 冷却 1h；连续 6 次失败 → 自动停用。
 - 重启后 run_index 计数清零；scheduled_runs.json 中的历史记录保留。
*/

use chrono::{Duration as ChronoDuration, Local, Utc};
use log::{info, warn};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::Mutex as AsyncMutex;
use tokio::task::JoinHandle;
use tokio::time::{sleep, Duration as TokioDuration};

use crate::config::VideoConfig;
use crate::scheduled_fetcher::{
    run_once, RunOutcome, RunStatus, RunTrigger, ScheduledFetcher, ScheduledRun,
};
use crate::storage;

/// 失败 → 冷却阈值（次）。
const COOLDOWN_FAILURE_THRESHOLD: u32 = 3;
/// 失败 → 自动停用阈值（次）。
const PAUSE_FAILURE_THRESHOLD: u32 = 6;
/// 冷却时长（小时）。
const COOLDOWN_HOURS: i64 = 1;
/// 调度循环的 tick 步长（秒）。
const TICK_INTERVAL_SECS: u64 = 60;
/// 多个 fetcher 串行执行时的错峰间隔（秒）。
const SERIAL_GAP_SECS: u64 = 5;

/**
 调度器：管理所有运行期 fetcher 的生命周期与失败状态机。
*/
pub struct Scheduler {
    /// fetcher_id → JoinHandle，用于取消。
    tasks: AsyncMutex<HashMap<String, JoinHandle<()>>>,
    /// fetcher_id → 进程内累计执行序号（重启清零）。
    run_counters: AsyncMutex<HashMap<String, Arc<AtomicU32>>>,
    /// 串行锁：保证同一时刻只有一个 fetcher 在执行 yt-dlp。
    run_lock: Arc<AsyncMutex<()>>,
}

impl Scheduler {
    /**
     新建一个空的调度器。

     返回:
     - `Arc<Scheduler>`: 可在 AppState 中共享。
    */
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            tasks: AsyncMutex::new(HashMap::new()),
            run_counters: AsyncMutex::new(HashMap::new()),
            run_lock: Arc::new(AsyncMutex::new(())),
        })
    }

    /**
     注册一个 fetcher 的调度循环。

     参数:
     - `fetcher`: 任务配置快照（spawn 内部不再回读最新配置，由 reconcile 负责差异处理）。
     - `config_id` / `config_name`: 所属配置上下文。
     - `app_handle`: 用于 emit 事件与读取 AppState。

     返回:
     - 无；spawn 一个 tokio 任务并将其句柄登记到 `tasks`。
    */
    pub async fn register(
        self: &Arc<Self>,
        fetcher: ScheduledFetcher,
        config_id: String,
        config_name: String,
        app_handle: AppHandle,
    ) {
        if !fetcher.enabled {
            info!("Scheduler: fetcher {} 已停用，跳过注册", fetcher.id);
            return;
        }
        // 已存在则先取消旧任务，避免重复
        self.cancel(&fetcher.id).await;

        let counter = {
            let mut counters = self.run_counters.lock().await;
            counters
                .entry(fetcher.id.clone())
                .or_insert_with(|| Arc::new(AtomicU32::new(0)))
                .clone()
        };

        let run_lock = self.run_lock.clone();
        let scheduler = self.clone();
        let fetcher_id = fetcher.id.clone();

        let handle = tokio::spawn(async move {
            scheduler_loop(fetcher, config_id, config_name, app_handle, counter, run_lock).await;
        });

        let mut tasks = self.tasks.lock().await;
        tasks.insert(fetcher_id, handle);
    }

    /**
     取消并清理一个 fetcher 的调度任务。

     参数:
     - `fetcher_id`: 任务 ID。

     返回:
     - 无。即使任务不存在也不会报错。
    */
    pub async fn cancel(&self, fetcher_id: &str) {
        let mut tasks = self.tasks.lock().await;
        if let Some(handle) = tasks.remove(fetcher_id) {
            handle.abort();
            info!("Scheduler: 已取消 fetcher {}", fetcher_id);
        }
        // run_counters 不在此移除，保留计数避免误删后立刻 register 又从 1 起
    }

    /**
     立即触发一次手动测试运行（不修改调度计数；仍参与全局串行锁）。

     参数:
     - `fetcher`: 当前最新的 fetcher 快照。
     - `config_id` / `config_name`: 配置上下文。
     - `app_handle`: 用于 emit 与读取 AppState。

     返回:
     - `Ok(String)`: 新建 run 的 id。
     - `Err(String)`: 错误信息。
    */
    pub async fn run_now(
        self: &Arc<Self>,
        fetcher: ScheduledFetcher,
        config_id: String,
        config_name: String,
        app_handle: AppHandle,
    ) -> Result<String, String> {
        let counter = {
            let mut counters = self.run_counters.lock().await;
            counters
                .entry(fetcher.id.clone())
                .or_insert_with(|| Arc::new(AtomicU32::new(0)))
                .clone()
        };
        let run_index = counter.fetch_add(1, Ordering::SeqCst) + 1;
        let run_lock = self.run_lock.clone();
        let scheduler = self.clone();
        let _guard = run_lock.lock().await;

        let outcome = execute_one_run(
            &fetcher,
            run_index,
            RunTrigger::Manual,
            &config_id,
            &config_name,
            app_handle.clone(),
        )
        .await;

        let run_id = outcome.run.id.clone();
        scheduler
            .apply_post_run(&fetcher, &config_id, outcome, &app_handle)
            .await;
        Ok(run_id)
    }

    /**
     启动期：从 AppState 已加载的 configs 中全量注册 enabled=true 的 fetchers。

     参数:
     - `app_handle`: Tauri AppHandle。

     返回:
     - 无。
    */
    pub async fn boot_from_app_data(self: &Arc<Self>, app_handle: AppHandle) {
        let snapshot: Vec<(String, String, ScheduledFetcher)> = {
            let state = app_handle.state::<crate::AppState>();
            let configs = match state.configs.read() {
                Ok(g) => g,
                Err(_) => return,
            };
            let mut snap = Vec::new();
            for c in configs.iter() {
                for f in &c.scheduled_fetchers {
                    if f.enabled {
                        snap.push((c.id.clone(), c.name.clone(), f.clone()));
                    }
                }
            }
            snap
        };
        info!("Scheduler: boot 注册 {} 个 fetcher", snapshot.len());
        for (config_id, config_name, fetcher) in snapshot {
            self.register(fetcher, config_id, config_name, app_handle.clone())
                .await;
        }
    }

    /**
     save_configs 后调用：根据新配置 diff 出 register / cancel 集合并应用。

     参数:
     - `new_configs`: 持久化后的 configs 快照。
     - `app_handle`: 用于 register 时使用。

     返回:
     - 无。
    */
    pub async fn reconcile_after_save(
        self: &Arc<Self>,
        new_configs: &[VideoConfig],
        app_handle: AppHandle,
    ) {
        // 收集新配置中 enabled=true 的所有 fetcher
        let mut desired: HashMap<String, (String, String, ScheduledFetcher)> = HashMap::new();
        for c in new_configs {
            for f in &c.scheduled_fetchers {
                if f.enabled {
                    desired.insert(f.id.clone(), (c.id.clone(), c.name.clone(), f.clone()));
                }
            }
        }
        // 取消不再期望存在的任务
        let to_cancel: Vec<String> = {
            let tasks = self.tasks.lock().await;
            tasks
                .keys()
                .filter(|k| !desired.contains_key(*k))
                .cloned()
                .collect()
        };
        for id in to_cancel {
            self.cancel(&id).await;
        }
        // 注册（或重注册）当前所有期望存在的任务
        for (_id, (config_id, config_name, fetcher)) in desired {
            self.register(fetcher, config_id, config_name, app_handle.clone())
                .await;
        }
    }

    /**
     单次执行结束后：更新失败计数 / 冷却 / 停用，并把 ScheduledRun 持久化与广播。
    */
    async fn apply_post_run(
        self: &Arc<Self>,
        fetcher: &ScheduledFetcher,
        config_id: &str,
        outcome: RunOutcome,
        app_handle: &AppHandle,
    ) {
        // 持久化 run 记录
        if let Err(e) = storage::upsert_scheduled_run(config_id, outcome.run.clone()) {
            warn!("Scheduler: 持久化 run 失败 {}", e);
        }
        // 广播事件
        let _ = app_handle.emit("scheduled-run-update", &outcome.run);

        // 计算并更新 fetcher 状态：仅在 final 状态触发
        let is_failure = matches!(outcome.run.status, RunStatus::Failed);
        let mut updated = fetcher.clone();
        if is_failure {
            updated.consecutive_failures = updated.consecutive_failures.saturating_add(1);
        } else if matches!(outcome.run.status, RunStatus::Success) {
            updated.consecutive_failures = 0;
            updated.cooldown_until = None;
        }

        let mut should_emit_cooldown = false;
        let mut should_emit_paused = false;

        if is_failure {
            if updated.consecutive_failures == COOLDOWN_FAILURE_THRESHOLD {
                updated.cooldown_until =
                    Some(Utc::now() + ChronoDuration::hours(COOLDOWN_HOURS));
                should_emit_cooldown = true;
            }
            if updated.consecutive_failures >= PAUSE_FAILURE_THRESHOLD {
                updated.enabled = false;
                should_emit_paused = true;
            }
        }
        updated.updated_at = Utc::now();

        // 写回 AppState 中的 configs，并 emit 状态变更
        if let Err(e) = persist_fetcher_update(app_handle, config_id, &updated).await {
            warn!("Scheduler: 持久化 fetcher 更新失败 {}", e);
        }

        if outcome.iid_invalid {
            let _ = app_handle.emit(
                "scheduled-fetcher-iid-invalid",
                serde_json::json!({
                    "fetcher_id": updated.id,
                    "config_id": config_id,
                    "fetcher_name": updated.name,
                }),
            );
        }
        if should_emit_cooldown {
            let _ = app_handle.emit(
                "scheduled-fetcher-cooldown",
                serde_json::json!({
                    "fetcher_id": updated.id,
                    "config_id": config_id,
                    "cooldown_until": updated.cooldown_until,
                }),
            );
        }
        if should_emit_paused {
            let _ = app_handle.emit(
                "scheduled-fetcher-paused",
                serde_json::json!({
                    "fetcher_id": updated.id,
                    "config_id": config_id,
                    "consecutive_failures": updated.consecutive_failures,
                }),
            );
            // 自动停用：取消调度循环
            self.cancel(&updated.id).await;
        }
    }
}

/**
 单 fetcher 调度主循环。

 - tick 步长 60s；每次醒来检查 enabled / cooldown_until / 距上次执行的间隔是否达成。
 - 命中条件 → 抢全局 run_lock → 执行 run_once → 更新状态。
 - 任务被 abort 时整个 future drop，不会再触发新的 tick。
*/
async fn scheduler_loop(
    initial_fetcher: ScheduledFetcher,
    config_id: String,
    config_name: String,
    app_handle: AppHandle,
    counter: Arc<AtomicU32>,
    run_lock: Arc<AsyncMutex<()>>,
) {
    let mut last_run_at: Option<chrono::DateTime<Utc>> = None;

    loop {
        // 总是从最新的 AppState 读取配置；如不存在则退出循环
        let fetcher_opt = read_latest_fetcher(&app_handle, &initial_fetcher.id);
        let fetcher = match fetcher_opt {
            Some(f) => f,
            None => {
                info!(
                    "Scheduler: fetcher {} 已不存在，退出循环",
                    initial_fetcher.id
                );
                return;
            }
        };
        if !fetcher.enabled {
            info!("Scheduler: fetcher {} 已停用，退出循环", fetcher.id);
            return;
        }
        // 冷却中：直接睡到 cooldown_until 或 tick
        if let Some(until) = fetcher.cooldown_until {
            let now = Utc::now();
            if until > now {
                let secs = (until - now).num_seconds().max(1) as u64;
                let nap = secs.min(TICK_INTERVAL_SECS);
                sleep(TokioDuration::from_secs(nap)).await;
                continue;
            }
        }
        // 执行间隔判断
        let interval_secs = (fetcher.interval_days as i64) * 86_400
            + (fetcher.interval_hours as i64) * 3_600;
        let interval_secs = interval_secs.max(3_600); // spec：最小 1 小时
        let due = match last_run_at {
            None => true,
            Some(prev) => (Utc::now() - prev).num_seconds() >= interval_secs,
        };
        if !due {
            sleep(TokioDuration::from_secs(TICK_INTERVAL_SECS)).await;
            continue;
        }

        // 抢锁串行执行
        let _guard = run_lock.lock().await;
        let run_index = counter.fetch_add(1, Ordering::SeqCst) + 1;
        // 多 fetcher 错峰
        sleep(TokioDuration::from_secs(SERIAL_GAP_SECS)).await;

        let outcome = execute_one_run(
            &fetcher,
            run_index,
            RunTrigger::Scheduled,
            &config_id,
            &config_name,
            app_handle.clone(),
        )
        .await;

        last_run_at = Some(Utc::now());
        // 释放锁后再做持久化与状态机更新
        drop(_guard);

        let scheduler = match app_handle.try_state::<crate::AppState>() {
            Some(s) => s.scheduler.clone(),
            None => return,
        };
        scheduler
            .apply_post_run(&fetcher, &config_id, outcome, &app_handle)
            .await;
    }
}

/**
 在线程内调用 run_once 并把 emit_update 回调路由到 Tauri Event 与 storage.upsert。
*/
async fn execute_one_run(
    fetcher: &ScheduledFetcher,
    run_index: u32,
    trigger: RunTrigger,
    config_id: &str,
    config_name: &str,
    app_handle: AppHandle,
) -> RunOutcome {
    let fetcher_clone = fetcher.clone();
    let config_id_owned = config_id.to_string();
    let config_name_owned = config_name.to_string();
    let app_for_callback = app_handle.clone();
    let trigger_clone = trigger.clone();

    // run_once 是阻塞调用（内部包含 std::process::Command::output），用 spawn_blocking 隔离
    let cb_config_id = config_id_owned.clone();
    let cb_config_name = config_name_owned.clone();
    tokio::task::spawn_blocking(move || {
        let now_ts = Utc::now().timestamp();
        let started_local = Local::now();
        run_once(
            &fetcher_clone,
            run_index,
            trigger_clone,
            &cb_config_id,
            &cb_config_name,
            now_ts,
            started_local,
            |run: &ScheduledRun| {
                let _ = app_for_callback.emit("scheduled-run-update", run);
                let _ = storage::upsert_scheduled_run(&cb_config_id, run.clone());
            },
        )
    })
    .await
    .unwrap_or_else(|e| {
        // spawn_blocking panic 兜底
        let mut run = build_failed_run(fetcher, run_index, trigger, config_id, config_name);
        run.error_message = Some(format!("调度内部异常：{}", e));
        RunOutcome { run, iid_invalid: false }
    })
}

/**
 在 panic 兜底场景下构造一个 status=Failed 的 run。
*/
fn build_failed_run(
    fetcher: &ScheduledFetcher,
    run_index: u32,
    trigger: RunTrigger,
    config_id: &str,
    config_name: &str,
) -> ScheduledRun {
    let now = Utc::now();
    ScheduledRun {
        id: uuid::Uuid::new_v4().to_string(),
        fetcher_id: fetcher.id.clone(),
        fetcher_name: fetcher.name.clone(),
        config_id: config_id.to_string(),
        config_name: config_name.to_string(),
        run_index,
        trigger,
        status: RunStatus::Failed,
        started_at: now,
        finished_at: Some(now),
        fetched_total: 0,
        matched_in_window: 0,
        new_appended: 0,
        csv_path: None,
        error_message: None,
        progress_message: "调度内部异常".to_string(),
    }
}

/**
 从 AppState 读取最新的 fetcher 快照（每个 tick 都重新读，确保配置变更立即生效）。

 返回:
 - `Some(ScheduledFetcher)`: 仍存在且未被删除。
 - `None`: 已被用户删除。
*/
fn read_latest_fetcher(app_handle: &AppHandle, fetcher_id: &str) -> Option<ScheduledFetcher> {
    let state = app_handle.state::<crate::AppState>();
    let configs = state.configs.read().ok()?;
    for c in configs.iter() {
        for f in &c.scheduled_fetchers {
            if f.id == fetcher_id {
                return Some(f.clone());
            }
        }
    }
    None
}

/**
 把 fetcher 的 consecutive_failures / cooldown_until / enabled / updated_at 写回 configs 并持久化。

 注意：本函数仅修改内存中的 AppState.configs 与磁盘 app_data.json，不会再次触发
 `reconcile_after_save`，避免在调度循环内对自身造成 cancel/重注册抖动。
*/
async fn persist_fetcher_update(
    app_handle: &AppHandle,
    config_id: &str,
    updated: &ScheduledFetcher,
) -> Result<(), String> {
    let app_handle = app_handle.clone();
    let config_id = config_id.to_string();
    let updated = updated.clone();
    tokio::task::spawn_blocking(move || -> Result<(), String> {
        let state = app_handle.state::<crate::AppState>();
        // 1) 修改内存
        {
            let mut configs = state
                .configs
                .write()
                .map_err(|_| "configs 写锁获取失败".to_string())?;
            if let Some(c) = configs.iter_mut().find(|c| c.id == config_id) {
                if let Some(f) = c
                    .scheduled_fetchers
                    .iter_mut()
                    .find(|f| f.id == updated.id)
                {
                    *f = updated.clone();
                }
            }
        }
        // 2) 同步落盘 app_data.json（沿用现有 save_data 路径）
        let data_file = storage::resolve_app_data_file_path()?;
        let configs_snapshot = {
            let guard = state
                .configs
                .read()
                .map_err(|_| "configs 读锁获取失败".to_string())?;
            guard.clone()
        };
        let tasks_snapshot = {
            let guard = state
                .tasks
                .read()
                .map_err(|_| "tasks 读锁获取失败".to_string())?;
            guard.clone()
        };
        storage::write_app_data_file_only(&data_file, &configs_snapshot, &tasks_snapshot)?;
        Ok(())
    })
    .await
    .map_err(|e| format!("spawn_blocking 失败: {}", e))?
}

// =============================================================================
// Tauri Commands
// =============================================================================

/**
 列出指定 configs 下的所有 ScheduledRun，自动按 started_at 倒序。

 参数:
 - `config_ids`: 可选过滤集合；为 None / 空时返回全部 configs 的 runs。

 返回:
 - `Ok(Vec<ScheduledRun>)`: 合并后的 run 列表。
 - `Err(String)`: 任意 config 读取失败。
*/
#[tauri::command]
pub async fn list_scheduled_runs(
    state: tauri::State<'_, crate::AppState>,
    config_ids: Option<Vec<String>>,
) -> Result<Vec<ScheduledRun>, String> {
    let target_ids: Vec<String> = match config_ids {
        Some(ids) if !ids.is_empty() => ids,
        _ => {
            let configs = state
                .configs
                .read()
                .map_err(|_| "configs 读锁获取失败".to_string())?;
            configs.iter().map(|c| c.id.clone()).collect()
        }
    };
    let mut all = Vec::new();
    for cid in target_ids {
        match storage::load_scheduled_runs(&cid) {
            Ok(rs) => all.extend(rs),
            Err(e) => log::warn!("list_scheduled_runs: {} -> {}", cid, e),
        }
    }
    all.sort_by(|a, b| b.started_at.cmp(&a.started_at));
    Ok(all)
}

/**
 触发一次手动 run（测试按钮路径）。

 参数:
 - `fetcher_id`: 目标任务 ID。

 返回:
 - `Ok(String)`: 新建 run 的 id（前端可据此订阅事件）。
 - `Err(String)`: fetcher 不存在或调度器拒绝。
*/
#[tauri::command]
pub async fn trigger_scheduled_fetcher_test(
    app: AppHandle,
    state: tauri::State<'_, crate::AppState>,
    fetcher_id: String,
) -> Result<String, String> {
    // 健康检查
    let healthy = *state
        .yt_dlp_healthy
        .read()
        .map_err(|_| "yt_dlp_healthy 读锁失败".to_string())?;
    if !healthy {
        return Err("yt-dlp 不可用，请先安装 sidecar 或检查打包".to_string());
    }
    let (config_id, config_name, fetcher) = {
        let configs = state
            .configs
            .read()
            .map_err(|_| "configs 读锁失败".to_string())?;
        let mut found = None;
        for c in configs.iter() {
            for f in &c.scheduled_fetchers {
                if f.id == fetcher_id {
                    found = Some((c.id.clone(), c.name.clone(), f.clone()));
                    break;
                }
            }
            if found.is_some() {
                break;
            }
        }
        found.ok_or_else(|| format!("未找到 fetcher_id={}", fetcher_id))?
    };
    let scheduler = state.scheduler.clone();
    scheduler
        .run_now(fetcher, config_id, config_name, app)
        .await
}

/**
 在系统文件管理器中打开 CSV 所在文件夹（macOS Finder / Windows Explorer / Linux xdg-open）。

 参数:
 - `csv_path`: CSV 文件绝对路径。

 返回:
 - `Ok(())`: 命令已派发。
 - `Err(String)`: 路径不存在或系统命令失败。
*/
#[tauri::command]
pub fn open_csv_in_finder(csv_path: String) -> Result<(), String> {
    let path = std::path::Path::new(&csv_path);
    if !path.exists() {
        return Err(format!("CSV 文件不存在：{}", csv_path));
    }
    let parent = path
        .parent()
        .ok_or_else(|| "无法解析 CSV 文件父目录".to_string())?;
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    #[cfg(target_os = "windows")]
    {
        let mut cmd = std::process::Command::new("explorer.exe");
        crate::video_processor::apply_hidden_process_startup(&mut cmd);
        cmd.arg(format!("/select,{}", path.display()))
            .spawn()
            .map_err(|e| e.to_string())?;
        return Ok(());
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        std::process::Command::new("xdg-open")
            .arg(parent)
            .spawn()
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}

/**
 查询 yt-dlp 启动期健康状态（前端用于显示告警）。

 返回:
 - `Ok(bool)`: 是否就绪。
*/
#[tauri::command]
pub fn get_yt_dlp_healthy(state: tauri::State<'_, crate::AppState>) -> Result<bool, String> {
    let v = state
        .yt_dlp_healthy
        .read()
        .map_err(|_| "yt_dlp_healthy 读锁失败".to_string())?;
    Ok(*v)
}
