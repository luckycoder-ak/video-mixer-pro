use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock, RwLock};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

fn find_ffmpeg_executable() -> String {
    static FFMPEG_PATH: OnceLock<String> = OnceLock::new();

    FFMPEG_PATH.get_or_init(|| {
        // 优先使用与可执行文件同目录下的 sidecar ffmpeg（打包模式）
        // sidecar 命名规则：ffmpeg-<target-triple>(.exe)
        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(exe_dir) = current_exe.parent() {
                #[cfg(target_os = "windows")]
                let sidecar_names: &[&str] = &["ffmpeg.exe", "ffmpeg-x86_64-pc-windows-msvc.exe"];
                #[cfg(target_os = "macos")]
                let sidecar_names: &[&str] = &[
                    "ffmpeg",
                    "ffmpeg-aarch64-apple-darwin",
                    "ffmpeg-x86_64-apple-darwin",
                ];
                #[cfg(target_os = "linux")]
                let sidecar_names: &[&str] = &["ffmpeg", "ffmpeg-x86_64-unknown-linux-gnu"];

                for name in sidecar_names {
                    let candidate = exe_dir.join(name);
                    if candidate.exists() {
                        info!("使用捆绑的 ffmpeg sidecar: {}", candidate.display());
                        return candidate.to_string_lossy().to_string();
                    }
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            // macOS 开发环境回退：优先 ffmpeg-full (Homebrew keg-only)
            let ffmpeg_full_paths = vec![
                "/usr/local/opt/ffmpeg-full/bin/ffmpeg",
                "/opt/homebrew/opt/ffmpeg-full/bin/ffmpeg",
            ];

            for path in ffmpeg_full_paths {
                if std::path::Path::new(path).exists() {
                    info!("找到 ffmpeg-full: {}", path);
                    return path.to_string();
                }
            }
        }

        // 默认使用系统 PATH 中的 ffmpeg
        "ffmpeg".to_string()
    }).clone()
}

use log::{error, info, warn};
use rand::Rng;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

use crate::AppState;
use crate::config;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const WINDOWS_CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Running,
    Paused,
    Completed,
    Partial,
    Error,
}

impl Default for TaskStatus {
    fn default() -> Self {
        TaskStatus::Pending
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    Pending,
    Running,
    Completed,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskStep {
    pub id: String,
    pub name: String,
    pub status: StepStatus,
    pub error: Option<String>,
    /// 步骤进入 Running 状态的时刻
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    /// 步骤进入 Completed/Error 状态的时刻
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<DateTime<Utc>>,
}

/// 日志级别
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Info,
    Warn,
    Error,
}

/// 任务结构化日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: DateTime<Utc>,
    pub level: LogLevel,
    /// 关联的视频序号（0 表示任务级别，非视频级别）
    #[serde(default)]
    pub video_index: usize,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub config_id: String,
    pub task_name: String,
    pub total_count: usize,
    pub completed_count: usize,
    #[serde(default)]
    pub failed_count: usize,
    #[serde(default)]
    pub failed_videos: Vec<String>,
    pub status: TaskStatus,
    pub output_folder: String,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub current_video: usize,
    pub progress_steps: Vec<TaskStep>,
    #[serde(default)]
    pub logs: Vec<LogEntry>,
}

impl Task {
    pub fn new(config_name: String, total_count: usize) -> Self {
        // 使用更友好的日期时间格式: YYYY-MM-DD_HH-MM-SS
        let timestamp = Utc::now().format("%Y-%m-%d_%H-%M-%S").to_string();
        let safe_name = sanitize_task_name(&config_name);
        let task_name = format!("{}_{}", safe_name, timestamp);
        Task {
            id: Uuid::new_v4().to_string(),
            name: config_name.clone(),
            config_id: String::new(),
            task_name,
            total_count,
            completed_count: 0,
            failed_count: 0,
            failed_videos: Vec::new(),
            status: TaskStatus::Pending,
            output_folder: String::new(),
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            error_message: None,
            current_video: 0,
            progress_steps: Vec::new(),
            logs: Vec::new(),
        }
    }
}

/**
 返回 Windows 隐藏控制台窗口所需的 creation flags。

 参数:
 - 无。

 返回:
 - Windows 平台返回 `CREATE_NO_WINDOW` 对应的标志位；
 - 非 Windows 平台返回 `0`。

 异常:
 - 本函数不抛出异常。
 */
#[cfg_attr(not(any(target_os = "windows", test)), allow(dead_code))]
fn hidden_process_creation_flags() -> u32 {
    #[cfg(target_os = "windows")]
    {
        WINDOWS_CREATE_NO_WINDOW
    }
    #[cfg(not(target_os = "windows"))]
    {
        0
    }
}

/**
 为外部子进程应用“隐藏控制台窗口”的启动参数。

 参数:
 - `command`: 待启动的子进程命令对象。

 返回:
 - `&mut Command`: 返回原命令对象，便于链式继续设置参数。

 异常:
 - 本函数不抛出异常；非 Windows 平台为无副作用空操作。
 */
pub fn apply_hidden_process_startup(command: &mut Command) -> &mut Command {
    #[cfg(target_os = "windows")]
    {
        command.creation_flags(hidden_process_creation_flags());
    }
    command
}

/**
 将任务名清洗为文件系统安全的字符串。

 规则:
 - trim 头尾空白；
 - 替换非法字符 `/ \ : * ? " < > |` 为 `_`；
 - 控制字符与制表/换行替换为 `_`；
 - 全部空白后回退为 `task`。

 参数:
 - `name`: 原始名称。

 返回:
 - 清洗后的安全名称。
 */
fn sanitize_task_name(name: &str) -> String {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return "task".to_string();
    }
    let cleaned: String = trimmed
        .chars()
        .map(|c| {
            if c.is_control() {
                '_'
            } else {
                match c {
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' => '_',
                    _ => c,
                }
            }
        })
        .collect();
    if cleaned.trim().is_empty() {
        "task".to_string()
    } else {
        cleaned
    }
}

struct TempDirGuard(PathBuf);

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        match fs::remove_dir_all(&self.0) {
            Ok(()) => info!("自动清理临时目录: {:?}", self.0),
            Err(e) => warn!("自动清理临时目录失败: {:?}, 错误: {}", self.0, e),
        }
    }
}

/**
 简易计数信号量，用于限制全局同时执行的 ffmpeg 子进程数量。
 */
struct CountingSemaphore {
    inner: Mutex<usize>,
    cond: Condvar,
}

impl CountingSemaphore {
    fn new(permits: usize) -> Self {
        Self {
            inner: Mutex::new(permits.max(1)),
            cond: Condvar::new(),
        }
    }

    fn acquire(&self) -> SemaphoreGuard<'_> {
        let mut guard = self.inner.lock().unwrap();
        while *guard == 0 {
            guard = self.cond.wait(guard).unwrap();
        }
        *guard -= 1;
        SemaphoreGuard { sem: self }
    }

    fn release(&self) {
        let mut guard = self.inner.lock().unwrap();
        *guard += 1;
        self.cond.notify_one();
    }
}

struct SemaphoreGuard<'a> {
    sem: &'a CountingSemaphore,
}

impl<'a> Drop for SemaphoreGuard<'a> {
    fn drop(&mut self) {
        self.sem.release();
    }
}

fn ffmpeg_semaphore() -> &'static CountingSemaphore {
    static SEM: OnceLock<CountingSemaphore> = OnceLock::new();
    SEM.get_or_init(|| {
        let cpus = num_cpus::get();
        let encoder = detect_best_encoder();
        let permits = if encoder.use_hw_accel {
            2
        } else {
            (cpus / 2).max(1)
        };
        info!("ffmpeg 全局并发上限: {} (cpus={}, hw={})", permits, cpus, encoder.use_hw_accel);
        CountingSemaphore::new(permits)
    })
}

/**
 任务级取消令牌注册表。
 - key: task_id
 - value: AtomicBool；true 表示请求取消（删除/停止）。
 */
fn task_cancel_registry() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static REG: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

/**
 任务级暂停令牌注册表。
 - key: task_id
 - value: AtomicBool；true 表示已暂停，主循环在每个视频开始前等待。
 */
fn task_pause_registry() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    static REG: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

/**
 任务级 ffmpeg 子进程注册表。
 - key: task_id
 - value: 该任务当前活跃的 Child handle 列表，用于取消时 kill。
 */
fn task_children_registry() -> &'static Mutex<HashMap<String, Vec<Arc<Mutex<Option<Child>>>>>> {
    static REG: OnceLock<Mutex<HashMap<String, Vec<Arc<Mutex<Option<Child>>>>>>> = OnceLock::new();
    REG.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register_cancel(task_id: &str) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let mut reg = task_cancel_registry().lock().unwrap();
    reg.insert(task_id.to_string(), flag.clone());
    flag
}

fn register_pause(task_id: &str) -> Arc<AtomicBool> {
    let flag = Arc::new(AtomicBool::new(false));
    let mut reg = task_pause_registry().lock().unwrap();
    reg.insert(task_id.to_string(), flag.clone());
    flag
}

fn unregister_task(task_id: &str) {
    let _ = task_cancel_registry().lock().unwrap().remove(task_id);
    let _ = task_pause_registry().lock().unwrap().remove(task_id);
    let _ = task_children_registry().lock().unwrap().remove(task_id);
}

fn signal_cancel(task_id: &str) {
    if let Some(flag) = task_cancel_registry().lock().unwrap().get(task_id) {
        flag.store(true, Ordering::SeqCst);
    }
    // kill 当前活跃的所有 ffmpeg 子进程
    if let Some(children) = task_children_registry().lock().unwrap().get(task_id).cloned() {
        for child_slot in children {
            if let Ok(mut guard) = child_slot.lock() {
                if let Some(child) = guard.as_mut() {
                    let _ = child.kill();
                }
            }
        }
    }
}

fn set_pause(task_id: &str, paused: bool) {
    if let Some(flag) = task_pause_registry().lock().unwrap().get(task_id) {
        flag.store(paused, Ordering::SeqCst);
    }
    if paused {
        // 暂停时同时 kill 当前活跃 ffmpeg；恢复后会从下一个视频继续
        if let Some(children) = task_children_registry().lock().unwrap().get(task_id).cloned() {
            for child_slot in children {
                if let Ok(mut guard) = child_slot.lock() {
                    if let Some(child) = guard.as_mut() {
                        let _ = child.kill();
                    }
                }
            }
        }
    }
}

fn add_child(task_id: &str, child: Arc<Mutex<Option<Child>>>) {
    let mut reg = task_children_registry().lock().unwrap();
    reg.entry(task_id.to_string()).or_insert_with(Vec::new).push(child);
}

fn remove_child(task_id: &str, child: &Arc<Mutex<Option<Child>>>) {
    if let Some(list) = task_children_registry().lock().unwrap().get_mut(task_id) {
        list.retain(|c| !Arc::ptr_eq(c, child));
    }
}

/**
 推送/更新任务进度步骤。
 - 若 `id` 已存在则更新 status/error；否则追加。
 */
fn push_step(
    tasks: &Arc<RwLock<Vec<Task>>>,
    task_id: &str,
    step_id: &str,
    name: &str,
    status: StepStatus,
    error: Option<String>,
) {
    if let Ok(mut tasks) = tasks.write() {
        if let Some(t) = tasks.iter_mut().find(|t| t.id == task_id) {
            let now = Utc::now();
            let should_emit_step_log = matches!(status, StepStatus::Error) || step_id == "finish";
            let mut should_log = false;
            let mut log_message = String::new();
            if let Some(step) = t.progress_steps.iter_mut().find(|s| s.id == step_id) {
                let changed = step.status != status || step.error != error || step.name != name;
                step.name = name.to_string();
                step.status = status.clone();
                step.error = error.clone();
                if status == StepStatus::Running && step.started_at.is_none() {
                    step.started_at = Some(now);
                }
                if matches!(status, StepStatus::Completed | StepStatus::Error) {
                    if step.started_at.is_none() {
                        step.started_at = Some(now);
                    }
                    step.completed_at = Some(now);
                }
                if changed && should_emit_step_log {
                    should_log = true;
                    log_message = build_step_log_message(name, &status, error.as_deref());
                }
            } else {
                t.progress_steps.push(TaskStep {
                    id: step_id.to_string(),
                    name: name.to_string(),
                    status: status.clone(),
                    error: error.clone(),
                    started_at: if status == StepStatus::Running || matches!(status, StepStatus::Completed | StepStatus::Error) {
                        Some(now)
                    } else {
                        None
                    },
                    completed_at: if matches!(status, StepStatus::Completed | StepStatus::Error) {
                        Some(now)
                    } else {
                        None
                    },
                });
                if should_emit_step_log {
                    should_log = true;
                    log_message = build_step_log_message(name, &status, error.as_deref());
                }
            }
            if should_log {
                append_task_log_entry(
                    t,
                    LogEntry {
                        timestamp: now,
                        level: match status {
                            StepStatus::Error => LogLevel::Error,
                            _ => LogLevel::Info,
                        },
                        video_index: extract_video_index(step_id),
                        message: log_message,
                    },
                );
            }
        }
    }
}

const TASK_LOG_LIMIT: usize = 500;

fn extract_video_index(step_id: &str) -> usize {
    step_id
        .split('_')
        .nth(1)
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(0)
}

fn build_step_log_message(name: &str, status: &StepStatus, error: Option<&str>) -> String {
    match status {
        StepStatus::Pending => format!("步骤等待: {}", name),
        StepStatus::Running => format!("开始执行: {}", name),
        StepStatus::Completed => format!("执行完成: {}", name),
        StepStatus::Error => match error {
            Some(err) if !err.is_empty() => format!("执行失败: {} - {}", name, err),
            _ => format!("执行失败: {}", name),
        },
    }
}

fn append_task_log_entry(task: &mut Task, entry: LogEntry) {
    if let Some(last) = task.logs.last() {
        if last.level == entry.level && last.video_index == entry.video_index && last.message == entry.message {
            return;
        }
    }
    task.logs.push(entry);
    if task.logs.len() > TASK_LOG_LIMIT {
        let drain_count = task.logs.len().saturating_sub(TASK_LOG_LIMIT);
        task.logs.drain(0..drain_count);
    }
}

fn push_log(
    tasks: &Arc<RwLock<Vec<Task>>>,
    task_id: &str,
    level: LogLevel,
    video_index: usize,
    message: impl Into<String>,
) {
    if let Ok(mut tasks) = tasks.write() {
        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
            append_task_log_entry(
                task,
                LogEntry {
                    timestamp: Utc::now(),
                    level,
                    video_index,
                    message: message.into(),
                },
            );
        }
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct VideoInfo {
    pub path: PathBuf,
    pub duration: f64,
}

/// 视频文件列表缓存条目。
/// - `videos`：扫描得到的视频文件路径列表
/// - `dir_mtime`：扫描时记录的目录修改时间（用于检测文件增删）
/// - `cached_at`：缓存写入时刻（用于兜底 TTL）
#[derive(Clone)]
struct VideoCacheEntry {
    videos: Vec<PathBuf>,
    dir_mtime: Option<SystemTime>,
    cached_at: Instant,
}

fn video_files_cache() -> &'static RwLock<HashMap<String, VideoCacheEntry>> {
    static CACHE: OnceLock<RwLock<HashMap<String, VideoCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// 缓存兜底 TTL：即使 mtime 检测异常，超过该时长也会强制重扫
const VIDEO_CACHE_TTL: Duration = Duration::from_secs(5);

/// 扫描指定文件夹下的视频文件列表
///
/// # 缓存策略
/// 1. 命中缓存：当前目录 mtime 与缓存记录的 mtime 一致 且 缓存未超过 [`VIDEO_CACHE_TTL`]
/// 2. 失效重扫：目录被新增/删除/重命名文件后 mtime 会变化 → 自动失效
/// 3. 兜底 TTL：极少数文件系统 mtime 不更新时，5s 后强制重扫
///
/// # Arguments
/// - `folder`: 要扫描的文件夹绝对/相对路径
///
/// # Returns
/// - `Ok(Vec<PathBuf>)`: 该文件夹下所有视频文件（按文件名排序）
/// - `Err(String)`: 文件夹不存在或读取失败
pub fn get_video_files(folder: &str) -> Result<Vec<PathBuf>, String> {
    let normalized_folder = normalize_folder_key(folder);

    // 当前目录的 mtime（失败时 None，按"始终重扫"处理）
    let current_mtime: Option<SystemTime> = fs::metadata(folder)
        .and_then(|m| m.modified())
        .ok();

    // 尝试命中缓存：mtime 必须一致 且 未超过 TTL
    {
        let cache = video_files_cache().read().map_err(|e| e.to_string())?;
        if let Some(entry) = cache.get(&normalized_folder) {
            let mtime_match = entry.dir_mtime == current_mtime;
            let ttl_ok = entry.cached_at.elapsed() < VIDEO_CACHE_TTL;
            if mtime_match && ttl_ok {
                return Ok(entry.videos.clone());
            }
        }
    }

    let path = PathBuf::from(folder);
    if !path.exists() {
        return Err(format!("文件夹不存在: {}", folder));
    }

    let mut videos = Vec::new();
    if let Ok(entries) = fs::read_dir(&path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                let ext_lower = ext.to_lowercase();
                if matches!(ext_lower.as_str(), "mp4" | "avi" | "mov" | "mkv" | "flv" | "wmv" | "webm") {
                    videos.push(path);
                }
            }
        }
    }

    videos.sort();

    // 写入缓存（含 mtime 与时间戳）
    let mut cache = video_files_cache().write().map_err(|e| e.to_string())?;
    cache.insert(
        normalized_folder,
        VideoCacheEntry {
            videos: videos.clone(),
            dir_mtime: current_mtime,
            cached_at: Instant::now(),
        },
    );

    Ok(videos)
}

pub fn select_random_videos(
    videos: &[PathBuf],
    count: usize,
    used: &HashSet<String>,
) -> Result<Vec<PathBuf>, String> {
    let available: Vec<_> = videos
        .iter()
        .filter(|v| !used.contains(&v.to_string_lossy().to_string()))
        .cloned()
        .collect();

    if available.len() < count {
        return Err(format!(
            "可用视频文件不足，需要 {} 个，剩余 {} 个（已扣除全局已用素材）",
            count,
            available.len()
        ));
    }

    let mut selected = Vec::new();
    let mut indices: Vec<usize> = (0..available.len()).collect();
    let mut rng = rand::thread_rng();

    for _ in 0..count {
        if indices.is_empty() {
            break;
        }
        let random_index = rng.gen_range(0..indices.len());
        let video_index = indices.remove(random_index);
        selected.push(available[video_index].clone());
    }

    Ok(selected)
}

/**
 将 source_folder 路径规范化为 used_per_folder 的 HashMap key。
 规则：trim + canonicalize（失败时退化为原始字符串），保证同一目录的不同写法（带空格、相对路径等）映射到同一桶里。
 */
fn normalize_folder_key(folder: &str) -> String {
    let trimmed = folder.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    match std::fs::canonicalize(trimmed) {
        Ok(p) => p.to_string_lossy().to_string(),
        Err(_) => trimmed.to_string(),
    }
}

fn save_tasks_to_disk(
    app_data_file: &Arc<RwLock<Option<PathBuf>>>,
    tasks: &Arc<RwLock<Vec<Task>>>,
    configs: &Arc<RwLock<Vec<crate::config::VideoConfig>>>,
    tutorial_used_by_config: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
) -> Result<(), String> {
    let path_opt = app_data_file
        .read()
        .map_err(|e| format!("读取 app_data_file 失败: {}", e))?
        .clone();
    let Some(path) = path_opt else {
        return Err("app_data_file 尚未初始化".to_string());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("创建目录失败: {}", e))?;
    }

    // 读取现有数据，保留 usage_records
    let usage_records = if path.exists() {
        if let Ok(content) = fs::read_to_string(&path) {
            if let Ok(existing_data) = serde_json::from_str::<crate::storage::AppData>(&content) {
                existing_data.usage_records
            } else {
                std::collections::HashMap::new()
            }
        } else {
            std::collections::HashMap::new()
        }
    } else {
        std::collections::HashMap::new()
    };

    // 获取最新的任务和配置
    let current_tasks = tasks.read().map_err(|e| e.to_string())?.clone();
    let current_configs = configs.read().map_err(|e| e.to_string())?.clone();
    let current_used_tutorial = tutorial_used_by_config
        .read()
        .map_err(|e| e.to_string())?
        .clone();
    let base_dir = path
        .parent()
        .ok_or_else(|| "无法解析 app_data.json 所在目录".to_string())?;
    let (_, persisted_tasks) = crate::storage::persist_runtime_store_in_dir(
        base_dir,
        &current_configs,
        &current_tasks,
        &current_used_tutorial,
        usage_records,
    )?;

    if let Ok(mut task_guard) = tasks.write() {
        *task_guard = persisted_tasks.clone();
    }
    info!("成功保存 {} 个任务到磁盘", persisted_tasks.len());
    Ok(())
}

pub fn calculate_video_dimensions(ratio: &str) -> (u32, u32) {
    match ratio {
        "9:16" => (1080, 1920),
        "16:9" => (1920, 1080),
        "1:1" => (1080, 1080),
        "4:5" => (1080, 1350),
        _ => (1080, 1920),
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EncoderConfig {
    pub video_codec: &'static str,
    pub audio_codec: &'static str,
    pub use_hw_accel: bool,
}

fn detect_best_encoder() -> EncoderConfig {
    static ENCODER: OnceLock<EncoderConfig> = OnceLock::new();

    ENCODER.get_or_init(|| {
        let mut command = Command::new("ffmpeg");
        apply_hidden_process_startup(&mut command);
        let output = command.args(["-hide_banner", "-encoders"]).output();

        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);

                if stdout.contains("h264_nvenc") {
                    info!("使用 NVIDIA GPU 加速编码器");
                    EncoderConfig { video_codec: "h264_nvenc", audio_codec: "aac", use_hw_accel: true }
                } else if stdout.contains("h264_vaapi") {
                    info!("使用 VAAPI 硬件加速编码器");
                    EncoderConfig { video_codec: "h264_vaapi", audio_codec: "aac", use_hw_accel: true }
                } else if stdout.contains("h264_qsv") {
                    info!("使用 Intel QSV 硬件加速编码器");
                    EncoderConfig { video_codec: "h264_qsv", audio_codec: "aac", use_hw_accel: true }
                } else {
                    info!("使用软件编码器 (libx264)");
                    EncoderConfig { video_codec: "libx264", audio_codec: "aac", use_hw_accel: false }
                }
            }
            Err(e) => {
                warn!("检测编码器失败: {}, 使用默认软件编码器", e);
                EncoderConfig { video_codec: "libx264", audio_codec: "aac", use_hw_accel: false }
            }
        }
    }).clone()
}

/**
 随机选取一种 xfade 转场类型。
 池中包含 fade/dissolve/slide/smooth/circle/zoom/pixelize 等多类风格，
 wipe 系仅保留 4 种以稀释占比，避免视觉上"全是翻页"的观感。
 返回值：ffmpeg xfade 滤镜支持的 transition 名称。
 */
pub fn get_random_transition_type() -> String {
    let transitions = vec![
        // 淡变类（柔和）
        "fade".to_string(),
        "fadeblack".to_string(),
        "fadewhite".to_string(),
        "dissolve".to_string(),
        // 滑动类
        "slideleft".to_string(),
        "slideright".to_string(),
        "slideup".to_string(),
        "slidedown".to_string(),
        // 平滑过渡
        "smoothleft".to_string(),
        "smoothright".to_string(),
        "smoothup".to_string(),
        "smoothdown".to_string(),
        // 形状类
        "circleopen".to_string(),
        "circleclose".to_string(),
        "circlecrop".to_string(),
        "rectcrop".to_string(),
        "radial".to_string(),
        // 缩放/像素化
        "zoomin".to_string(),
        "pixelize".to_string(),
        // 切片类
        "hlslice".to_string(),
        "hrslice".to_string(),
        "vuslice".to_string(),
        "vdslice".to_string(),
        // wipe 系（保留少量以维持多样性）
        "wipeleft".to_string(),
        "wiperight".to_string(),
        "wipeup".to_string(),
        "wipedown".to_string(),
    ];

    let mut rng = rand::thread_rng();
    let index = rng.gen_range(0..transitions.len());
    transitions[index].clone()
}

/**
 根据各模板片段的时长，计算每个片段在素材时间线上的起始偏移。
 */
/// 根据时间点数组计算各片段的起始偏移量
/// 时间点表示每个片段的结束时间，例如：[3.28, 6.29, 9.33]
/// 则：
/// - 第一段起始=0，结束=3.28，时长=3.28
/// - 第二段起始=3.28，结束=6.29，时长=3.01
/// - 第三段起始=6.29，结束=9.33，时长=3.04
fn compute_segment_offsets(time_points: &[f32]) -> Vec<f32> {
    let mut offsets = Vec::with_capacity(time_points.len());
    let mut prev_time_point: f32 = 0.0;
    for (i, &time_point) in time_points.iter().enumerate() {
        if i == 0 {
            offsets.push(0.0);
        } else {
            offsets.push(prev_time_point);
        }
        prev_time_point = time_point;
    }
    offsets
}

/// 根据时间点数组计算各片段的实际时长
fn compute_segment_durations(time_points: &[f32]) -> Vec<f32> {
    let offsets = compute_segment_offsets(time_points);
    time_points.iter()
        .enumerate()
        .map(|(i, &tp)| tp - offsets[i])
        .collect()
}

fn process_segment(
    task_id: &str,
    cancel: &Arc<AtomicBool>,
    videos: &[PathBuf],
    output: &PathBuf,
    crop_mode: &config::CropMode,
    (output_width, output_height): (u32, u32),
    start_offset: f32,
    duration: f32,
    temp_dir: &PathBuf,
    scale_percent: u32,
) -> Result<(), String> {
    match crop_mode {
        config::CropMode::Single => {
            process_single_mode_segment(task_id, cancel, videos, output, output_width, output_height, start_offset, duration)
        }
        config::CropMode::Dual => {
            process_dual_mode_optimized(task_id, cancel, videos, output, output_width, output_height, start_offset, duration, temp_dir, scale_percent)
        }
        config::CropMode::Quadrant => {
            process_quadrant_mode_optimized(task_id, cancel, videos, output, output_width, output_height, start_offset, duration, temp_dir)
        }
    }
}

fn process_single_mode_segment(
    task_id: &str,
    cancel: &Arc<AtomicBool>,
    videos: &[PathBuf],
    output: &PathBuf,
    output_width: u32,
    output_height: u32,
    start_offset: f32,
    duration: f32,
) -> Result<(), String> {
    let video = &videos[0];
    let input_str = video.to_string_lossy().to_string();
    let output_str = output.to_string_lossy().to_string();

    let encoder = detect_best_encoder();
    let num_cpus_str = num_cpus::get().to_string();

    // S7：所有片段提前对齐 setsar/fps/format，single 段不带音轨（统一在 xfade 后挂背景音）
    // 修复 NVENC "Invalid color range"：通过 setrange=tv 显式标注色彩范围元数据，避免 filter 链 reinit 失败
    let filter_str = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black,setsar=sar=1,fps=30,format=yuv420p,setrange=tv",
        output_width, output_height, output_width, output_height
    );

    let start_str = start_offset.to_string();
    let duration_str = duration.to_string();

    let args: Vec<String> = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(), "error".to_string(),
        "-ss".to_string(), start_str,
        "-i".to_string(), input_str,
        "-t".to_string(), duration_str,
        "-an".to_string(),
        "-vf".to_string(), filter_str,
        "-c:v".to_string(), encoder.video_codec.to_string(),
        "-pix_fmt".to_string(), "yuv420p".to_string(),
        "-movflags".to_string(), "+faststart".to_string(),
        "-threads".to_string(), num_cpus_str,
        "-y".to_string(),
        output_str,
    ];

    run_ffmpeg_with_cancel(task_id, cancel, &args)?;
    Ok(())
}

/**
 根据模板第一个片段选中的首个素材，从其尾部截取片段补充视频时长。

 参数:
 - `task_id`: 当前任务 ID
 - `cancel`: 取消标记
 - `input_video`: 当前待补充的视频文件
 - `output_video`: 补充完成后的输出文件
 - `duration_needed`: 需要补充的时长，单位秒
 - `supplement_source_video`: 用于补充的源视频，取自模板第一个片段选中的首个素材
 - `output_width`: 输出视频宽度
 - `output_height`: 输出视频高度

 返回:
 - `Ok(())`: 补充成功
 - `Err(String)`: 截取、拼接或探测视频失败

 异常:
 - 当补充源视频不可用或 ffmpeg 执行失败时返回错误字符串。
 */
fn supplement_video_with_first_segment_tail(
    task_id: &str,
    cancel: &Arc<AtomicBool>,
    input_video: &PathBuf,
    output_video: &PathBuf,
    duration_needed: f32,
    supplement_source_video: &PathBuf,
    output_width: u32,
    output_height: u32,
) -> Result<(), String> {
    let clip_duration = probe_video_duration(&supplement_source_video.to_string_lossy())?;
    let (start_offset, use_duration) =
        calculate_tail_supplement_window(clip_duration, duration_needed);

    let encoder = detect_best_encoder();
    let num_cpus_str = num_cpus::get().to_string();

    // 构建 filter_complex：第一个视频直接输出，第二个视频截取并缩放（规范化色彩范围）
    let filter_complex = format!(
        "[1:v]scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black,setsar=sar=1,fps=30,format=yuv420p,setrange=tv,trim=start={}:duration={},setpts=PTS-STARTPTS[v1];[0:v][v1]concat=n=2:v=1:a=0",
        output_width, output_height, output_width, output_height, start_offset, use_duration
    );

    let args: Vec<String> = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(), "error".to_string(),
        "-i".to_string(), input_video.to_string_lossy().to_string(),
        "-i".to_string(), supplement_source_video.to_string_lossy().to_string(),
        "-filter_complex".to_string(), filter_complex,
        "-c:v".to_string(), encoder.video_codec.to_string(),
        "-pix_fmt".to_string(), "yuv420p".to_string(),
        "-movflags".to_string(), "+faststart".to_string(),
        "-threads".to_string(), num_cpus_str,
        "-y".to_string(),
        output_video.to_string_lossy().to_string(),
    ];

    run_ffmpeg_with_cancel(task_id, cancel, &args)
}

/**
 计算用于补尾的尾部截取窗口。

 参数:
 - `clip_duration`: 源视频总时长，单位秒
 - `duration_needed`: 需要补充的时长，单位秒

 返回:
 - `(start_offset, use_duration)`: 从源视频尾部开始截取的起始偏移和实际使用时长

 异常:
 - 本函数不抛出异常；当输入为非正值时自动回退到 0。
 */
fn calculate_tail_supplement_window(clip_duration: f32, duration_needed: f32) -> (f32, f32) {
    let safe_clip_duration = clip_duration.max(0.0);
    let safe_duration_needed = duration_needed.max(0.0);
    let use_duration = safe_duration_needed.min(safe_clip_duration);
    let start_offset = (safe_clip_duration - use_duration).max(0.0);
    (start_offset, use_duration)
}

fn process_dual_mode_optimized(
    task_id: &str,
    cancel: &Arc<AtomicBool>,
    videos: &[PathBuf],
    output: &PathBuf,
    output_width: u32,
    output_height: u32,
    start_offset: f32,
    duration: f32,
    _temp_dir: &PathBuf,
    scale_percent: u32,
) -> Result<(), String> {
    let input1_str = videos[0].to_string_lossy().to_string();
    let input2_str = videos[1].to_string_lossy().to_string();
    let output_str = output.to_string_lossy().to_string();

    let half_width = output_width / 2;
    let _half_height = output_height / 2;
    let encoder = detect_best_encoder();
    let num_cpus_str = num_cpus::get().to_string();

    // 严格按照表格逻辑：
    // 缩放目标尺寸 = (output_width * scale_percent%, output_height * scale_percent%)
    // 裁剪目标尺寸 = (half_width, output_height * scale_percent%)
    let scale_factor = scale_percent as f32 / 100.0;
    let target_scale_w = (output_width as f32 * scale_factor) as u32;
    let target_scale_h = (output_height as f32 * scale_factor) as u32;
    let crop_h = target_scale_h;

    info!("双列模式: scale_percent={}, 缩放目标尺寸={}x{}, 裁剪目标尺寸={}x{}",
        scale_percent, target_scale_w, target_scale_h, half_width, crop_h);

    let center_h = crop_h;
    let y_offset = (output_height as i32 - center_h as i32) / 2;

    // 合并为单次 ffmpeg 调用，避免 NVENC 中间文件空流导致 merge 阶段 "matches no streams"。
    // 关键：filter 链最前置 setparams=range=tv 强制规范化色彩范围元数据，
    // 防止上游素材 color_range 异常触发 NVENC reinit 失败。
    // - 输入 0: input1（用于 left + bg 模糊背景）
    // - 输入 1: input2（用于 right）
    // 通过 split 共享 input1，避免重复解码。
    let filter_complex = format!(
        "[0:v]trim=start={start}:duration={dur},setpts=PTS-STARTPTS,setparams=range=tv,split=2[v0a][v0b];\
         [1:v]trim=start={start}:duration={dur},setpts=PTS-STARTPTS,setparams=range=tv[v1a];\
         [v0a]scale={tw}:{th}:force_original_aspect_ratio=increase,crop={hw}:{ch}:(iw-{hw})/2:0,setsar=sar=1,fps=30,format=yuv420p,setrange=tv[left];\
         [v1a]scale={tw}:{th}:force_original_aspect_ratio=increase,crop={hw}:{ch}:(iw-{hw})/2:0,setsar=sar=1,fps=30,format=yuv420p,setrange=tv[right];\
         [v0b]scale={ow}:{oh}:force_original_aspect_ratio=decrease,pad={ow}:{oh}:(ow-iw)/2:(oh-ih)/2:0x000000,gblur=sigma=50,setsar=sar=1,fps=30,format=yuv420p,setrange=tv[bg];\
         [left][right]hstack=inputs=2[center];\
         [bg][center]overlay=x=0:y={yoff}:shortest=1[out]",
        start = start_offset,
        dur = duration,
        tw = target_scale_w,
        th = target_scale_h,
        hw = half_width,
        ch = crop_h,
        ow = output_width,
        oh = output_height,
        yoff = y_offset,
    );

    let args: Vec<String> = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(), "error".to_string(),
        "-i".to_string(), input1_str,
        "-i".to_string(), input2_str,
        "-filter_complex".to_string(), filter_complex,
        "-map".to_string(), "[out]".to_string(),
        "-an".to_string(),
        "-c:v".to_string(), encoder.video_codec.to_string(),
        "-pix_fmt".to_string(), "yuv420p".to_string(),
        "-threads".to_string(), num_cpus_str,
        "-y".to_string(),
        output_str,
    ];

    run_ffmpeg_with_cancel(task_id, cancel, &args)?;

    Ok(())
}

fn process_quadrant_mode_optimized(
    task_id: &str,
    cancel: &Arc<AtomicBool>,
    videos: &[PathBuf],
    output: &PathBuf,
    output_width: u32,
    output_height: u32,
    start_offset: f32,
    duration: f32,
    _temp_dir: &PathBuf,
) -> Result<(), String> {
    let half_width = output_width / 2;
    let half_height = output_height / 2;
    let encoder = detect_best_encoder();
    let num_cpus_str = num_cpus::get().to_string();

    let inputs = [
        videos[0].to_string_lossy().to_string(),
        videos[1].to_string_lossy().to_string(),
        videos[2].to_string_lossy().to_string(),
        videos[3].to_string_lossy().to_string(),
    ];
    let output_str = output.to_string_lossy().to_string();

    // 合并为单次 ffmpeg 调用，避免 NVENC 中间文件空流导致 merge 阶段 "matches no streams"。
    // 关键：filter 链最前置 setparams=range=tv 强制规范化色彩范围元数据，
    // 防止上游素材 color_range 异常触发 NVENC reinit 失败。
    let mut cell_chains: Vec<String> = Vec::with_capacity(4);
    for i in 0..4 {
        cell_chains.push(format!(
            "[{i}:v]trim=start={start}:duration={dur},setpts=PTS-STARTPTS,setparams=range=tv,scale={hw}:{hh}:force_original_aspect_ratio=decrease,pad={hw}:{hh}:(ow-iw)/2:(oh-ih)/2:0x000000,setsar=sar=1,fps=30,format=yuv420p,setrange=tv[c{i}]",
            i = i,
            start = start_offset,
            dur = duration,
            hw = half_width,
            hh = half_height,
        ));
    }

    let filter_complex = format!(
        "{};[c0][c1]hstack=inputs=2[top];[c2][c3]hstack=inputs=2[bottom];[top][bottom]vstack=inputs=2[out]",
        cell_chains.join(";")
    );

    let _ = output_width; // 仅用于满足参数语义（最终输出宽度 = half_width * 2）
    let args: Vec<String> = vec![
        "-hide_banner".to_string(), "-loglevel".to_string(), "error".to_string(),
        "-i".to_string(), inputs[0].clone(),
        "-i".to_string(), inputs[1].clone(),
        "-i".to_string(), inputs[2].clone(),
        "-i".to_string(), inputs[3].clone(),
        "-filter_complex".to_string(), filter_complex,
        "-map".to_string(), "[out]".to_string(),
        "-an".to_string(),
        "-c:v".to_string(), encoder.video_codec.to_string(),
        "-pix_fmt".to_string(), "yuv420p".to_string(),
        "-threads".to_string(), num_cpus_str,
        "-y".to_string(),
        output_str,
    ];

    run_ffmpeg_with_cancel(task_id, cancel, &args)?;

    Ok(())
}

/**
 在全局并发信号量下执行 ffmpeg，并支持任务级取消（kill 子进程）。

 参数:
 - `task_id`: 任务 ID，用于注册子进程方便外部 kill。
 - `cancel`: 取消令牌，true 时不再等待 wait，主动 kill 并返回 Err。
 - `args`: ffmpeg 完整参数。

 返回:
 - 成功 Ok(())；失败/取消时返回错误描述。
 */
pub fn run_ffmpeg_with_cancel(
    task_id: &str,
    cancel: &Arc<AtomicBool>,
    args: &[String],
) -> Result<(), String> {
    if cancel.load(Ordering::SeqCst) {
        return Err("已取消".to_string());
    }

    let _permit = ffmpeg_semaphore().acquire();

    if cancel.load(Ordering::SeqCst) {
        return Err("已取消".to_string());
    }

    let ffmpeg_path = find_ffmpeg_executable();
    let mut command = Command::new(&ffmpeg_path);
    apply_hidden_process_startup(&mut command);
    let child = command
        .args(args.iter().map(|s| s.as_str()).collect::<Vec<&str>>())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("启动 FFmpeg 失败: {}", e))?;

    let child_slot = Arc::new(Mutex::new(Some(child)));
    add_child(task_id, child_slot.clone());

    // 轮询等待，期间检测取消标志
    loop {
        if cancel.load(Ordering::SeqCst) {
            if let Ok(mut guard) = child_slot.lock() {
                if let Some(c) = guard.as_mut() {
                    let _ = c.kill();
                }
            }
            // 等待回收
            if let Ok(mut guard) = child_slot.lock() {
                if let Some(mut c) = guard.take() {
                    let _ = c.wait();
                }
            }
            remove_child(task_id, &child_slot);
            return Err("已取消".to_string());
        }

        let try_status = {
            let mut guard = child_slot.lock().unwrap();
            match guard.as_mut() {
                Some(c) => c.try_wait().map_err(|e| format!("等待 FFmpeg 失败: {}", e))?,
                None => Some(std::process::ExitStatus::from_raw_unspecified()),
            }
        };

        if let Some(status) = try_status {
            // 收集输出
            let output_result = {
                let mut guard = child_slot.lock().unwrap();
                guard.take().map(|c| c.wait_with_output())
            };
            remove_child(task_id, &child_slot);
            if status.success() {
                return Ok(());
            } else {
                let stderr = match output_result {
                    Some(Ok(out)) => String::from_utf8_lossy(&out.stderr).to_string(),
                    _ => String::new(),
                };
                return Err(format!("FFmpeg 执行失败: {}", stderr));
            }
        }

        thread::sleep(Duration::from_millis(120));
    }
}

// 兼容旧调用：保留无取消版本（仅 detect_best_encoder 等场景用，不长时间）。
#[allow(dead_code)]
pub fn run_ffmpeg_fast(args: &[&str]) -> Result<(), String> {
    let ffmpeg_path = find_ffmpeg_executable();
    let mut command = Command::new(&ffmpeg_path);
    apply_hidden_process_startup(&mut command);
    let output = command
        .args(args)
        .output()
        .map_err(|e| format!("执行 FFmpeg 失败: {}", e))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!("FFmpeg 执行失败: {}", stderr))
    }
}

/**
 跨平台占位：Stable Rust 不直接暴露 ExitStatus::from_raw_unspecified，
 此处用 trait 提供兜底，避免编译错误。仅在 child slot 已被取走时使用。
 */
trait ExitStatusExt {
    fn from_raw_unspecified() -> std::process::ExitStatus;
}

#[cfg(unix)]
impl ExitStatusExt for std::process::ExitStatus {
    fn from_raw_unspecified() -> std::process::ExitStatus {
        use std::os::unix::process::ExitStatusExt as _;
        std::process::ExitStatus::from_raw(0)
    }
}

#[cfg(windows)]
impl ExitStatusExt for std::process::ExitStatus {
    fn from_raw_unspecified() -> std::process::ExitStatus {
        use std::os::windows::process::ExitStatusExt as _;
        std::process::ExitStatus::from_raw(0)
    }
}

pub fn probe_video_duration(video_path: &str) -> Result<f32, String> {
    let mut command = Command::new("ffprobe");
    apply_hidden_process_startup(&mut command);
    let output = command
        .args([
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            video_path,
        ])
        .output()
        .map_err(|e| format!("执行 ffprobe 失败: {}", e))?;

    if output.status.success() {
        let duration_str = String::from_utf8_lossy(&output.stdout);
        duration_str
            .trim()
            .parse::<f32>()
            .map_err(|e| format!("解析视频时长失败: {}", e))
    } else {
        Err("无法获取视频时长".to_string())
    }
}

/// 视频时长缓存条目。
/// - `duration`: ffprobe 探测得到的时长（秒）
/// - `file_mtime`: 缓存时记录的文件修改时间
/// - `file_size`: 缓存时记录的文件大小
/// - `cached_at`: 缓存写入时刻（用于兜底 TTL）
#[derive(Debug, Clone)]
struct DurationCacheEntry {
    duration: f32,
    file_mtime: Option<SystemTime>,
    file_size: Option<u64>,
    cached_at: Instant,
}

/**
 进程级视频时长缓存，避免同一素材重复 ffprobe。
 */
fn duration_cache() -> &'static RwLock<HashMap<String, DurationCacheEntry>> {
    static CACHE: OnceLock<RwLock<HashMap<String, DurationCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// 时长缓存兜底 TTL：文件系统元数据异常时，超过该时长强制重新 ffprobe
const DURATION_CACHE_TTL: Duration = Duration::from_secs(5);

/**
 带缓存的视频时长探测。
 参数:
 - `video_path`: 视频文件绝对路径字符串。
 返回:
 - 成功: 时长（秒）。
 - 失败: 错误描述（ffprobe 异常或解析失败）。
 */
pub fn probe_video_duration_cached(video_path: &str) -> Result<f32, String> {
    let current_meta = fs::metadata(video_path).ok();
    let current_mtime = current_meta.as_ref().and_then(|m| m.modified().ok());
    let current_size = current_meta.as_ref().map(|m| m.len());

    if let Ok(guard) = duration_cache().read() {
        if let Some(entry) = guard.get(video_path) {
            let meta_match = entry.file_mtime == current_mtime && entry.file_size == current_size;
            let ttl_ok = entry.cached_at.elapsed() < DURATION_CACHE_TTL;
            if meta_match && ttl_ok {
                return Ok(entry.duration);
            }
        }
    }
    let d = probe_video_duration(video_path)?;
    if let Ok(mut guard) = duration_cache().write() {
        guard.insert(
            video_path.to_string(),
            DurationCacheEntry {
                duration: d,
                file_mtime: current_mtime,
                file_size: current_size,
                cached_at: Instant::now(),
            },
        );
    }
    Ok(d)
}

/**
 按最小时长过滤视频列表：剔除 `duration < min_duration` 的素材，
 用于避免片段选片命中后在 ffmpeg `-ss start_offset -t duration` 时产出空视频流。

 参数:
 - `videos`: 候选视频路径。
 - `min_duration`: 要求的最小时长（秒），通常 = segment 起点累计偏移 + 该段截取时长。
 返回:
 - 满足时长要求的视频路径列表，顺序保持不变。
 探测失败的素材视为不可用（过滤掉），并打 warn 日志。
 */
pub fn filter_videos_by_min_duration(videos: &[PathBuf], min_duration: f32) -> Vec<PathBuf> {
    videos
        .iter()
        .filter(|v| {
            let p = v.to_string_lossy().to_string();
            match probe_video_duration_cached(&p) {
                Ok(d) => d + 0.05 >= min_duration,
                Err(e) => {
                    warn!("素材时长探测失败，已跳过: {} ({})", p, e);
                    false
                }
            }
        })
        .cloned()
        .collect()
}

fn apply_chained_xfade(
    task_id: &str,
    cancel: &Arc<AtomicBool>,
    segment_files: &[PathBuf],
    segment_durations: &[f32],
    transition_types: &[String],
    output: &PathBuf,
    transition_duration: f32,
    output_width: u32,
    output_height: u32,
    background_audio_path: &str,
) -> Result<(), String> {
    if segment_files.is_empty() {
        return Err("没有片段文件".to_string());
    }

    if segment_files.len() == 1 {
        info!("只有一个片段，直接复制");
        fs::copy(&segment_files[0], output).map_err(|e| format!("复制文件失败: {}", e))?;
        return Ok(());
    }

    info!("开始链式转场处理，共 {} 个片段", segment_files.len());

    let mut args: Vec<String> = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(), "error".to_string(),
    ];

    for segment in segment_files {
        args.extend(["-i".to_string(), segment.to_string_lossy().to_string()]);
    }

    let output_str = output.to_string_lossy().to_string();
    let num_cpus_str = num_cpus::get().to_string();
    let transition_duration_str = transition_duration.to_string();
    let filter_complex_base = build_chained_xfade_filter_complex(
        segment_files.len(),
        segment_durations,
        transition_types,
        &transition_duration_str,
        transition_duration,
        output_width,
        output_height,
    );

    let encoder = detect_best_encoder();

    if !background_audio_path.is_empty() {
        args.extend(["-i".to_string(), background_audio_path.to_string()]);

        let total_duration = segment_durations.iter().fold(0.0, |acc, &d| acc + d)
            - (transition_duration * (segment_files.len() - 1) as f32);

        let audio_input_index = segment_files.len();
        let fade_out_start = (total_duration - 1.0).max(0.0);

        let filter_complex = format!(
            "{};[{}:a]volume=0.8,fade=t=in:st=0:d=1,fade=t=out:st={}:d=1[a]",
            filter_complex_base, audio_input_index, fade_out_start
        );

        args.extend([
            "-filter_complex".to_string(), filter_complex,
            "-map".to_string(), "[final_video]".to_string(),
            "-map".to_string(), "[a]".to_string(),
            "-c:v".to_string(), encoder.video_codec.to_string(),
            "-c:a".to_string(), encoder.audio_codec.to_string(),
            "-pix_fmt".to_string(), "yuv420p".to_string(),
            "-movflags".to_string(), "+faststart".to_string(),
            "-threads".to_string(), num_cpus_str,
            "-y".to_string(),
            output_str,
        ]);
    } else {
        args.extend([
            "-filter_complex".to_string(), filter_complex_base,
            "-map".to_string(), "[final_video]".to_string(),
            "-c:v".to_string(), encoder.video_codec.to_string(),
            "-an".to_string(),
            "-pix_fmt".to_string(), "yuv420p".to_string(),
            "-movflags".to_string(), "+faststart".to_string(),
            "-threads".to_string(), num_cpus_str,
            "-y".to_string(),
            output_str,
        ]);
    }

    run_ffmpeg_with_cancel(task_id, cancel, &args)?;
    info!("转场处理完成");
    Ok(())
}

/**
 构建链式 xfade 的 filter_complex 字符串。
 */
fn build_chained_xfade_filter_complex(
    segment_count: usize,
    segment_durations: &[f32],
    transition_types: &[String],
    transition_duration_str: &str,
    transition_duration: f32,
    output_width: u32,
    output_height: u32,
) -> String {
    let mut filter_parts: Vec<String> = Vec::new();

    for i in 0..segment_count {
        filter_parts.push(format!(
            "[{}:v]settb=AVTB,setpts=PTS-STARTPTS,scale={}:{},setsar=sar=1,fps=30,format=yuv420p,setrange=tv[v{}]",
            i, output_width, output_height, i
        ));
    }

    let mut offset = 0.0;
    let mut current_label = "[v0]".to_string();
    for i in 1..segment_count {
        if i == 1 {
            offset = segment_durations[0] - transition_duration;
        } else {
            offset += segment_durations[i - 1] - transition_duration;
        }

        let output_label = format!("[v{}_out]", i);
        filter_parts.push(format!(
            "{}[v{}]xfade=transition={}:duration={}:offset={}{}",
            current_label,
            i,
            transition_types[i - 1],
            transition_duration_str,
            offset,
            output_label
        ));
        current_label = output_label;
    }

    format!(
        "{};{}format=yuv420p[final_video]",
        filter_parts.join(";"),
        current_label
    )
}

fn process_single_mode(
    task_id: &str,
    cancel: &Arc<AtomicBool>,
    tasks: &Arc<RwLock<Vec<Task>>>,
    video_index: usize,
    template_segments: &[config::TemplateSegment],
    tutorial_folder: &str,
    video_ratio: &str,
    audio_path: &str,
    audio_duration: f32,
    subtitle_path: &str,
    output_dir: &PathBuf,
    output_filename: &str,
    // 每个 source_folder 独立的已用集合；仅保证单次任务内同文件夹不重复选中。
    used_per_folder: &mut HashMap<String, HashSet<String>>,
    // 教程素材按配置隔离的已用集合；本视频内挑中的需要立即写入内存，持久化留到任务保存时统一处理。
    tutorial_used_by_config: &Arc<RwLock<HashMap<String, HashSet<String>>>>,
    config_id: &str,
    _app_data_file: &Arc<RwLock<Option<PathBuf>>>,
    _root_folder: &str,
    enable_transition: bool,
    transition_duration: f32,
    // 任务级别共享的临时目录
    temp_dir: &PathBuf,
) -> Result<(), String> {
    let (output_width, output_height) = calculate_video_dimensions(video_ratio);
    // 使用任务级别共享的临时目录，每个视频在其中创建独立子目录
    let video_temp_dir = temp_dir.join(format!("video_{}", video_index));
    fs::create_dir_all(&video_temp_dir).map_err(|e| e.to_string())?;

    let mut segment_files: Vec<PathBuf> = Vec::new();
    let mut first_segment_primary_video: Option<PathBuf> = None;

    let time_points: Vec<f32> = template_segments.iter().map(|s| s.duration).collect();
    let mut segment_offsets = compute_segment_offsets(&time_points);
    let mut segment_durations = compute_segment_durations(&time_points);

    // 如果启用转场效果，除第一个片段外，后续片段的起点都增加转场时长
    // 这样可以为转场效果预留空间，避免片段重叠
    if enable_transition {
        for i in 1..segment_offsets.len() {
            segment_offsets[i] += transition_duration;
            segment_durations[i] -= transition_duration;
        }
    }

    for (i, segment) in template_segments.iter().enumerate() {
        if cancel.load(Ordering::SeqCst) {
            return Err("已取消".to_string());
        }

        let scan_step_id = format!("video_{}__scan_{}", video_index, i + 1);
        push_step(tasks, task_id, &scan_step_id, &format!("视频{} - 扫描片段{}素材", video_index, i + 1), StepStatus::Running, None);

        let videos = get_video_files(&segment.source_folder)?;
        if videos.is_empty() {
            let err = format!("片段 {} 的源文件夹中没有视频文件", i + 1);
            push_step(tasks, task_id, &scan_step_id, &format!("视频{} - 扫描片段{}素材", video_index, i + 1), StepStatus::Error, Some(err.clone()));
            return Err(err);
        }

        let video_count = match segment.crop_mode {
            config::CropMode::Single => 1,
            config::CropMode::Dual => 2,
            config::CropMode::Quadrant => 4,
        };

        if videos.len() < video_count {
            let err = format!("片段 {} 需要 {} 个视频文件，但源文件夹中只有 {} 个", i + 1, video_count, videos.len());
            push_step(tasks, task_id, &scan_step_id, &format!("视频{} - 扫描片段{}素材", video_index, i + 1), StepStatus::Error, Some(err.clone()));
            return Err(err);
        }

        // 预过滤时长不足的素材：截取范围是 [start_offset, start_offset + duration]，
        // 否则 ffmpeg 在 dual/quadrant 的 cell 阶段会产出空视频流，merge 时报 "matches no streams"。
        let min_duration = segment_offsets[i] + segment_durations[i];
        let videos = filter_videos_by_min_duration(&videos, min_duration);
        if videos.len() < video_count {
            let err = format!(
                "片段 {} 需要 {} 个时长 ≥ {:.1}s 的视频文件，但符合条件的只有 {} 个",
                i + 1, video_count, min_duration, videos.len()
            );
            push_step(tasks, task_id, &scan_step_id, &format!("视频{} - 扫描片段{}素材", video_index, i + 1), StepStatus::Error, Some(err.clone()));
            return Err(err);
        }

        // S6：仅在同一 source_folder 内去重，跨 folder 互不干扰
        let folder_key = normalize_folder_key(&segment.source_folder);
        let folder_used = used_per_folder.entry(folder_key).or_insert_with(HashSet::new);
        let selected = select_random_videos(&videos, video_count, folder_used)
            .map_err(|e| format!("片段 {} 选取素材失败: {}", i + 1, e))?;
        if i == 0 {
            first_segment_primary_video = selected.first().cloned();
        }
        let source_folder_abs = std::fs::canonicalize(&segment.source_folder)
            .unwrap_or_else(|_| PathBuf::from(&segment.source_folder))
            .to_string_lossy()
            .to_string();
        let selected_summary = selected
            .iter()
            .map(|p| p.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string())
            .collect::<Vec<String>>()
            .join(", ");
        push_log(
            tasks,
            task_id,
            LogLevel::Info,
            video_index,
            format!(
                "片段{} 从文件夹 {} 选中素材 {} 个，裁剪模式 {:?}，偏移 {:.2}s，时长 {:.2}s；文件名：{}",
                i + 1,
                source_folder_abs,
                selected.len(),
                segment.crop_mode,
                segment_offsets[i],
                segment_durations[i],
                selected_summary
            ),
        );
        for s in &selected {
            folder_used.insert(s.to_string_lossy().to_string());
        }

        push_step(tasks, task_id, &scan_step_id, &format!("视频{} - 扫描片段{}素材", video_index, i + 1), StepStatus::Completed, None);

        let process_step_id = format!("video_{}__segment_{}", video_index, i + 1);
        push_step(tasks, task_id, &process_step_id, &format!("视频{} - 处理片段{}", video_index, i + 1), StepStatus::Running, None);

        let trimmed_segment = video_temp_dir.join(format!("segment_{}.mp4", i));
        match process_segment(
            task_id,
            cancel,
            &selected,
            &trimmed_segment,
            &segment.crop_mode,
            (output_width, output_height),
            segment_offsets[i],
            segment_durations[i],
            &video_temp_dir,
            segment.scale_percent,
        ) {
            Ok(()) => {
                push_step(tasks, task_id, &process_step_id, &format!("视频{} - 处理片段{}", video_index, i + 1), StepStatus::Completed, None);
            }
            Err(e) => {
                push_step(tasks, task_id, &process_step_id, &format!("视频{} - 处理片段{}", video_index, i + 1), StepStatus::Error, Some(e.clone()));
                return Err(e);
            }
        }

        segment_files.push(trimmed_segment);
    }

    // 教程片段：必须配置文件夹；按配置去重，用过的素材永不再选；耗尽即报错。
    if tutorial_folder.trim().is_empty() {
        let err = "教程素材文件夹未配置".to_string();
        let tutorial_step_id = format!("video_{}__tutorial", video_index);
        push_step(tasks, task_id, &tutorial_step_id, &format!("视频{} - 处理教程片段", video_index), StepStatus::Error, Some(err.clone()));
        return Err(err);
    }

    let tutorial_videos = get_video_files(tutorial_folder)?;
    if tutorial_videos.is_empty() {
        let err = format!("教程素材文件夹为空: {}", tutorial_folder);
        let tutorial_step_id = format!("video_{}__tutorial", video_index);
        push_step(tasks, task_id, &tutorial_step_id, &format!("视频{} - 处理教程片段", video_index), StepStatus::Error, Some(err.clone()));
        return Err(err);
    }

    let tutorial_step_id = format!("video_{}__tutorial", video_index);
    push_step(tasks, task_id, &tutorial_step_id, &format!("视频{} - 处理教程片段", video_index), StepStatus::Running, None);

    // 从当前配置的已用集合里过滤出可用的教程素材，同时清理已不存在的视频
    let tutorial_video = {
        let used_snapshot: HashSet<String> = tutorial_used_by_config
            .read()
            .ok()
            .and_then(|guard| guard.get(config_id).cloned())
            .unwrap_or_default();
        
        // 清理掉已不存在的视频
        {
            if let Ok(mut guard) = tutorial_used_by_config.write() {
                if let Some(used_set) = guard.get_mut(config_id) {
                    let mut to_remove = Vec::new();
                    for used in used_set.iter() {
                        let path = PathBuf::from(used);
                        if !path.exists() {
                            to_remove.push(used.clone());
                        }
                    }
                    for remove in to_remove {
                        used_set.remove(&remove);
                    }
                }
            }
        }
        
        // 重新获取清理后的 used 集合
        let used_cleaned: HashSet<String> = tutorial_used_by_config
            .read()
            .ok()
            .and_then(|guard| guard.get(config_id).cloned())
            .unwrap_or_default();
        
        let available: Vec<PathBuf> = tutorial_videos
            .iter()
            .filter(|v| !used_cleaned.contains(&v.to_string_lossy().to_string()))
            .cloned()
            .collect();
        
        if available.is_empty() {
            let err = format!(
                "教程素材已全部使用过，请补充新素材到: {}（配置已用 {} 个）",
                tutorial_folder,
                used_cleaned.len()
            );
            push_step(tasks, task_id, &tutorial_step_id, &format!("视频{} - 处理教程片段", video_index), StepStatus::Error, Some(err.clone()));
            return Err(err);
        }
        let mut rng = rand::thread_rng();
        let idx = rng.gen_range(0..available.len());
        available[idx].clone()
    };

    // 立即登记为已用（内存中），防止同配置后续视频重复选取，持久化留到任务保存时统一处理
    // 保存原始教程视频路径用于后续删除
    let tutorial_key = tutorial_video.to_string_lossy().to_string();
    let original_tutorial_path = tutorial_video.clone();
    {
        if let Ok(mut guard) = tutorial_used_by_config.write() {
            guard
                .entry(config_id.to_string())
                .or_insert_with(HashSet::new)
                .insert(tutorial_key.clone());
        }
    }
    push_log(
        tasks,
        task_id,
        LogLevel::Info,
        video_index,
        format!(
            "教程片段从文件夹 {} 选中素材文件名：{}",
            std::fs::canonicalize(tutorial_folder)
                .unwrap_or_else(|_| PathBuf::from(tutorial_folder))
                .to_string_lossy(),
            tutorial_video.file_name().and_then(|n| n.to_str()).unwrap_or_default()
        ),
    );

    let tutorial_scaled = video_temp_dir.join("tutorial_scaled.mp4");

    // 教程片段：仅做格式对齐（setsar/fps/format），不做时间截取，保持完整原始内容（规范化色彩范围）
    let tutorial_scale_filter = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black,setsar=sar=1,fps=30,format=yuv420p,setrange=tv",
        output_width, output_height, output_width, output_height
    );

    let tutorial_scaled_str = tutorial_scaled.to_string_lossy().to_string();
    let tutorial_input_str = tutorial_video.to_string_lossy().to_string();
    let encoder = detect_best_encoder();
    let num_cpus_str = num_cpus::get().to_string();

    let mut tutorial_args: Vec<String> = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(), "error".to_string(),
    ];

    // 如果启用转场效果，教程视频从转场时长秒数开始截取
    if enable_transition {
        tutorial_args.push("-ss".to_string());
        tutorial_args.push(transition_duration.to_string());
    }

    tutorial_args.extend(vec![
        "-i".to_string(), tutorial_input_str,
        "-vf".to_string(), tutorial_scale_filter,
        "-c:v".to_string(), encoder.video_codec.to_string(),
        "-c:a".to_string(), encoder.audio_codec.to_string(),
        "-pix_fmt".to_string(), "yuv420p".to_string(),
        "-threads".to_string(), num_cpus_str,
        "-y".to_string(),
        tutorial_scaled_str,
    ]);

    match run_ffmpeg_with_cancel(task_id, cancel, &tutorial_args) {
        Ok(()) => {
            push_step(tasks, task_id, &tutorial_step_id, &format!("视频{} - 处理教程片段", video_index), StepStatus::Completed, None);
            // 教程片段不加入segment_files参与转场，后续直接拼接
        }
        Err(e) => {
            push_step(tasks, task_id, &tutorial_step_id, &format!("视频{} - 处理教程片段", video_index), StepStatus::Error, Some(e.clone()));
            return Err(e);
        }
    }

    let output_path = output_dir.join(output_filename);
    let temp_output_path = video_temp_dir.join("temp_output.mp4");
    let temp_with_tutorial_path = video_temp_dir.join("temp_with_tutorial.mp4");

    let mut segment_durations: Vec<f32> = Vec::new();
    for (i, segment_file) in segment_files.iter().enumerate() {
        let duration = probe_video_duration(&segment_file.to_string_lossy())?;
        segment_durations.push(duration);
        info!("片段 {} 时长: {:.2}秒", i + 1, duration);
    }

    let xfade_step_id = format!("video_{}__xfade", video_index);
    
    if enable_transition {
        let mut transition_types: Vec<String> = Vec::new();
        for i in 0..segment_files.len().saturating_sub(1) {
            let transition = get_random_transition_type();
            transition_types.push(transition.clone());
            info!("转场 {} 类型: {}", i + 1, transition);
        }

        push_step(tasks, task_id, &xfade_step_id, &format!("视频{} - 链式转场", video_index), StepStatus::Running, None);

        info!("开始应用链式转场...");
        // 先不对模板片段添加音频，音频将在最后与教程片段合并时添加
        match apply_chained_xfade(
            task_id, cancel,
            &segment_files,
            &segment_durations,
            &transition_types,
            &temp_output_path,
            transition_duration,
            output_width,
            output_height,
            "",  // 暂时不传音频
        ) {
            Ok(()) => push_step(tasks, task_id, &xfade_step_id, &format!("视频{} - 链式转场", video_index), StepStatus::Completed, None),
            Err(e) => {
                push_step(tasks, task_id, &xfade_step_id, &format!("视频{} - 链式转场", video_index), StepStatus::Error, Some(e.clone()));
                return Err(e);
            }
        }
    } else {
        // 不启用转场效果，直接拼接片段
        push_step(tasks, task_id, &xfade_step_id, &format!("视频{} - 拼接片段", video_index), StepStatus::Running, None);

        info!("开始直接拼接片段...");
        // 使用 concat 滤镜直接拼接多个视频文件
        // 注意：-i 和文件路径必须是独立的参数，不能合并
        let mut input_args: Vec<String> = Vec::new();
        for f in &segment_files {
            input_args.push("-i".to_string());
            input_args.push(f.to_string_lossy().to_string());
        }
        
        // 构建 concat 滤镜：先缩放每个输入视频，然后拼接
        let mut filter_parts: Vec<String> = Vec::new();
        for i in 0..segment_files.len() {
            filter_parts.push(format!("[{}:v]scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2,setsar=sar=1,fps=30,format=yuv420p,setrange=tv[v{}]",
                i, output_width, output_height, output_width, output_height, i));
        }
        let scaled_streams: String = (0..segment_files.len()).map(|i| format!("[v{}]", i)).collect::<Vec<_>>().join("");
        filter_parts.push(format!("{}concat=n={}:v=1:a=0[out]", scaled_streams, segment_files.len()));
        let filter_complex = filter_parts.join(";");
        
        let output_path_str = temp_output_path.to_string_lossy().to_string();
        let mut ffmpeg_args = vec!["-y".to_string()];
        ffmpeg_args.extend(input_args);
        ffmpeg_args.extend([
            "-filter_complex".to_string(), filter_complex,
            "-map".to_string(), "[out]".to_string(),
            "-c:v".to_string(), "libx264".to_string(),
            "-preset".to_string(), "fast".to_string(),
            "-crf".to_string(), "23".to_string(),
            "-pix_fmt".to_string(), "yuv420p".to_string(),
            output_path_str,
        ]);
        
        match run_ffmpeg_with_cancel(task_id, cancel, &ffmpeg_args) {
            Ok(()) => push_step(tasks, task_id, &xfade_step_id, &format!("视频{} - 拼接片段", video_index), StepStatus::Completed, None),
            Err(e) => {
                push_step(tasks, task_id, &xfade_step_id, &format!("视频{} - 拼接片段", video_index), StepStatus::Error, Some(e.clone()));
                return Err(e);
            }
        }
    }

    // 处理模板片段和教程片段的合并（仅视频转场，音频稍后添加）
    let tutorial_scaled = video_temp_dir.join("tutorial_scaled.mp4");
    if tutorial_scaled.exists() {
        let concat_step_id = format!("video_{}__concat", video_index);
        
        if enable_transition {
            push_step(tasks, task_id, &concat_step_id, &format!("视频{} - 拼接教程片段（带转场）", video_index), StepStatus::Running, None);

            // 模板视频实际时长（temp_output_path 已经是链式 xfade 后的产物）
            let template_total_duration = probe_video_duration(&temp_output_path.to_string_lossy())?;
            info!("模板视频时长: {:.2}秒", template_total_duration);
            push_log(tasks, task_id, LogLevel::Info, video_index, format!("模板视频时长: {:.2}秒", template_total_duration));

            // 教程片段时长
            let tutorial_duration = probe_video_duration(&tutorial_scaled.to_string_lossy())?;
            info!("教程片段时长: {:.2}秒", tutorial_duration);
            push_log(tasks, task_id, LogLevel::Info, video_index, format!("教程片段时长: {:.2}秒", tutorial_duration));

            // 单次 xfade：模板尾部 transition_duration 秒与教程头部交叠
            let tutorial_transition = get_random_transition_type();
            info!("模板与教程转场类型: {}", tutorial_transition);
            push_log(tasks, task_id, LogLevel::Info, video_index, format!("模板与教程转场类型: {}", tutorial_transition));
            let xfade_offset = (template_total_duration - transition_duration).max(0.0);

            let encoder = detect_best_encoder();
            let num_cpus_str = num_cpus::get().to_string();

            let args: Vec<String> = vec![
                "-hide_banner".to_string(),
                "-loglevel".to_string(), "error".to_string(),
                "-i".to_string(), temp_output_path.to_string_lossy().to_string(),
                "-i".to_string(), tutorial_scaled.to_string_lossy().to_string(),
                "-filter_complex".to_string(),
                format!(
                    "[0:v][1:v]xfade=transition={}:duration={}:offset={}[final_video]",
                    tutorial_transition, transition_duration, xfade_offset
                ),
                "-map".to_string(), "[final_video]".to_string(),
                "-c:v".to_string(), encoder.video_codec.to_string(),
                "-pix_fmt".to_string(), "yuv420p".to_string(),
                "-movflags".to_string(), "+faststart".to_string(),
                "-threads".to_string(), num_cpus_str,
                "-y".to_string(),
                temp_with_tutorial_path.to_string_lossy().to_string(),
            ];

            match run_ffmpeg_with_cancel(task_id, cancel, &args) {
                Ok(()) => {
                    push_step(tasks, task_id, &concat_step_id, &format!("视频{} - 拼接教程片段（带转场）", video_index), StepStatus::Completed, None);
                }
                Err(e) => {
                    push_step(tasks, task_id, &concat_step_id, &format!("视频{} - 拼接教程片段（带转场）", video_index), StepStatus::Error, Some(e.clone()));
                    return Err(e);
                }
            }
        } else {
            // 不启用转场效果，直接拼接模板和教程片段
            push_step(tasks, task_id, &concat_step_id, &format!("视频{} - 拼接教程片段", video_index), StepStatus::Running, None);
            
            let output_path_str = temp_with_tutorial_path.to_string_lossy().to_string();
            let ffmpeg_args: Vec<String> = vec![
                "-hide_banner".to_string(),
                "-loglevel".to_string(), "error".to_string(),
                "-i".to_string(), temp_output_path.to_string_lossy().to_string(),
                "-i".to_string(), tutorial_scaled.to_string_lossy().to_string(),
                "-filter_complex".to_string(), "[0:v][1:v]concat=n=2:v=1:a=0[out]".to_string(),
                "-map".to_string(), "[out]".to_string(),
                "-c:v".to_string(), "libx264".to_string(),
                "-preset".to_string(), "fast".to_string(),
                "-crf".to_string(), "23".to_string(),
                "-pix_fmt".to_string(), "yuv420p".to_string(),
                "-y".to_string(),
                output_path_str,
            ];

            match run_ffmpeg_with_cancel(task_id, cancel, &ffmpeg_args) {
                Ok(()) => push_step(tasks, task_id, &concat_step_id, &format!("视频{} - 拼接教程片段", video_index), StepStatus::Completed, None),
                Err(e) => {
                    push_step(tasks, task_id, &concat_step_id, &format!("视频{} - 拼接教程片段", video_index), StepStatus::Error, Some(e.clone()));
                    return Err(e);
                }
            }
        }
    } else {
        // 如果没有教程片段，直接复制转场后的输出（仅视频，音频稍后添加）
        fs::copy(&temp_output_path, &temp_with_tutorial_path)
            .map_err(|e| format!("复制视频文件失败: {}", e))?;
    }

    // 音频处理：直接按最短长度截断，不做素材补充
    // 若视频比音频长，截断视频；若音频比视频长，截断音频
    if !audio_path.is_empty() {
        let audio_step_id = format!("video_{}__audio", video_index);
        push_step(tasks, task_id, &audio_step_id, &format!("视频{} - 添加音频", video_index), StepStatus::Running, None);

        let actual_audio_duration = probe_video_duration(audio_path)
            .unwrap_or_else(|e| {
                info!("ffprobe 音频时长失败，回退使用配置值 {:.2}s: {}", audio_duration, e);
                push_log(tasks, task_id, LogLevel::Warn, video_index, format!("ffprobe 音频时长失败，回退使用配置值 {:.2}s: {}", audio_duration, e));
                audio_duration
            });
        
        let video_duration = probe_video_duration(&temp_with_tutorial_path.to_string_lossy())?;
        
        info!("当前视频时长: {:.2}秒, 音频实际时长: {:.2}秒（配置值 {:.2}秒）",
            video_duration, actual_audio_duration, audio_duration);
        push_log(
            tasks,
            task_id,
            LogLevel::Info,
            video_index,
            format!(
                "当前视频时长: {:.2}秒, 音频实际时长: {:.2}秒（配置值 {:.2}秒）",
                video_duration, actual_audio_duration, audio_duration
            ),
        );
        
        // 取最短时长作为最终输出时长
        let final_duration = if video_duration < actual_audio_duration {
            info!("视频时长 {:.2}s < 音频时长 {:.2}s，将截断音频", video_duration, actual_audio_duration);
            push_log(tasks, task_id, LogLevel::Info, video_index, 
                format!("视频时长 {:.2}s < 音频时长 {:.2}s，将截断音频", video_duration, actual_audio_duration));
            video_duration
        } else {
            info!("音频时长 {:.2}s <= 视频时长 {:.2}s，将截断视频", actual_audio_duration, video_duration);
            push_log(tasks, task_id, LogLevel::Info, video_index, 
                format!("音频时长 {:.2}s <= 视频时长 {:.2}s，将截断视频", actual_audio_duration, video_duration));
            actual_audio_duration
        };

        let encoder = detect_best_encoder();
        let num_cpus_str = num_cpus::get().to_string();
        let temp_with_audio_path = video_temp_dir.join("temp_with_audio.mp4");
        let final_duration_str = final_duration.to_string();

        let args: Vec<String> = vec![
            "-hide_banner".to_string(),
            "-loglevel".to_string(), "error".to_string(),
            "-i".to_string(), temp_with_tutorial_path.to_string_lossy().to_string(),
            "-i".to_string(), audio_path.to_string(),
            "-filter_complex".to_string(),
            format!("[1:a]volume=0.8,atrim=start=0:duration={}[a]", final_duration_str),
            "-map".to_string(), "0:v".to_string(),
            "-map".to_string(), "[a]".to_string(),
            "-c:v".to_string(), "copy".to_string(),
            "-c:a".to_string(), encoder.audio_codec.to_string(),
            "-t".to_string(), final_duration_str,
            "-movflags".to_string(), "+faststart".to_string(),
            "-threads".to_string(), num_cpus_str,
            "-y".to_string(),
            temp_with_audio_path.to_string_lossy().to_string(),
        ];

        match run_ffmpeg_with_cancel(task_id, cancel, &args) {
            Ok(()) => {
                push_step(tasks, task_id, &audio_step_id, &format!("视频{} - 添加音频", video_index), StepStatus::Completed, None);
                fs::copy(&temp_with_audio_path, &temp_with_tutorial_path)
                    .map_err(|e| format!("复制音频处理后的文件失败: {}", e))?;
            }
            Err(e) => {
                push_step(tasks, task_id, &audio_step_id, &format!("视频{} - 添加音频", video_index), StepStatus::Error, Some(e.clone()));
                return Err(e);
            }
        }
    }

    if !subtitle_path.is_empty() {
        let sub_step_id = format!("video_{}__subtitle", video_index);
        push_step(tasks, task_id, &sub_step_id, &format!("视频{} - 添加字幕", video_index), StepStatus::Running, None);
        match add_subtitles(task_id, cancel, &temp_with_tutorial_path, subtitle_path, &output_path) {
            Ok(()) => push_step(tasks, task_id, &sub_step_id, &format!("视频{} - 添加字幕", video_index), StepStatus::Completed, None),
            Err(e) => {
                push_step(tasks, task_id, &sub_step_id, &format!("视频{} - 添加字幕", video_index), StepStatus::Error, Some(e.clone()));
                return Err(e);
            }
        }
    } else {
        fs::copy(&temp_with_tutorial_path, &output_path)
            .map_err(|e| format!("复制视频文件失败: {}", e))?;
    }

    // 视频生成完成后自动删除原始教程视频文件
    if original_tutorial_path.exists() {
        match fs::remove_file(&original_tutorial_path) {
            Ok(()) => {
                info!("已删除原始教程视频: {:?}", original_tutorial_path);
                push_log(
                    tasks,
                    task_id,
                    LogLevel::Info,
                    video_index,
                    format!("已删除原始教程视频: {}", original_tutorial_path.file_name().and_then(|n| n.to_str()).unwrap_or_default()),
                );
            }
            Err(e) => {
                warn!("删除原始教程视频失败: {:?}, 错误: {}", original_tutorial_path, e);
                push_log(
                    tasks,
                    task_id,
                    LogLevel::Warn,
                    video_index,
                    format!("删除原始教程视频失败: {}", e),
                );
            }
        }
    }

    Ok(())
}

struct SrtEntry {
    start_secs: f64,
    end_secs: f64,
    text: String,
}

fn parse_srt_timestamp(ts: &str) -> Result<f64, String> {
    let ts = ts.trim().replace(',', ".");
    let parts: Vec<&str> = ts.split(':').collect();
    if parts.len() != 3 {
        return Err(format!("无效的 SRT 时间戳: {}", ts));
    }
    let hours: f64 = parts[0].parse().map_err(|_| format!("无效的小时: {}", parts[0]))?;
    let minutes: f64 = parts[1].parse().map_err(|_| format!("无效的分钟: {}", parts[1]))?;
    let seconds: f64 = parts[2].parse().map_err(|_| format!("无效的秒: {}", parts[2]))?;
    Ok(hours * 3600.0 + minutes * 60.0 + seconds)
}

fn parse_srt_content(content: &str) -> Result<Vec<SrtEntry>, String> {
    let mut entries = Vec::new();
    let normalized = content.replace("\r\n", "\n");
    let blocks: Vec<&str> = normalized.split("\n\n").collect();

    for block in blocks {
        let block = block.trim();
        if block.is_empty() {
            continue;
        }
        let lines: Vec<&str> = block.lines().collect();
        if lines.len() < 3 {
            continue;
        }
        let ts_line = lines[1].trim();
        let ts_parts: Vec<&str> = ts_line.split("-->").collect();
        if ts_parts.len() != 2 {
            continue;
        }
        let start_secs = parse_srt_timestamp(ts_parts[0])?;
        let end_secs = parse_srt_timestamp(ts_parts[1])?;
        let text = lines[2..].join("\\n");
        entries.push(SrtEntry { start_secs, end_secs, text });
    }

    if entries.is_empty() {
        return Err("SRT 文件中没有有效的字幕条目".to_string());
    }
    Ok(entries)
}

fn escape_drawtext_text(text: &str) -> String {
    text.replace('\\', "\\\\")
        .replace('\'', "\\'")
        .replace(':', "\\:")
        .replace('%', "\\%")
        .replace('{', "\\{")
        .replace('}', "\\}")
}

fn build_drawtext_subtitle_filter(entries: &[SrtEntry], fontfile: &str) -> String {
    let mut parts: Vec<String> = Vec::new();

    for entry in entries {
        let escaped_text = escape_drawtext_text(&entry.text);
        let start = entry.start_secs;
        let end = entry.end_secs;

        let drawtext = format!(
            "drawtext=fontfile='{}':text='{}':fontsize=28:fontcolor=white@0.92:borderw=2.5:bordercolor=black@0.55:line_spacing=6:x=(w-tw)/2:y=h-th-80:enable='between(t,{},{})'",
            fontfile, escaped_text, start, end
        );
        parts.push(drawtext);
    }

    parts.join(",")
}

fn add_subtitles(
    task_id: &str,
    cancel: &Arc<AtomicBool>,
    input_path: &PathBuf,
    subtitle_path: &str,
    output_path: &PathBuf,
) -> Result<(), String> {
    info!("添加字幕: subtitle_path={}", subtitle_path);

    if subtitle_path.trim().is_empty() {
        info!("字幕路径为空，跳过字幕添加");
        fs::copy(input_path, output_path).map_err(|e| format!("复制视频文件失败: {}", e))?;
        return Ok(());
    }

    let subtitle_path_buf = PathBuf::from(subtitle_path);
    if !subtitle_path_buf.exists() {
        error!("字幕文件不存在: {}", subtitle_path);
        fs::copy(input_path, output_path).map_err(|e| format!("复制视频文件失败: {}", e))?;
        return Ok(());
    }

    let input_str = input_path.to_string_lossy().to_string();
    let output_str = output_path.to_string_lossy().to_string();
    let encoder = detect_best_encoder();

    let temp_dir = std::env::temp_dir();
    let temp_subtitle_path = temp_dir.join(format!("temp_subtitle_{}.srt", Uuid::new_v4()));

    fs::copy(&subtitle_path_buf, &temp_subtitle_path).map_err(|e| format!("复制字幕文件失败: {}", e))?;
    info!("已复制字幕文件到临时位置: {:?}", temp_subtitle_path);

    let subtitle_ext = subtitle_path_buf.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
    let is_ass_format = subtitle_ext == "ass" || subtitle_ext == "ssa";

    let temp_subtitle_str = temp_subtitle_path.to_string_lossy().to_string();
    let escaped_path = temp_subtitle_str.replace('\\', "\\\\").replace(":", "\\:");
    let filter_str = if is_ass_format {
        format!("ass=filename={}", escaped_path)
    } else {
        format!("subtitles=filename={}:si=0", escaped_path)
    };

    let args: Vec<String> = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(), "error".to_string(),
        "-i".to_string(), input_str.clone(),
        "-vf".to_string(), filter_str,
        "-c:v".to_string(), encoder.video_codec.to_string(),
        "-c:a".to_string(), "copy".to_string(),
        "-pix_fmt".to_string(), "yuv420p".to_string(),
        "-movflags".to_string(), "+faststart".to_string(),
        "-y".to_string(),
        output_str.clone(),
    ];

    let result = run_ffmpeg_with_cancel(task_id, cancel, &args);
    let _ = fs::remove_file(&temp_subtitle_path);

    match result {
        Ok(_) => {
            info!("字幕添加成功: {:?}", output_path);
            return Ok(());
        }
        Err(e) => {
            error!("subtitles/ass 滤镜失败: {}", e);
            warn!("提示：若需要字幕支持，请使用 Homebrew 安装完整版本的 FFmpeg：brew install ffmpeg-full");
        }
    }

    // subtitles/ass 滤镜不可用，回退到 drawtext 方案
    if is_ass_format {
        warn!("ASS 字幕回退到 drawtext 方案不支持，跳过字幕");
        fs::copy(input_path, output_path).map_err(|e| format!("复制视频文件失败: {}", e))?;
        return Ok(());
    }

    info!("尝试 drawtext 字幕方案");
    let srt_content = fs::read_to_string(subtitle_path).map_err(|e| format!("读取字幕文件失败: {}", e))?;
    let entries = match parse_srt_content(&srt_content) {
        Ok(e) => e,
        Err(e) => {
            error!("SRT 解析失败: {}", e);
            fs::copy(input_path, output_path).map_err(|e| format!("复制视频文件失败: {}", e))?;
            return Ok(());
        }
    };

    let fontfile = "/System/Library/Fonts/STHeiti Light.ttc";
    let drawtext_filter = build_drawtext_subtitle_filter(&entries, fontfile);

    info!("drawtext 滤镜已构建，共 {} 条字幕", entries.len());

    let drawtext_args: Vec<String> = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(), "error".to_string(),
        "-i".to_string(), input_str,
        "-vf".to_string(), drawtext_filter,
        "-c:v".to_string(), encoder.video_codec.to_string(),
        "-c:a".to_string(), "copy".to_string(),
        "-pix_fmt".to_string(), "yuv420p".to_string(),
        "-movflags".to_string(), "+faststart".to_string(),
        "-y".to_string(),
        output_str,
    ];

    match run_ffmpeg_with_cancel(task_id, cancel, &drawtext_args) {
        Ok(_) => info!("drawtext 字幕添加成功: {:?}", output_path),
        Err(e) => {
            error!("drawtext 字幕方案也失败: {}", e);
            warn!("提示：若需要字幕支持，请使用 Homebrew 安装完整版本的 FFmpeg：brew install ffmpeg-full");
            fs::copy(input_path, output_path).map_err(|e| format!("复制视频文件失败: {}", e))?;
        }
    }

    Ok(())
}

/**
 解析任务级输出目录。
 - 当 `output_folder` 非空: `<output_folder>/<task_name>/`；
 - 当 `output_folder` 为空且 `root_folder` 非空: `<root_folder>/output/<task_name>/`；
 - 两者都为空时返回错误。
 */
fn resolve_task_output_dir(
    output_folder: &str,
    root_folder: &str,
    task_name: &str,
) -> Result<PathBuf, String> {
    let safe_name = sanitize_task_name(task_name);
    let base = if !output_folder.trim().is_empty() {
        PathBuf::from(output_folder.trim())
    } else if !root_folder.trim().is_empty() {
        PathBuf::from(root_folder.trim()).join("output")
    } else {
        return Err("未配置输出目录，且主目录也为空，无法确定输出位置".to_string());
    };
    Ok(base.join(safe_name))
}

#[tauri::command]
pub fn create_task(state: tauri::State<AppState>, config_name: String, count: usize) -> Result<Task, String> {
    info!("创建任务: config_name={}, count={}", config_name, count);

    let configs = state.configs.read().map_err(|e| e.to_string())?;
    let config = configs.iter().find(|c| c.name == config_name).cloned();
    drop(configs);

    let config = config.ok_or_else(|| format!("配置 '{}' 不存在", config_name))?;

    let mut task = Task::new(config_name.clone(), count);
    task.config_id = config.id.clone();

    let task_output_dir = resolve_task_output_dir(
        &config.output_folder,
        &config.root_folder,
        &task.task_name,
    )?;
    task.output_folder = task_output_dir.to_string_lossy().to_string();

    let mut tasks = state.tasks.write().map_err(|e| e.to_string())?;
    tasks.push(task.clone());
    info!("任务创建成功: id={}, output_dir={:?}", task.id, task_output_dir);
    drop(tasks);

    let cancel_flag = register_cancel(&task.id);
    let pause_flag = register_pause(&task.id);

    let tasks_clone = state.tasks.clone();
    let configs_clone = state.configs.clone();
    let task_id = task.id.clone();
    let config_clone = config.clone();
    let output_dir = task_output_dir.clone();
    let tutorial_used_by_config = state.used_tutorial_videos.clone();
    let app_data_file = state.app_data_file.clone();

    thread::spawn(move || {
        info!("开始处理任务: id={}", task_id);

        push_step(&tasks_clone, &task_id, "init", "初始化任务", StepStatus::Running, None);

        {
            let mut tasks = tasks_clone.write().unwrap();
            for t in tasks.iter_mut() {
                if t.id == task_id {
                    t.status = TaskStatus::Running;
                    t.started_at = Some(chrono::Utc::now());
                    break;
                }
            }
        }

        if !output_dir.exists() {
            if let Err(e) = fs::create_dir_all(&output_dir) {
                error!("创建输出目录失败: {}", e);
                push_step(&tasks_clone, &task_id, "init", "初始化任务", StepStatus::Error, Some(format!("创建输出目录失败: {}", e)));
                let mut tasks = tasks_clone.write().unwrap();
                for t in tasks.iter_mut() {
                    if t.id == task_id {
                        t.status = TaskStatus::Error;
                        t.error_message = Some(format!("创建输出目录失败: {}", e));
                        break;
                    }
                }
                unregister_task(&task_id);
                return;
            }
        }

        push_step(&tasks_clone, &task_id, "init", "初始化任务", StepStatus::Completed, None);

        // 创建任务级别的共享临时目录
        let task_temp_dir = std::env::temp_dir().join(format!("video_mixer_{}", task_id));
        if let Err(e) = fs::create_dir_all(&task_temp_dir) {
            error!("创建任务临时目录失败: {}", e);
            push_step(&tasks_clone, &task_id, "init", "初始化任务", StepStatus::Error, Some(format!("创建临时目录失败: {}", e)));
            let mut tasks = tasks_clone.write().unwrap();
            for t in tasks.iter_mut() {
                if t.id == task_id {
                    t.status = TaskStatus::Error;
                    t.error_message = Some(format!("创建临时目录失败: {}", e));
                    break;
                }
            }
            unregister_task(&task_id);
            return;
        }

        // 并发处理准备
        let (tx, rx) = std::sync::mpsc::channel();
        let rx = Arc::new(Mutex::new(rx));
        let completed_count = Arc::new(Mutex::new(0));
        let failed_count = Arc::new(Mutex::new(0));
        let failed_videos = Arc::new(Mutex::new(Vec::new()));
        let used_per_folder = Arc::new(Mutex::new(HashMap::<String, HashSet<String>>::new()));
        let active_workers = Arc::new(Mutex::new(0));
        
        // 确定工作线程数量（与FFmpeg并发限制一致）
        let worker_count = {
            let s = ffmpeg_semaphore();
            let inner = s.inner.lock().unwrap();
            *inner
        };
        info!("任务 {} 使用 {} 个工作线程", task_id, worker_count);

        // 发送任务到channel
        for i in 0..count {
            tx.send(i + 1).unwrap();
        }
        drop(tx); // 关闭发送端

        // 启动工作线程
        let mut handles = Vec::new();
        for worker_id in 0..worker_count {
            let rx = rx.clone();
            let task_id = task_id.clone();
            let cancel_flag = cancel_flag.clone();
            let pause_flag = pause_flag.clone();
            let tasks_clone = tasks_clone.clone();
            let config_clone = config_clone.clone();
            let output_dir = output_dir.clone();
            let task_temp_dir = task_temp_dir.clone();
            let tutorial_used_by_config = tutorial_used_by_config.clone();
            let app_data_file = app_data_file.clone();
            let completed_count = completed_count.clone();
            let failed_count = failed_count.clone();
            let failed_videos = failed_videos.clone();
            let used_per_folder = used_per_folder.clone();
            let active_workers = active_workers.clone();

            let handle = thread::spawn(move || {
                *active_workers.lock().unwrap() += 1;
                info!("工作线程 {} 启动", worker_id);
                
                loop {
                    let video_index = {
                        let rx_guard = rx.lock().unwrap();
                        match rx_guard.recv() {
                            Ok(idx) => idx,
                            Err(_) => break, // channel已关闭，退出循环
                        }
                    };
                    // 检查取消
                    if cancel_flag.load(Ordering::SeqCst) {
                        info!("工作线程 {} 收到取消信号，退出", worker_id);
                        break;
                    }

                    // 暂停等待
                    while pause_flag.load(Ordering::SeqCst) && !cancel_flag.load(Ordering::SeqCst) {
                        thread::sleep(Duration::from_millis(300));
                    }
                    if cancel_flag.load(Ordering::SeqCst) {
                        info!("工作线程 {} 收到取消信号，退出", worker_id);
                        break;
                    }

                    info!("工作线程 {} 处理第 {} 个视频", worker_id, video_index);

                    let video_step_id = format!("video_{}", video_index);
                    push_step(&tasks_clone, &task_id, &video_step_id, &format!("视频{} - 开始处理", video_index), StepStatus::Running, None);
                    push_log(&tasks_clone, &task_id, LogLevel::Info, video_index, format!("工作线程 {} 开始处理视频{}", worker_id, video_index));

                    let output_filename = format!("{}_{}.mp4", config_clone.name, video_index);

                    // 为每个视频创建独立的临时子目录
                    let video_temp_dir = task_temp_dir.join(format!("video_{}", video_index));
                    if let Err(e) = fs::create_dir_all(&video_temp_dir) {
                        error!("创建视频 {} 临时目录失败: {}", video_index, e);
                        let mut failed = failed_videos.lock().unwrap();
                        failed.push(format!("视频{}: 创建临时目录失败 - {}", video_index, e));
                        let mut fail_count = failed_count.lock().unwrap();
                        *fail_count += 1;
                        push_step(&tasks_clone, &task_id, &video_step_id, &format!("视频{} - 失败", video_index), StepStatus::Error, Some(format!("创建临时目录失败: {}", e)));
                        continue;
                    }

                    // 获取used_per_folder的锁
                    let used_per_folder_guard = used_per_folder.lock().unwrap();
                    let mut local_used_per_folder = used_per_folder_guard.clone();
                    drop(used_per_folder_guard);

                    let result = process_single_mode(
                        &task_id,
                        &cancel_flag,
                        &tasks_clone,
                        video_index,
                        &config_clone.template_segments,
                        &config_clone.tutorial_folder,
                        &config_clone.video_ratio,
                        &config_clone.audio_path,
                        config_clone.audio_duration,
                        &config_clone.subtitle_path,
                        &output_dir,
                        &output_filename,
                        &mut local_used_per_folder,
                        &tutorial_used_by_config,
                        &config_clone.id,
                        &app_data_file,
                        &config_clone.root_folder,
                        config_clone.enable_transition,
                        config_clone.transition_duration,
                        &video_temp_dir,
                    );

                    // 合并used_per_folder的更新
                    let mut used_per_folder_guard = used_per_folder.lock().unwrap();
                    for (key, values) in local_used_per_folder {
                        let entry = used_per_folder_guard.entry(key).or_insert_with(HashSet::new);
                        entry.extend(values);
                    }
                    drop(used_per_folder_guard);

                    // 清理视频临时目录
                    let _ = fs::remove_dir_all(&video_temp_dir);

                    match result {
                        Ok(()) => {
                            info!("视频 {} 处理成功", video_index);
                            push_log(&tasks_clone, &task_id, LogLevel::Info, video_index, format!("视频{} 处理成功", video_index));
                            push_step(&tasks_clone, &task_id, &video_step_id, &format!("视频{} - 完成", video_index), StepStatus::Completed, None);
                            let mut count = completed_count.lock().unwrap();
                            *count += 1;
                            // 更新任务状态
                            let mut tasks = tasks_clone.write().unwrap();
                            for t in tasks.iter_mut() {
                                if t.id == task_id {
                                    t.completed_count = *count;
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            error!("视频 {} 处理失败: {}", video_index, e);
                            push_log(&tasks_clone, &task_id, LogLevel::Error, video_index, format!("视频{} 处理失败: {}", video_index, e));
                            if cancel_flag.load(Ordering::SeqCst) {
                                push_step(&tasks_clone, &task_id, &video_step_id, &format!("视频{} - 已取消", video_index), StepStatus::Error, Some(e.clone()));
                                break;
                            }
                            push_step(&tasks_clone, &task_id, &video_step_id, &format!("视频{} - 失败", video_index), StepStatus::Error, Some(e.clone()));
                            let mut failed = failed_videos.lock().unwrap();
                            failed.push(format!("视频{}: {}", video_index, e));
                            let mut fail_count = failed_count.lock().unwrap();
                            *fail_count += 1;
                            // 更新任务状态
                            let mut tasks = tasks_clone.write().unwrap();
                            for t in tasks.iter_mut() {
                                if t.id == task_id {
                                    t.failed_count = *fail_count;
                                    t.failed_videos = failed.clone();
                                    break;
                                }
                            }
                        }
                    }
                }
                
                *active_workers.lock().unwrap() -= 1;
                info!("工作线程 {} 退出", worker_id);
            });

            handles.push(handle);
        }

        // 等待所有工作线程完成
        for handle in handles {
            let _ = handle.join();
        }

        // 获取最终的计数
        let final_completed = *completed_count.lock().unwrap();
        let final_failed = *failed_count.lock().unwrap();
        let final_failed_videos = failed_videos.lock().unwrap().clone();

        // 终态决定
        push_step(&tasks_clone, &task_id, "finish", "任务完成", StepStatus::Completed, None);
        {
            let mut tasks = tasks_clone.write().unwrap();
            for t in tasks.iter_mut() {
                if t.id == task_id {
                    t.completed_at = Some(chrono::Utc::now());
                    t.completed_count = final_completed;
                    t.failed_count = final_failed;
                    t.failed_videos = final_failed_videos.clone();
                    if cancel_flag.load(Ordering::SeqCst) {
                        t.status = TaskStatus::Error;
                        if t.error_message.is_none() {
                            t.error_message = Some("任务已取消".to_string());
                        }
                    } else if final_failed == 0 {
                        t.status = TaskStatus::Completed;
                    } else if final_completed == 0 {
                        t.status = TaskStatus::Error;
                        let detail = final_failed_videos.first().cloned().unwrap_or_default();
                        t.error_message = Some(if detail.is_empty() {
                            format!("全部 {} 个视频处理失败", final_failed)
                        } else {
                            format!("全部 {} 个视频处理失败；首条失败: {}", final_failed, detail)
                        });
                    } else {
                        t.status = TaskStatus::Partial;
                        let detail = final_failed_videos.first().cloned().unwrap_or_default();
                        t.error_message = Some(if detail.is_empty() {
                            format!("成功 {} / 失败 {}", final_completed, final_failed)
                        } else {
                            format!("成功 {} / 失败 {}；首条失败: {}", final_completed, final_failed, detail)
                        });
                    }
                    break;
                }
            }
        }

        // 保存任务状态到磁盘
        if let Err(e) = save_tasks_to_disk(&app_data_file, &tasks_clone, &configs_clone, &tutorial_used_by_config) {
            error!("保存任务状态到磁盘失败: {}", e);
        }

        unregister_task(&task_id);

        // 清理任务级别的临时目录
        if let Err(e) = fs::remove_dir_all(&task_temp_dir) {
            warn!("清理任务临时目录失败: {}", e);
        } else {
            info!("任务临时目录已清理: {:?}", task_temp_dir);
        }

        info!("任务 {} 全部完成", task_id);
    });

    Ok(task)
}

#[tauri::command]
pub fn pause_task(state: tauri::State<AppState>, id: String) -> Result<(), String> {
    info!("暂停任务: id={}", id);
    set_pause(&id, true);
    let mut tasks = state.tasks.write().map_err(|e| e.to_string())?;
    if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
        task.status = TaskStatus::Paused;
        info!("任务 {} 已暂停", id);
        // 释放锁后再保存到磁盘
        drop(tasks);
        if let Err(e) = save_tasks_to_disk(
            &state.app_data_file,
            &state.tasks,
            &state.configs,
            &state.used_tutorial_videos,
        ) {
            error!("保存任务状态失败: {}", e);
        } else {
            info!("成功保存任务状态到磁盘");
        }
        Ok(())
    } else {
        Err(format!("任务 {} 不存在", id))
    }
}

#[tauri::command]
pub fn resume_task(state: tauri::State<AppState>, id: String) -> Result<(), String> {
    info!("恢复任务: id={}", id);
    set_pause(&id, false);
    let mut tasks = state.tasks.write().map_err(|e| e.to_string())?;
    if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
        task.status = TaskStatus::Running;
        info!("任务 {} 已恢复", id);
        // 释放锁后再保存到磁盘
        drop(tasks);
        if let Err(e) = save_tasks_to_disk(
            &state.app_data_file,
            &state.tasks,
            &state.configs,
            &state.used_tutorial_videos,
        ) {
            error!("保存任务状态失败: {}", e);
        } else {
            info!("成功保存任务状态到磁盘");
        }
        Ok(())
    } else {
        Err(format!("任务 {} 不存在", id))
    }
}

#[tauri::command]
pub fn retry_task(state: tauri::State<AppState>, id: String) -> Result<(), String> {
    info!("重试任务: id={}", id);
    set_pause(&id, false);
    let mut tasks = state.tasks.write().map_err(|e| e.to_string())?;
    if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
        // 重置信务状态，允许重新执行
        task.status = TaskStatus::Running;
        task.completed_count = 0;
        task.failed_count = 0;
        task.failed_videos = vec![];
        task.error_message = None;
        task.current_video = 0;
        task.started_at = Some(chrono::Utc::now());
        task.completed_at = None;
        // 清空进度步骤
        for step in task.progress_steps.iter_mut() {
            if step.status != StepStatus::Pending {
                step.status = StepStatus::Pending;
                step.error = None;
            }
        }
        info!("任务 {} 已重置并准备重试", id);
        // 释放锁后再保存到磁盘
        drop(tasks);
        if let Err(e) = save_tasks_to_disk(
            &state.app_data_file,
            &state.tasks,
            &state.configs,
            &state.used_tutorial_videos,
        ) {
            error!("保存任务状态失败: {}", e);
        } else {
            info!("成功保存任务状态到磁盘");
        }
        Ok(())
    } else {
        Err(format!("任务 {} 不存在", id))
    }
}

#[allow(non_snake_case)]
#[tauri::command]
pub fn delete_task(state: tauri::State<AppState>, id: String, deleteVideos: bool) -> Result<(), String> {
    info!("删除任务: id={}, deleteVideos={}", id, deleteVideos);
    // S2：删除即取消，立即 kill ffmpeg
    signal_cancel(&id);

    // 获取任务信息（包含output_folder）以便后续删除文件夹
    let task_to_delete: Option<Task> = {
        let tasks = state.tasks.read().map_err(|e| e.to_string())?;
        tasks.iter().find(|t| t.id == id).cloned()
    };

    let mut tasks = state.tasks.write().map_err(|e| e.to_string())?;
    tasks.retain(|t| t.id != id);
    info!("任务 {} 已删除", id);
    // 释放锁后再保存到磁盘
    drop(tasks);
    
    // 如果需要删除视频文件夹
    if deleteVideos {
        if let Some(task) = task_to_delete {
            let output_path = PathBuf::from(&task.output_folder);
            if output_path.exists() && output_path.is_dir() {
                match fs::remove_dir_all(&output_path) {
                    Ok(_) => info!("成功删除任务文件夹: {}", task.output_folder),
                    Err(e) => warn!("删除任务文件夹失败: {}", e),
                }
            }
        }
    }
    
    if let Err(e) = save_tasks_to_disk(
        &state.app_data_file,
        &state.tasks,
        &state.configs,
        &state.used_tutorial_videos,
    ) {
        error!("保存任务状态失败: {}", e);
    } else {
        info!("成功保存任务状态到磁盘");
    }
    Ok(())
}

#[tauri::command]
pub fn get_task(state: tauri::State<AppState>, id: String) -> Result<Task, String> {
    let tasks = state.tasks.read().map_err(|e| e.to_string())?;
    tasks
        .iter()
        .find(|t| t.id == id)
        .cloned()
        .ok_or_else(|| format!("任务 {} 不存在", id))
}

#[tauri::command]
pub fn get_tasks(state: tauri::State<AppState>) -> Result<Vec<Task>, String> {
    let tasks = state.tasks.read().map_err(|e| e.to_string())?;
    Ok(tasks.clone())
}

#[tauri::command]
pub fn refresh_tasks_from_disk(state: tauri::State<AppState>) -> Result<Vec<Task>, String> {
    let runtime = crate::storage::load_runtime_store()?;
    {
        let mut configs = state.configs.write().map_err(|e| e.to_string())?;
        *configs = runtime.app_data.configs.clone();
    }
    {
        let mut tasks = state.tasks.write().map_err(|e| e.to_string())?;
        *tasks = runtime.app_data.tasks.clone();
    }
    {
        let mut used = state.used_tutorial_videos.write().map_err(|e| e.to_string())?;
        *used = runtime.used_tutorial_by_config;
    }
    Ok(runtime.app_data.tasks)
}

#[tauri::command]
pub fn get_task_status(state: tauri::State<AppState>, id: String) -> Result<TaskStatus, String> {
    let tasks = state.tasks.read().map_err(|e| e.to_string())?;
    match tasks.iter().find(|t| t.id == id) {
        Some(task) => Ok(task.status.clone()),
        None => Err(format!("任务 {} 不存在", id)),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TutorialCheckResult {
    pub has_available: bool,
    pub total_count: usize,
    pub used_count: usize,
    pub available_count: usize,
}

#[tauri::command]
pub fn check_tutorial_available(
    state: tauri::State<AppState>,
    config_id: String,
    tutorial_folder: String,
) -> Result<TutorialCheckResult, String> {
    info!("检查教程文件夹可用性: config_id={}, folder={}", config_id, tutorial_folder);
    
    if tutorial_folder.trim().is_empty() {
        return Err("教程文件夹未配置".to_string());
    }
    
    let path = std::path::PathBuf::from(&tutorial_folder);
    if !path.exists() || !path.is_dir() {
        return Err(format!("教程文件夹不存在: {}", tutorial_folder));
    }
    
    let all_videos = get_video_files(&tutorial_folder)?;
    let total_count = all_videos.len();
    
    if total_count == 0 {
        return Ok(TutorialCheckResult {
            has_available: false,
            total_count: 0,
            used_count: 0,
            available_count: 0,
        });
    }
    
    let used_snapshot: HashSet<String> = state
        .used_tutorial_videos
        .read()
        .map_err(|e| e.to_string())?
        .get(&config_id)
        .cloned()
        .unwrap_or_default();
    
    // 清理掉已不存在的视频
    {
        if let Ok(mut guard) = state.used_tutorial_videos.write() {
            if let Some(used_set) = guard.get_mut(&config_id) {
                let mut to_remove = Vec::new();
                for used in used_set.iter() {
                    let path = PathBuf::from(used);
                    if !path.exists() {
                        to_remove.push(used.clone());
                    }
                }
                for remove in to_remove {
                    used_set.remove(&remove);
                }
            }
        }
    }
    
    // 重新获取清理后的 used 集合
    let used_cleaned: HashSet<String> = state
        .used_tutorial_videos
        .read()
        .map_err(|e| e.to_string())?
        .get(&config_id)
        .cloned()
        .unwrap_or_default();
    
    // 只统计同时存在于当前文件夹中的已使用视频（已删除的不计入）
    let mut actual_used_count = 0;
    let mut available_videos = Vec::new();
    for video_path in all_videos {
        let video_path_str = video_path.to_string_lossy().to_string();
        if used_cleaned.contains(&video_path_str) {
            actual_used_count += 1;
        } else {
            available_videos.push(video_path);
        }
    }
    let available_count = available_videos.len();
    
    let result = TutorialCheckResult {
        has_available: available_count > 0,
        total_count,
        used_count: actual_used_count,
        available_count,
    };
    
    info!("教程检查结果: 总数={}, 已用={}, 可用={}", 
        result.total_count, result.used_count, result.available_count);
    
    Ok(result)
}

#[tauri::command]
pub fn open_folder(path: String) -> Result<(), String> {
    info!("打开文件夹: {}", path);

    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("打开文件夹失败: {}", e))?;
    }

    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("打开文件夹失败: {}", e))?;
    }

    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| format!("打开文件夹失败: {}", e))?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_chained_xfade_filter_complex, calculate_tail_supplement_window,
        compute_segment_offsets, duration_cache, filter_videos_by_min_duration,
        get_random_transition_type, hidden_process_creation_flags,
        resolve_task_output_dir, sanitize_task_name, DurationCacheEntry,
    };
    use std::collections::HashSet;
    use std::path::PathBuf;
    use std::time::Instant;

    #[test]
    fn build_chained_xfade_filter_complex_should_use_processed_streams() {
        let filter = build_chained_xfade_filter_complex(
            2,
            &[5.0, 6.0],
            &[String::from("fade")],
            "0.5",
            0.5,
            1080,
            1920,
        );

        assert!(filter.contains("[0:v]settb=AVTB,setpts=PTS-STARTPTS,scale=1080:1920,setsar=sar=1,fps=30,format=yuv420p,setrange=tv[v0]"));
        assert!(filter.contains("[1:v]settb=AVTB,setpts=PTS-STARTPTS,scale=1080:1920,setsar=sar=1,fps=30,format=yuv420p,setrange=tv[v1]"));
        assert!(filter.contains("[v0][v1]xfade=transition=fade:duration=0.5:offset=4.5[v1_out]"));
        assert!(filter.contains("[v1_out]format=yuv420p[final_video]"));
        assert!(!filter.contains("[v0_out][1:v]xfade"));
    }

    #[test]
    fn build_chained_xfade_filter_complex_should_chain_previous_output() {
        let filter = build_chained_xfade_filter_complex(
            3,
            &[4.0, 5.0, 6.0],
            &[String::from("fade"), String::from("wipeleft")],
            "0.5",
            0.5,
            1920,
            1080,
        );

        assert!(filter.contains("[v0][v1]xfade=transition=fade:duration=0.5:offset=3.5[v1_out]"));
        assert!(filter.contains("[v1_out][v2]xfade=transition=wipeleft:duration=0.5:offset=8[v2_out]"));
        assert!(filter.contains("[v2_out]format=yuv420p[final_video]"));
    }

    #[test]
    fn compute_segment_offsets_should_accumulate_previous_durations() {
        let offsets = compute_segment_offsets(&[10.0, 20.0, 30.0]);
        assert_eq!(offsets, vec![0.0, 10.0, 30.0]);
    }

    #[test]
    fn compute_segment_offsets_should_return_empty_when_no_segments() {
        let offsets = compute_segment_offsets(&[]);
        assert!(offsets.is_empty());
    }

    #[test]
    fn resolve_task_output_dir_should_prefer_output_folder() {
        let dir = resolve_task_output_dir(
            "/tmp/out",
            "/tmp/root",
            "my-task-1700000000",
        ).unwrap();
        assert_eq!(dir, PathBuf::from("/tmp/out/my-task-1700000000"));
    }

    #[test]
    fn resolve_task_output_dir_should_fallback_to_root_folder_output() {
        let dir = resolve_task_output_dir(
            "",
            "/tmp/root",
            "my-task-1700000000",
        ).unwrap();
        assert_eq!(dir, PathBuf::from("/tmp/root/output/my-task-1700000000"));
    }

    #[test]
    fn resolve_task_output_dir_should_error_when_both_empty() {
        let result = resolve_task_output_dir("", "   ", "my-task");
        assert!(result.is_err());
    }

    #[test]
    fn resolve_task_output_dir_should_trim_inputs() {
        let dir = resolve_task_output_dir(
            "  /tmp/out  ",
            "  /tmp/root  ",
            "  my-task  ",
        ).unwrap();
        assert_eq!(dir, PathBuf::from("/tmp/out/my-task"));
    }

    #[test]
    fn sanitize_task_name_should_replace_illegal_chars() {
        assert_eq!(sanitize_task_name("a/b\\c:d*e?f\"g<h>i|j"), "a_b_c_d_e_f_g_h_i_j");
        assert_eq!(sanitize_task_name("  hello  "), "hello");
        assert_eq!(sanitize_task_name(""), "task");
        assert_eq!(sanitize_task_name("\t\n"), "task");
    }

    /// 转场池扩大后，wipe 系占比应被显著稀释（≤ 1/5），且至少覆盖 5 类风格。
    #[test]
    fn get_random_transition_type_should_have_diversified_pool() {
        let mut seen: HashSet<String> = HashSet::new();
        for _ in 0..2000 {
            seen.insert(get_random_transition_type());
        }

        // 至少应见过若干种非 wipe 风格转场
        let non_wipe_samples = ["fade", "dissolve", "slideleft", "smoothleft", "circleopen", "zoomin", "pixelize"];
        let hit = non_wipe_samples
            .iter()
            .filter(|s| seen.contains(**s))
            .count();
        assert!(hit >= 5, "非 wipe 风格覆盖不足: {:?}", seen);

        // wipe 系命中数不能超过总池子的一半
        let wipe = ["wipeleft", "wiperight", "wipeup", "wipedown"];
        let wipe_hit = wipe.iter().filter(|s| seen.contains(**s)).count();
        assert!(wipe_hit <= 4);
        assert!(seen.len() >= 10, "转场池规模不足: {}", seen.len());
    }

    /// 通过预热进程级时长缓存，验证 filter_videos_by_min_duration 能剔除时长不足的素材。
    /// 这里不真正调用 ffprobe，直接往 duration_cache 写入伪造时长。
    #[test]
    fn filter_videos_by_min_duration_should_drop_short_videos() {
        let long_path = PathBuf::from("/tmp/__test_long_video_for_filter.mp4");
        let short_path = PathBuf::from("/tmp/__test_short_video_for_filter.mp4");

        {
            let mut guard = duration_cache().write().unwrap();
            guard.insert(
                long_path.to_string_lossy().to_string(),
                DurationCacheEntry {
                    duration: 60.0,
                    file_mtime: None,
                    file_size: None,
                    cached_at: Instant::now(),
                },
            );
            guard.insert(
                short_path.to_string_lossy().to_string(),
                DurationCacheEntry {
                    duration: 5.0,
                    file_mtime: None,
                    file_size: None,
                    cached_at: Instant::now(),
                },
            );
        }

        let videos = vec![long_path.clone(), short_path.clone()];
        let kept = filter_videos_by_min_duration(&videos, 30.0);

        assert_eq!(kept, vec![long_path.clone()]);

        // 边界：恰好等于 min_duration（含 0.05 容差）也应保留
        let kept_edge = filter_videos_by_min_duration(&videos, 5.0);
        assert!(kept_edge.contains(&short_path));
        assert!(kept_edge.contains(&long_path));

        // 清理缓存避免影响其他测试
        let mut guard = duration_cache().write().unwrap();
        guard.remove(&long_path.to_string_lossy().to_string());
        guard.remove(&short_path.to_string_lossy().to_string());
    }

    #[test]
    fn calculate_tail_supplement_window_should_use_video_tail_when_duration_is_enough() {
        let (start_offset, use_duration) = calculate_tail_supplement_window(10.0, 2.0);

        assert_eq!(start_offset, 8.0);
        assert_eq!(use_duration, 2.0);
    }

    #[test]
    fn calculate_tail_supplement_window_should_fallback_to_full_video_when_duration_is_short() {
        let (start_offset, use_duration) = calculate_tail_supplement_window(1.5, 2.0);

        assert_eq!(start_offset, 0.0);
        assert_eq!(use_duration, 1.5);
    }

    #[test]
    #[cfg(target_os = "windows")]
    fn hidden_process_creation_flags_should_use_create_no_window_on_windows() {
        assert_eq!(hidden_process_creation_flags(), WINDOWS_CREATE_NO_WINDOW);
    }

    #[test]
    #[cfg(not(target_os = "windows"))]
    fn hidden_process_creation_flags_should_be_zero_on_non_windows() {
        assert_eq!(hidden_process_creation_flags(), 0);
    }
}
