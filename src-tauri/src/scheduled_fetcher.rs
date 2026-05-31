/*!
 定时 TikTok 元数据采集（CronFetcher）核心模型与算法。

 包含数据结构（ScheduledFetcher / ScheduledRun / RunTrigger / RunStatus）、
 yt-dlp 输出解析、单次执行内去重、时间窗口过滤与命中截断、
 CSV 文件命名与写入等纯逻辑函数。
 调度器、Tauri 命令与外部 IO 不在本文件实现。
*/

use chrono::{DateTime, Local, Utc};
use csv::{QuoteStyle, Terminator, WriterBuilder};
use serde::{Deserialize, Serialize};
use std::cmp::Reverse;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;

use crate::video_processor::apply_hidden_process_startup;

/// yt-dlp `tiktok:app_info` 中默认的 IID 值（2026-05 已验证可用）。
pub const DEFAULT_TIKTOK_IID: &str = "7501732030001269264";

/// 默认每次拉取的元数据条数上限。
pub fn default_max_fetch_num() -> u32 {
    1000
}

/// 默认每次写入 CSV 的命中条数上限。
pub fn default_num_meet_condition() -> u32 {
    100
}

/// 默认 IID。
pub fn default_tiktok_iid() -> String {
    DEFAULT_TIKTOK_IID.to_string()
}

/// 默认 enabled = true。
pub fn default_true() -> bool {
    true
}

/// 单个定时元数据采集任务的持久化配置。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledFetcher {
    pub id: String,
    pub name: String,
    pub target_url: String,
    pub window_days: u32,
    pub window_hours: u32,
    pub interval_days: u32,
    pub interval_hours: u32,
    pub output_dir: String,
    #[serde(default = "default_max_fetch_num")]
    pub max_fetch_num: u32,
    #[serde(default = "default_num_meet_condition")]
    pub num_meet_condition: u32,
    #[serde(default = "default_tiktok_iid")]
    pub tiktok_iid: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub consecutive_failures: u32,
    #[serde(default)]
    pub cooldown_until: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 单次执行的触发来源。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunTrigger {
    Manual,
    Scheduled,
}

/// 单次执行的状态机。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RunStatus {
    Pending,
    FetchingList,
    Filtering,
    Writing,
    Success,
    Failed,
}

/// 单次执行的运行记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduledRun {
    pub id: String,
    pub fetcher_id: String,
    pub fetcher_name: String,
    pub config_id: String,
    pub config_name: String,
    pub run_index: u32,
    pub trigger: RunTrigger,
    pub status: RunStatus,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub fetched_total: u32,
    #[serde(default)]
    pub matched_in_window: u32,
    #[serde(default)]
    pub new_appended: u32,
    #[serde(default)]
    pub csv_path: Option<String>,
    #[serde(default)]
    pub error_message: Option<String>,
    #[serde(default)]
    pub progress_message: String,
}

/// yt-dlp `--print` 一行解析后的视频元数据条目。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TikTokEntry {
    pub id: String,
    pub timestamp: i64,
    pub uploader: String,
    pub like_count: i64,
    pub comment_count: i64,
    pub repost_count: i64,
    pub save_count: i64,
    pub view_count: i64,
}

/// yt-dlp 调用错误分类。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum YtDlpErrorKind {
    /// IID 失效（stderr 命中 `No working app info`）。
    IidInvalid,
    /// 二进制不可用 / 路径找不到。
    BinaryUnavailable,
    /// 其他错误（含网络、退出码非 0 等）。
    Other,
}

/// yt-dlp 调用错误。
#[derive(Debug, Clone)]
pub struct YtDlpError {
    pub kind: YtDlpErrorKind,
    pub message: String,
}

impl YtDlpError {
    pub fn new(kind: YtDlpErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

/**
 校验目标 URL 是否为 TikTok 域名。

 参数:
 - `url`: 待校验的目标链接。

 返回:
 - `Ok(())`: URL 以允许的 TikTok 前缀开头。
 - `Err(String)`: URL 不符合规则。

 异常:
 - 不抛出异常；只通过 `Result::Err` 返回信息。
*/
pub fn validate_target_url(url: &str) -> Result<(), String> {
    let trimmed = url.trim();
    if trimmed.is_empty() {
        return Err("目标链接不能为空".to_string());
    }
    let allow_prefixes = [
        "https://www.tiktok.com",
        "https://tiktok.com",
        "https://m.tiktok.com",
    ];
    if allow_prefixes.iter().any(|p| trimmed.starts_with(p)) {
        Ok(())
    } else {
        Err(format!("仅支持 TikTok 链接（前缀须为 {:?}）", allow_prefixes))
    }
}

/**
 校验 IID 字面格式：长度 10-30 的纯数字串。

 参数:
 - `iid`: 待校验的 IID 字符串。

 返回:
 - `Ok(())`: IID 合法。
 - `Err(String)`: IID 非法。

 异常:
 - 不抛出异常。
*/
pub fn validate_iid_format(iid: &str) -> Result<(), String> {
    let trimmed = iid.trim();
    if trimmed.len() < 10 || trimmed.len() > 30 {
        return Err("IID 长度必须在 10-30 之间".to_string());
    }
    if !trimmed.chars().all(|c| c.is_ascii_digit()) {
        return Err("IID 必须全部为数字".to_string());
    }
    Ok(())
}

/**
 解析 yt-dlp `--print` 的多行制表符输出。

 每行严格 8 列，按顺序：
 `id\ttimestamp\tuploader\tlike\tcomment\trepost\tsave\tview`。
 缺失字段（NA / 空 / 非数字）按 0 处理。
 解析失败的行（列数不足 8）会被静默跳过。

 参数:
 - `stdout`: yt-dlp 进程标准输出全文。

 返回:
 - `Vec<TikTokEntry>`: 已解析的条目列表。

 异常:
 - 不抛出异常；非法行被忽略。
*/
pub fn parse_entries(stdout: &str) -> Vec<TikTokEntry> {
    let mut result = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim_end_matches('\r');
        if trimmed.is_empty() {
            continue;
        }
        let cols: Vec<&str> = trimmed.split('\t').collect();
        if cols.len() < 8 {
            continue;
        }
        let id = cols[0].trim().to_string();
        if id.is_empty() || id == "NA" {
            continue;
        }
        let timestamp = parse_int_or_zero(cols[1]);
        let uploader = cols[2].trim().to_string();
        let like_count = parse_int_or_zero(cols[3]);
        let comment_count = parse_int_or_zero(cols[4]);
        let repost_count = parse_int_or_zero(cols[5]);
        let save_count = parse_int_or_zero(cols[6]);
        let view_count = parse_int_or_zero(cols[7]);

        result.push(TikTokEntry {
            id,
            timestamp,
            uploader,
            like_count,
            comment_count,
            repost_count,
            save_count,
            view_count,
        });
    }
    result
}

/**
 将字符串解析为 i64，失败或 NA 时返回 0。

 参数:
 - `value`: 待解析字符串。

 返回:
 - `i64`: 解析结果或 0。
*/
fn parse_int_or_zero(value: &str) -> i64 {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("NA") {
        return 0;
    }
    trimmed.parse::<i64>().unwrap_or(0)
}

/**
 在单次执行内按 id 去重，保留首次出现的条目顺序。
 用于防止 yt-dlp 在边界条件下重复返回同一视频。

 参数:
 - `entries`: 原始条目列表。

 返回:
 - `Vec<TikTokEntry>`: 去重后的条目列表。
*/
pub fn dedup_within_run(entries: Vec<TikTokEntry>) -> Vec<TikTokEntry> {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut result = Vec::with_capacity(entries.len());
    for entry in entries.into_iter() {
        if seen.insert(entry.id.clone()) {
            result.push(entry);
        }
    }
    result
}

/**
 按时间窗口过滤 + 命中数截断。

 - 先按 `timestamp` 倒序排序
 - 取所有 `timestamp >= now_ts - window_secs` 的条目
 - 命中数超过 `num_meet_condition` 时截断为前 N 条

 参数:
 - `entries`: 已去重的条目列表（无需预排序）。
 - `window_secs`: 时间窗口长度（秒），≥ 0。
 - `now_ts`: 任务执行起始时间的 Unix 时间戳。
 - `num_meet_condition`: 命中数上限，≥ 1。

 返回:
 - `Vec<TikTokEntry>`: 满足条件的条目，按 timestamp 倒序。
*/
pub fn filter_in_window_and_truncate(
    mut entries: Vec<TikTokEntry>,
    window_secs: i64,
    now_ts: i64,
    num_meet_condition: u32,
) -> Vec<TikTokEntry> {
    entries.sort_by_key(|e| Reverse(e.timestamp));
    let cutoff = now_ts.saturating_sub(window_secs);
    let mut matched: Vec<TikTokEntry> = entries
        .into_iter()
        .take_while(|e| e.timestamp >= cutoff)
        .collect();
    let n = num_meet_condition as usize;
    if matched.len() > n {
        matched.truncate(n);
    }
    matched
}

/**
 拼装 CSV 文件名（不含路径）。

 格式：`<run_index>-<YYYYMMDD_HHMMSS>.csv`。

 参数:
 - `run_index`: 本进程内该 fetcher 的执行轮次（从 1 开始）。
 - `started_at`: 任务执行起始时间（本地时区）。

 返回:
 - `String`: 文件名字符串。
*/
pub fn build_csv_filename(run_index: u32, started_at: DateTime<Local>) -> String {
    format!(
        "{}-{}.csv",
        run_index,
        started_at.format("%Y%m%d_%H%M%S")
    )
}

/**
 在文件已存在时追加 `_<uuid前8位>` 后缀，避免极端同秒重启冲突。

 参数:
 - `output_dir`: 目标目录。
 - `filename`: 候选文件名（含 `.csv` 后缀）。

 返回:
 - `PathBuf`: 最终可用的文件绝对路径（不存在）。
*/
pub fn resolve_csv_path_with_suffix(output_dir: &Path, filename: &str) -> PathBuf {
    let candidate = output_dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }
    let stem = filename.strip_suffix(".csv").unwrap_or(filename);
    let short = uuid::Uuid::new_v4().to_string();
    let short8 = &short[..8];
    output_dir.join(format!("{}_{}.csv", stem, short8))
}

/**
 写入新建的 CSV 文件：BOM + 表头 + 数据行。

 - 文件已存在时不会被覆盖（调用方应先用 `resolve_csv_path_with_suffix` 兜底）
 - UTF-8 with BOM、CRLF 行尾、按需引号
 - `fetched_at` 列写入参数 `fetched_at` 的本地时区格式化字符串

 参数:
 - `output_dir`: 输出目录（不存在时自动创建）。
 - `csv_filename`: 由 `build_csv_filename` 拼装的文件名。
 - `entries`: 待写入条目（已是最终顺序）。
 - `fetched_at`: 本次执行起始时间（本地时区）。

 返回:
 - `Ok(PathBuf)`: 实际写入的 CSV 路径。
 - `Err(String)`: 创建目录、打开文件或写入失败。

 异常:
 - 文件系统错误时返回 `Err`。
*/
pub fn write_csv_new_file(
    output_dir: &Path,
    csv_filename: &str,
    entries: &[TikTokEntry],
    fetched_at: DateTime<Local>,
) -> Result<PathBuf, String> {
    fs::create_dir_all(output_dir).map_err(|e| format!("创建输出目录失败: {}", e))?;
    let path = resolve_csv_path_with_suffix(output_dir, csv_filename);

    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .map_err(|e| format!("创建 CSV 文件失败: {}", e))?;
    // UTF-8 BOM
    file.write_all(&[0xEF, 0xBB, 0xBF])
        .map_err(|e| format!("写入 BOM 失败: {}", e))?;

    let mut writer = WriterBuilder::new()
        .terminator(Terminator::CRLF)
        .quote_style(QuoteStyle::Necessary)
        .from_writer(file);

    writer
        .write_record([
            "video_id",
            "publish_time",
            "like_count",
            "comment_count",
            "repost_count",
            "save_count",
            "view_count",
            "uploader",
            "video_url",
            "fetched_at",
        ])
        .map_err(|e| format!("写入 CSV 表头失败: {}", e))?;

    let fetched_at_str = fetched_at.format("%Y-%m-%d %H:%M:%S").to_string();

    for entry in entries {
        let publish_time = format_local_time(entry.timestamp);
        let video_url = format!(
            "https://www.tiktok.com/@{}/video/{}",
            entry.uploader, entry.id
        );
        writer
            .write_record([
                entry.id.as_str(),
                publish_time.as_str(),
                entry.like_count.to_string().as_str(),
                entry.comment_count.to_string().as_str(),
                entry.repost_count.to_string().as_str(),
                entry.save_count.to_string().as_str(),
                entry.view_count.to_string().as_str(),
                entry.uploader.as_str(),
                video_url.as_str(),
                fetched_at_str.as_str(),
            ])
            .map_err(|e| format!("写入 CSV 数据行失败: {}", e))?;
    }
    writer.flush().map_err(|e| format!("CSV flush 失败: {}", e))?;
    Ok(path)
}

/**
 将 Unix 时间戳格式化为 "YYYY-MM-DD HH:MM:SS"（本地时区）。
 timestamp <= 0 时返回空字符串。

 参数:
 - `timestamp`: Unix 时间戳（秒）。

 返回:
 - `String`: 格式化字符串或空字符串。
*/
pub fn format_local_time(timestamp: i64) -> String {
    if timestamp <= 0 {
        return String::new();
    }
    match chrono::DateTime::<Utc>::from_timestamp(timestamp, 0) {
        Some(utc_dt) => {
            let local_dt: DateTime<Local> = DateTime::from(utc_dt);
            local_dt.format("%Y-%m-%d %H:%M:%S").to_string()
        }
        None => String::new(),
    }
}

/**
 yt-dlp `--print` 字段拼装：固定 8 列、TAB 分隔。
*/
pub const YT_DLP_PRINT_FORMAT: &str =
    "%(id)s\t%(timestamp)s\t%(uploader)s\t%(like_count)s\t%(comment_count)s\t%(repost_count)s\t%(save_count)s\t%(view_count)s";

/**
 拼装 yt-dlp 调用所需的参数列表（不含可执行文件本体）。

 参数:
 - `target_url`: 目标列表/合集链接。
 - `iid`: 注入的 tiktok app_info IID。
 - `max_fetch_num`: 拉取条数上限（≥ 1）。

 返回:
 - `Vec<String>`: 完整 args，按顺序 push 给子进程。
*/
pub fn build_yt_dlp_args(target_url: &str, iid: &str, max_fetch_num: u32) -> Vec<String> {
    let extractor_args = format!(
        "tiktok:app_info={}/musical_ly/35.1.3/2023501030/1233",
        iid
    );
    vec![
        "--extractor-args".to_string(),
        extractor_args,
        "--flat-playlist".to_string(),
        "--playlist-end".to_string(),
        max_fetch_num.max(1).to_string(),
        "--no-warnings".to_string(),
        "--print".to_string(),
        YT_DLP_PRINT_FORMAT.to_string(),
        target_url.to_string(),
    ]
}

/**
 在打包/开发环境下查找 yt-dlp 可执行文件。

 优先：当前可执行文件同目录的 sidecar（`yt-dlp` / `yt-dlp.exe` / 三元组命名）；
 回退：`src-tauri/binaries/yt-dlp*`（开发模式从仓库根运行）；
 最终回退：系统 PATH 中的 `yt-dlp`。

 返回:
 - `Some(PathBuf)`: 找到的可执行路径。
 - `None`: 全部回退后仍找不到时由调用方决定。
*/
pub fn find_yt_dlp_executable() -> Option<PathBuf> {
    // 1) sidecar：与主程序同目录
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(exe_dir) = current_exe.parent() {
            #[cfg(target_os = "windows")]
            let names: &[&str] = &["yt-dlp.exe", "yt-dlp-x86_64-pc-windows-msvc.exe"];
            #[cfg(target_os = "macos")]
            let names: &[&str] = &[
                "yt-dlp",
                "yt-dlp-aarch64-apple-darwin",
                "yt-dlp-x86_64-apple-darwin",
                "yt-dlp_macos",
            ];
            #[cfg(target_os = "linux")]
            let names: &[&str] = &["yt-dlp", "yt-dlp-x86_64-unknown-linux-gnu"];

            for name in names {
                let candidate = exe_dir.join(name);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
    }
    // 2) 开发模式：从工程根目录的 src-tauri/binaries 查找
    let cwd = std::env::current_dir().ok()?;
    let dev_candidates = [
        cwd.join("src-tauri").join("binaries").join("yt-dlp"),
        cwd.join("src-tauri").join("binaries").join("yt-dlp_macos"),
        cwd.join("binaries").join("yt-dlp"),
    ];
    for candidate in dev_candidates {
        if candidate.exists() {
            return Some(candidate);
        }
    }
    // 3) 最终：让系统 PATH 接管，由调用方使用 "yt-dlp" 字面量
    None
}

/**
 启动期 yt-dlp 健康检查（仅探测可执行性，不联网）。

 - 调用 `yt-dlp --version`，2 秒内成功输出即视为健康。
 - 找不到二进制 / 进程执行失败 / 退出码非 0 → false。

 返回:
 - `bool`: 是否就绪。
*/
pub fn check_yt_dlp_health() -> bool {
    let exe = find_yt_dlp_executable()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "yt-dlp".to_string());

    let mut command = std::process::Command::new(&exe);
    apply_hidden_process_startup(&mut command);
    command.arg("--version").stdout(Stdio::piped()).stderr(Stdio::piped());

    match command.output() {
        Ok(output) => output.status.success(),
        Err(_) => false,
    }
}

/**
 同步执行 yt-dlp 子进程并返回 stdout/stderr。

 参数:
 - `target_url`: 列表/合集链接。
 - `iid`: 任务级 tiktok IID。
 - `max_fetch_num`: 拉取条数上限。

 返回:
 - `Ok((stdout, stderr))`: 退出码 0 时返回输出。
 - `Err(YtDlpError)`: 二进制不可用 / IID 失效 / 其他失败。

 异常:
 - 不抛 panic；所有错误经 `YtDlpError` 透出。
*/
pub fn exec_yt_dlp(
    target_url: &str,
    iid: &str,
    max_fetch_num: u32,
) -> Result<(String, String), YtDlpError> {
    let exe = find_yt_dlp_executable()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| "yt-dlp".to_string());

    let args = build_yt_dlp_args(target_url, iid, max_fetch_num);

    let mut command = std::process::Command::new(&exe);
    apply_hidden_process_startup(&mut command);
    command
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let output = command.output().map_err(|e| {
        YtDlpError::new(
            YtDlpErrorKind::BinaryUnavailable,
            format!("yt-dlp 启动失败: {} ({})", e, exe),
        )
    })?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();

    if output.status.success() {
        return Ok((stdout, stderr));
    }
    if stderr.contains("No working app info") || stderr.contains("app_info") {
        return Err(YtDlpError::new(
            YtDlpErrorKind::IidInvalid,
            format!("IID 失效或被拒绝：{}", stderr.lines().last().unwrap_or("")),
        ));
    }
    Err(YtDlpError::new(
        YtDlpErrorKind::Other,
        format!("yt-dlp 退出码非 0：{}", stderr.lines().last().unwrap_or("")),
    ))
}

/**
 单次执行的产出：最终 ScheduledRun + 是否触发了 IID 失效信号。
*/
#[derive(Debug, Clone)]
pub struct RunOutcome {
    pub run: ScheduledRun,
    pub iid_invalid: bool,
}

/**
 单次执行：拉取 → 解析 → 过滤 → CSV 写入。

 这是一个纯函数 + 进度回调的实现，不直接依赖 Tauri 类型；
 调度器在 spawn 内传入闭包将每个状态变化转发为 `scheduled-run-update` 事件。

 参数:
 - `fetcher`: 当前任务配置快照（IID / window / max_fetch_num / num_meet_condition / output_dir 等）。
 - `run_index`: 该 fetcher 在当前进程内的执行轮次（从 1 开始）。
 - `trigger`: 触发来源（manual / scheduled）。
 - `config_id` / `config_name`: 所属配置上下文。
 - `now_ts`: 任务执行起始时间的 Unix 秒（用于窗口过滤）。
 - `started_at_local`: 本地时区时间，用于 CSV 文件名与 fetched_at。
 - `emit_update`: 进度回调，每次状态变化均会被调用。

 返回:
 - `RunOutcome`: 含最终 run + iid_invalid 标志。

 异常:
 - 不抛 panic；所有异常通过 run.status=Failed + run.error_message 返回。
*/
pub fn run_once<F>(
    fetcher: &ScheduledFetcher,
    run_index: u32,
    trigger: RunTrigger,
    config_id: &str,
    config_name: &str,
    now_ts: i64,
    started_at_local: DateTime<Local>,
    mut emit_update: F,
) -> RunOutcome
where
    F: FnMut(&ScheduledRun),
{
    let started_at = Utc::now();
    let mut run = ScheduledRun {
        id: uuid::Uuid::new_v4().to_string(),
        fetcher_id: fetcher.id.clone(),
        fetcher_name: fetcher.name.clone(),
        config_id: config_id.to_string(),
        config_name: config_name.to_string(),
        run_index,
        trigger,
        status: RunStatus::FetchingList,
        started_at,
        finished_at: None,
        fetched_total: 0,
        matched_in_window: 0,
        new_appended: 0,
        csv_path: None,
        error_message: None,
        progress_message: format!("正在调用 yt-dlp 拉取 {} 条元数据…", fetcher.max_fetch_num),
    };
    emit_update(&run);

    // Stage 1: yt-dlp
    let (stdout, _stderr) = match exec_yt_dlp(
        &fetcher.target_url,
        &fetcher.tiktok_iid,
        fetcher.max_fetch_num,
    ) {
        Ok(pair) => pair,
        Err(err) => {
            let iid_invalid = err.kind == YtDlpErrorKind::IidInvalid;
            run.status = RunStatus::Failed;
            run.finished_at = Some(Utc::now());
            run.error_message = Some(err.message);
            run.progress_message = if iid_invalid {
                "IID 失效，已自动暂停（请在编辑任务的“高级”区块替换）".to_string()
            } else {
                "yt-dlp 调用失败".to_string()
            };
            emit_update(&run);
            return RunOutcome { run, iid_invalid };
        }
    };

    // Stage 2: parse + dedup + filter
    run.status = RunStatus::Filtering;
    run.progress_message = "正在按时间窗口过滤命中条目…".to_string();
    emit_update(&run);

    let raw_entries = parse_entries(&stdout);
    run.fetched_total = raw_entries.len() as u32;
    let deduped = dedup_within_run(raw_entries);
    let window_secs = (fetcher.window_days as i64) * 86_400 + (fetcher.window_hours as i64) * 3_600;
    let matched = filter_in_window_and_truncate(deduped, window_secs, now_ts, fetcher.num_meet_condition);
    run.matched_in_window = matched.len() as u32;

    // Stage 3: CSV
    run.status = RunStatus::Writing;
    run.progress_message = format!("正在写入 CSV（命中 {} 条）", matched.len());
    emit_update(&run);

    let filename = build_csv_filename(run_index, started_at_local);
    let output_dir = std::path::Path::new(&fetcher.output_dir);
    match write_csv_new_file(output_dir, &filename, &matched, started_at_local) {
        Ok(path) => {
            run.csv_path = Some(path.to_string_lossy().to_string());
            run.new_appended = matched.len() as u32;
            run.status = RunStatus::Success;
            run.finished_at = Some(Utc::now());
            run.progress_message = format!(
                "完成：拉取 {} 条 / 命中窗口 {} 条 / 已写入独立 CSV",
                run.fetched_total, run.matched_in_window
            );
            emit_update(&run);
            RunOutcome { run, iid_invalid: false }
        }
        Err(err_msg) => {
            run.status = RunStatus::Failed;
            run.finished_at = Some(Utc::now());
            run.error_message = Some(err_msg);
            run.progress_message = "CSV 写入失败".to_string();
            emit_update(&run);
            RunOutcome { run, iid_invalid: false }
        }
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(tag: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("scheduled_fetcher_test_{tag}_{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn validate_target_url_should_accept_tiktok_prefixes() {
        assert!(validate_target_url("https://www.tiktok.com/music/abc-123").is_ok());
        assert!(validate_target_url("https://tiktok.com/@user").is_ok());
        assert!(validate_target_url("https://m.tiktok.com/foo").is_ok());
    }

    #[test]
    fn validate_target_url_should_reject_non_tiktok() {
        assert!(validate_target_url("").is_err());
        assert!(validate_target_url("https://www.youtube.com/abc").is_err());
        assert!(validate_target_url("http://www.tiktok.com/abc").is_err());
    }

    #[test]
    fn validate_iid_format_should_accept_digits_in_range() {
        assert!(validate_iid_format("7501732030001269264").is_ok());
        assert!(validate_iid_format("1234567890").is_ok());
    }

    #[test]
    fn validate_iid_format_should_reject_invalid() {
        assert!(validate_iid_format("123").is_err());
        assert!(validate_iid_format("abc1234567890").is_err());
        assert!(validate_iid_format(&"1".repeat(31)).is_err());
    }

    #[test]
    fn parse_entries_should_handle_normal_lines() {
        let stdout = "abc123\t1717000000\tuser_a\t100\t20\t5\t8\t9999\n\
                      def456\t1717000100\tuser_b\t200\t30\t6\t9\t10000\n";
        let entries = parse_entries(stdout);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].id, "abc123");
        assert_eq!(entries[0].timestamp, 1717000000);
        assert_eq!(entries[0].like_count, 100);
        assert_eq!(entries[1].uploader, "user_b");
    }

    #[test]
    fn parse_entries_should_skip_short_or_empty_lines() {
        let stdout = "\n   \nbadrow\twithonly2\nabc123\t1717000000\tuser_a\t1\t2\t3\t4\t5\n";
        let entries = parse_entries(stdout);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].id, "abc123");
    }

    #[test]
    fn parse_entries_should_treat_na_and_invalid_as_zero() {
        let stdout = "abc\tNA\tuser_a\tNA\t\t-\tabc\t100\n";
        let entries = parse_entries(stdout);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].timestamp, 0);
        assert_eq!(entries[0].like_count, 0);
        assert_eq!(entries[0].comment_count, 0);
        assert_eq!(entries[0].repost_count, 0);
        assert_eq!(entries[0].save_count, 0);
        assert_eq!(entries[0].view_count, 100);
    }

    #[test]
    fn parse_entries_should_skip_id_na_or_empty() {
        let stdout = "\t1\tu\t1\t1\t1\t1\t1\nNA\t1\tu\t1\t1\t1\t1\t1\n";
        let entries = parse_entries(stdout);
        assert!(entries.is_empty());
    }

    #[test]
    fn dedup_within_run_should_keep_first_occurrence() {
        let entries = vec![
            TikTokEntry {
                id: "a".into(),
                timestamp: 100,
                uploader: "u".into(),
                like_count: 1,
                comment_count: 0,
                repost_count: 0,
                save_count: 0,
                view_count: 0,
            },
            TikTokEntry {
                id: "a".into(),
                timestamp: 200,
                uploader: "u".into(),
                like_count: 99,
                comment_count: 0,
                repost_count: 0,
                save_count: 0,
                view_count: 0,
            },
            TikTokEntry {
                id: "b".into(),
                timestamp: 150,
                uploader: "u".into(),
                like_count: 5,
                comment_count: 0,
                repost_count: 0,
                save_count: 0,
                view_count: 0,
            },
        ];
        let result = dedup_within_run(entries);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].id, "a");
        assert_eq!(result[0].like_count, 1);
        assert_eq!(result[1].id, "b");
    }

    #[test]
    fn filter_in_window_and_truncate_should_apply_window_and_cap() {
        let now = 1_000_000_i64;
        let window_secs = 3600_i64;
        let entries = vec![
            // 在窗口内
            entry_with(now - 100, "a"),
            entry_with(now - 200, "b"),
            entry_with(now - 3000, "c"),
            // 超出窗口
            entry_with(now - 4000, "d"),
            entry_with(now - 5000, "e"),
        ];
        let result = filter_in_window_and_truncate(entries.clone(), window_secs, now, 100);
        assert_eq!(result.len(), 3);
        // 倒序：最新在前
        assert_eq!(result[0].id, "a");
        assert_eq!(result[2].id, "c");

        // 命中截断
        let capped = filter_in_window_and_truncate(entries, window_secs, now, 2);
        assert_eq!(capped.len(), 2);
        assert_eq!(capped[0].id, "a");
        assert_eq!(capped[1].id, "b");
    }

    fn entry_with(ts: i64, id: &str) -> TikTokEntry {
        TikTokEntry {
            id: id.into(),
            timestamp: ts,
            uploader: "u".into(),
            like_count: 1,
            comment_count: 1,
            repost_count: 1,
            save_count: 1,
            view_count: 1,
        }
    }

    #[test]
    fn build_csv_filename_should_match_pattern() {
        let now = chrono::TimeZone::with_ymd_and_hms(&Local, 2026, 5, 31, 15, 30, 12)
            .single()
            .expect("valid local time");
        let name = build_csv_filename(3, now);
        assert_eq!(name, "3-20260531_153012.csv");
    }

    #[test]
    fn resolve_csv_path_should_append_suffix_on_conflict() {
        let dir = unique_temp_dir("conflict");
        let filename = "1-20260531_153012.csv";
        // 第一次：直接返回
        let p1 = resolve_csv_path_with_suffix(&dir, filename);
        assert_eq!(p1.file_name().unwrap().to_string_lossy(), filename);

        // 制造冲突
        fs::write(&p1, b"x").unwrap();
        let p2 = resolve_csv_path_with_suffix(&dir, filename);
        let p2_name = p2.file_name().unwrap().to_string_lossy().to_string();
        assert!(p2_name.starts_with("1-20260531_153012_"));
        assert!(p2_name.ends_with(".csv"));
        assert_ne!(p1, p2);

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn write_csv_new_file_should_emit_bom_header_and_rows() {
        let dir = unique_temp_dir("write");
        let entries = vec![
            TikTokEntry {
                id: "vid1".into(),
                timestamp: 1717000000,
                uploader: "user_a".into(),
                like_count: 100,
                comment_count: 20,
                repost_count: 3,
                save_count: 4,
                view_count: 9999,
            },
            TikTokEntry {
                id: "vid,2".into(),
                timestamp: 0,
                uploader: "user_\"b\"".into(),
                like_count: 0,
                comment_count: 0,
                repost_count: 0,
                save_count: 0,
                view_count: 0,
            },
        ];
        let started_at = chrono::TimeZone::with_ymd_and_hms(&Local, 2026, 5, 31, 15, 30, 12)
            .single()
            .unwrap();
        let filename = build_csv_filename(1, started_at);
        let path = write_csv_new_file(&dir, &filename, &entries, started_at).unwrap();

        let bytes = fs::read(&path).unwrap();
        assert_eq!(&bytes[..3], &[0xEF, 0xBB, 0xBF], "应有 UTF-8 BOM");
        let body = String::from_utf8(bytes[3..].to_vec()).unwrap();
        assert!(body.starts_with("video_id,publish_time,"));
        assert!(body.contains("\r\n"), "应使用 CRLF 行尾");
        assert!(body.contains("vid1"));
        // 包含逗号的字段必须加引号
        assert!(body.contains("\"vid,2\""));
        // fetched_at 写入了正确格式
        assert!(body.contains("2026-05-31 15:30:12"));
        // video_url 拼装规则
        assert!(body.contains("https://www.tiktok.com/@user_a/video/vid1"));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn format_local_time_should_handle_zero_and_negative() {
        assert_eq!(format_local_time(0), "");
        assert_eq!(format_local_time(-1), "");
        assert!(!format_local_time(1717000000).is_empty());
    }

    #[test]
    fn build_yt_dlp_args_should_inject_iid_and_truncate_max_fetch() {
        let args = build_yt_dlp_args("https://www.tiktok.com/music/abc", "7501732030001269264", 1000);
        // 必含 extractor-args 与 IID
        let joined = args.join(" ");
        assert!(joined.contains("tiktok:app_info=7501732030001269264/musical_ly/35.1.3/2023501030/1233"));
        assert!(joined.contains("--flat-playlist"));
        assert!(joined.contains("--playlist-end 1000"));
        assert!(joined.contains("--print"));
        assert!(joined.contains("https://www.tiktok.com/music/abc"));
    }

    #[test]
    fn build_yt_dlp_args_should_clamp_zero_to_one() {
        let args = build_yt_dlp_args("https://www.tiktok.com/x", "1234567890", 0);
        let idx = args.iter().position(|s| s == "--playlist-end").unwrap();
        assert_eq!(args[idx + 1], "1");
    }
}
