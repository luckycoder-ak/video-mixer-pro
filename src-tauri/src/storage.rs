use serde::{Deserialize, Serialize};
use serde::de::DeserializeOwned;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;
use log::info;

const TASKS_FILE_NAME: &str = "tasks.json";
const USED_TUTORIAL_FILE_NAME: &str = "used_tutorial_videos.json";
const TASK_RETENTION_DAYS: i64 = 30;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppData {
    pub configs: Vec<super::config::VideoConfig>,
    pub tasks: Vec<super::video_processor::Task>,
    #[serde(default)]
    pub usage_records: HashMap<String, UsageRecord>,
    /// 教程素材全局已用记录：绝对路径字符串集合，跨任务持久化，永不复用。
    #[serde(default)]
    pub used_tutorial_videos: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub tutorial_folder_hash: String,
    pub used_videos: Vec<UsedVideo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsedVideo {
    pub video_path: String,
    pub video_hash: String,
    pub used_at: chrono::DateTime<chrono::Utc>,
    pub task_id: String,
}

pub struct RuntimeStoreData {
    pub app_data: AppData,
    pub used_tutorial_by_config: HashMap<String, HashSet<String>>,
}

/**
 解析 app_data.json 的默认存储路径。

 参数:
 - 无

 返回:
 - `Ok(PathBuf)`: 当前进程工作目录下的 `app_data.json`
 - `Err(String)`: 获取当前工作目录失败

 异常:
 - 当进程当前工作目录不可访问时，返回错误字符串。
 */
pub fn resolve_app_data_file_path() -> Result<PathBuf, String> {
    let current_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    Ok(current_dir.join("app_data.json"))
}

#[allow(dead_code)]
/**
 解析 app_data_store 的默认存储目录。

 参数:
 - 无

 返回:
 - `Ok(PathBuf)`: 当前进程工作目录下的 `app_data_store`
 - `Err(String)`: 获取当前工作目录失败

 异常:
 - 当进程当前工作目录不可访问时，返回错误字符串。
 */
pub fn resolve_app_data_store_dir() -> Result<PathBuf, String> {
    let current_dir = std::env::current_dir().map_err(|e| e.to_string())?;
    Ok(current_dir.join("app_data_store"))
}

/**
 解析指定工作目录下的 app_data_store 路径。

 参数:
 - `base_dir`: 应用数据根目录

 返回:
 - `PathBuf`: `base_dir/app_data_store`

 异常:
 - 无；仅做路径拼接。
 */
fn resolve_app_data_store_dir_from(base_dir: &Path) -> PathBuf {
    base_dir.join("app_data_store")
}

/**
 解析单个配置目录路径。

 参数:
 - `store_dir`: `app_data_store` 根目录
 - `config_id`: 配置 ID

 返回:
 - `PathBuf`: `app_data_store/<config_id>`

 异常:
 - 无；仅做路径拼接。
 */
fn resolve_config_store_dir(store_dir: &Path, config_id: &str) -> PathBuf {
    store_dir.join(config_id)
}

/**
 读取 JSON 文件；文件不存在时返回默认值。

 参数:
 - `path`: JSON 文件路径

 返回:
 - `Ok(T)`: 读取并反序列化成功，或文件不存在时返回默认值
 - `Err(String)`: 读取或反序列化失败

 异常:
 - 当文件内容不是合法 JSON 时返回错误字符串。
 */
fn read_json_or_default<T>(path: &Path) -> Result<T, String>
where
    T: DeserializeOwned + Default,
{
    if !path.exists() {
        return Ok(T::default());
    }
    let content = fs::read_to_string(path).map_err(|e| e.to_string())?;
    serde_json::from_str(&content).map_err(|e| e.to_string())
}

/**
 将数据写入 JSON 文件。

 参数:
 - `path`: 目标文件路径
 - `value`: 待序列化对象

 返回:
 - `Ok(())`: 写入成功
 - `Err(String)`: 创建目录、序列化或写入失败

 异常:
 - 当上级目录创建失败或写文件失败时返回错误字符串。
 */
fn write_json_pretty<T>(path: &Path, value: &T) -> Result<(), String>
where
    T: Serialize,
{
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let content = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    if path.exists() {
        let existing = fs::read_to_string(path).map_err(|e| e.to_string())?;
        if existing == content {
            return Ok(());
        }
    }
    fs::write(path, content).map_err(|e| e.to_string())
}

/**
 计算任务是否应被 30 天保留策略清理。

 参数:
 - `task`: 待判断任务
 - `cutoff`: 保留窗口截止时间

 返回:
 - `true`: 该任务应被清理
 - `false`: 该任务应继续保留

 异常:
 - 无。
 */
fn should_prune_task(task: &super::video_processor::Task, cutoff: chrono::DateTime<chrono::Utc>) -> bool {
    let final_status = matches!(
        task.status,
        super::video_processor::TaskStatus::Completed
            | super::video_processor::TaskStatus::Error
            | super::video_processor::TaskStatus::Partial
    );
    if !final_status {
        return false;
    }
    let reference_time = task
        .completed_at
        .or(task.started_at)
        .unwrap_or(task.created_at);
    reference_time < cutoff
}

/**
 按 30 天窗口清理历史已完成/失败任务。

 参数:
 - `tasks`: 原始任务列表

 返回:
 - `(Vec<Task>, usize)`: 清理后的任务列表和被删除的任务数

 异常:
 - 无。
 */
fn prune_old_tasks(
    tasks: Vec<super::video_processor::Task>,
) -> (Vec<super::video_processor::Task>, usize) {
    let cutoff = chrono::Utc::now() - chrono::Duration::days(TASK_RETENTION_DAYS);
    let original_len = tasks.len();
    let retained: Vec<_> = tasks
        .into_iter()
        .filter(|task| !should_prune_task(task, cutoff))
        .collect();
    let removed = original_len.saturating_sub(retained.len());
    (retained, removed)
}

/**
 根据任务自身的 `config_id` 或配置名称，解析任务归属的配置 ID。

 参数:
 - `task`: 待归类任务
 - `configs`: 当前配置列表

 返回:
 - `Some(String)`: 解析到的配置 ID
 - `None`: 无法归类到任何配置

 异常:
 - 无。
 */
fn resolve_task_config_id(
    task: &super::video_processor::Task,
    configs: &[super::config::VideoConfig],
) -> Option<String> {
    if !task.config_id.trim().is_empty() {
        return Some(task.config_id.clone());
    }
    configs
        .iter()
        .find(|config| config.name == task.name)
        .map(|config| config.id.clone())
}

/**
 将单个配置下的任务写入 store，并执行 30 天清理。

 参数:
 - `store_dir`: `app_data_store` 根目录
 - `config_id`: 配置 ID
 - `tasks`: 该配置下的任务列表

 返回:
 - `Ok(usize)`: 被清理的旧任务数量
 - `Err(String)`: 写入文件失败

 异常:
 - 当序列化任务或写文件失败时返回错误字符串。
 */
fn write_tasks_to_store(
    store_dir: &Path,
    config_id: &str,
    tasks: Vec<super::video_processor::Task>,
) -> Result<usize, String> {
    let config_dir = resolve_config_store_dir(store_dir, config_id);
    fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;
    let (retained_tasks, removed_count) = prune_old_tasks(tasks);
    write_json_pretty(&config_dir.join(TASKS_FILE_NAME), &retained_tasks)?;
    Ok(removed_count)
}

/**
 将单个配置下的教程去重集合写入 store。

 参数:
 - `store_dir`: `app_data_store` 根目录
 - `config_id`: 配置 ID
 - `used_tutorial_videos`: 教程已用路径集合

 返回:
 - `Ok(())`: 写入成功
 - `Err(String)`: 写入失败

 异常:
 - 当序列化或写文件失败时返回错误字符串。
 */
fn write_used_tutorial_to_store(
    store_dir: &Path,
    config_id: &str,
    used_tutorial_videos: &HashSet<String>,
) -> Result<(), String> {
    let config_dir = resolve_config_store_dir(store_dir, config_id);
    fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;
    let mut values: Vec<String> = used_tutorial_videos.iter().cloned().collect();
    values.sort();
    write_json_pretty(&config_dir.join(USED_TUTORIAL_FILE_NAME), &values)
}

/**
 从配置目录读取任务列表，并执行 30 天清理回写。

 参数:
 - `config_dir`: 单个配置目录

 返回:
 - `Ok(Vec<Task>)`: 清理后的任务列表
 - `Err(String)`: 读取或写回失败

 异常:
 - 当 JSON 解析失败时返回错误字符串。
 */
fn load_tasks_from_config_dir(config_dir: &Path) -> Result<Vec<super::video_processor::Task>, String> {
    let tasks_file = config_dir.join(TASKS_FILE_NAME);
    let tasks: Vec<super::video_processor::Task> = read_json_or_default(&tasks_file)?;
    let (retained_tasks, removed_count) = prune_old_tasks(tasks);
    if removed_count > 0 {
        write_json_pretty(&tasks_file, &retained_tasks)?;
    }
    Ok(retained_tasks)
}

/**
 从配置目录读取教程去重集合。

 参数:
 - `config_dir`: 单个配置目录

 返回:
 - `Ok(HashSet<String>)`: 已用教程集合
 - `Err(String)`: 读取失败

 异常:
 - 当 JSON 解析失败时返回错误字符串。
 */
fn load_used_tutorial_from_config_dir(config_dir: &Path) -> Result<HashSet<String>, String> {
    let values: Vec<String> = read_json_or_default(&config_dir.join(USED_TUTORIAL_FILE_NAME))?;
    Ok(values.into_iter().collect())
}

/**
 从 app_data_store 聚合读取全部任务和教程去重集合，配置列表以 `app_data.json` 为准。

 参数:
 - `base_dir`: 应用数据根目录
 - `configs`: 当前配置列表

 返回:
 - `Ok(RuntimeStoreData)`: 聚合后的运行时数据
 - `Err(String)`: 读取目录或文件失败

 异常:
 - 当某个配置目录中的 JSON 非法时返回错误字符串。
 */
fn load_runtime_data_from_store(
    base_dir: &Path,
    configs: &[super::config::VideoConfig],
) -> Result<RuntimeStoreData, String> {
    let store_dir = resolve_app_data_store_dir_from(base_dir);
    fs::create_dir_all(&store_dir).map_err(|e| e.to_string())?;

    let mut tasks: Vec<super::video_processor::Task> = Vec::new();
    let mut used_tutorial_by_config: HashMap<String, HashSet<String>> = HashMap::new();

    for config in configs {
        let config_id = config.id.clone();
        let config_dir = resolve_config_store_dir(&store_dir, &config_id);
        let mut config_tasks = load_tasks_from_config_dir(&config_dir)?;
        for task in config_tasks.iter_mut() {
            task.config_id = config_id.clone();
        }

        let used_tutorial = load_used_tutorial_from_config_dir(&config_dir)?;
        used_tutorial_by_config.insert(config_id.clone(), used_tutorial);
        tasks.extend(config_tasks);
    }

    tasks.sort_by(|a, b| a.created_at.cmp(&b.created_at));

    Ok(RuntimeStoreData {
        app_data: AppData {
            configs: configs.to_vec(),
            tasks,
            usage_records: HashMap::new(),
            used_tutorial_videos: Vec::new(),
        },
        used_tutorial_by_config,
    })
}

/**
 同步配置目录到 app_data_store，并清理已不存在配置的目录。

 参数:
 - `base_dir`: 应用数据根目录（与 `app_data.json` 同级）
 - `configs`: 当前有效的配置列表

 返回:
 - `Ok(PathBuf)`: 已同步的 `app_data_store` 目录路径
 - `Err(String)`: 创建目录或清理旧目录失败

 异常:
 - 当文件系统操作失败时返回错误字符串。
 */
fn sync_config_store_in_dir(
    base_dir: &Path,
    configs: &[super::config::VideoConfig],
) -> Result<PathBuf, String> {
    let store_dir = resolve_app_data_store_dir_from(base_dir);
    fs::create_dir_all(&store_dir).map_err(|e| e.to_string())?;

    let active_config_ids: HashSet<&str> = configs.iter().map(|config| config.id.as_str()).collect();
    for config in configs {
        let config_dir = resolve_config_store_dir(&store_dir, &config.id);
        fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;
    }

    for entry in fs::read_dir(&store_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(dir_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if !active_config_ids.contains(dir_name) {
            fs::remove_dir_all(&path).map_err(|e| e.to_string())?;
        }
    }

    Ok(store_dir)
}

/**
 初始化 app_data_store，并把旧 `app_data.json` 中的任务迁移到按配置拆分的目录。

 参数:
 - `configs`: 当前从旧存储加载到内存中的配置列表
 - `legacy_tasks`: 旧 `app_data.json` 中的任务列表，仅在目标 `tasks.json` 不存在时迁移

 返回:
 - `Ok(PathBuf)`: 已初始化完成的 `app_data_store` 目录路径
 - `Err(String)`: 获取工作目录、创建目录或迁移任务失败

 异常:
 - 当文件系统操作或配置写入失败时返回错误字符串。
 */
pub fn initialize_app_data_store(
    configs: &[super::config::VideoConfig],
    legacy_tasks: &[super::video_processor::Task],
) -> Result<PathBuf, String> {
    let data_file = resolve_app_data_file_path()?;
    let base_dir = data_file
        .parent()
        .ok_or_else(|| "无法解析 app_data.json 所在目录".to_string())?;
    let store_dir = sync_config_store_in_dir(base_dir, configs)?;

    let mut legacy_tasks_by_config: HashMap<String, Vec<super::video_processor::Task>> = HashMap::new();
    for task in legacy_tasks {
        let Some(config_id) = resolve_task_config_id(task, configs) else {
            continue;
        };
        let mut task = task.clone();
        task.config_id = config_id.clone();
        legacy_tasks_by_config.entry(config_id).or_default().push(task);
    }

    for config in configs {
        let config_dir = resolve_config_store_dir(&store_dir, &config.id);
        let tasks_file = config_dir.join(TASKS_FILE_NAME);
        if !tasks_file.exists() {
            let legacy = legacy_tasks_by_config.remove(&config.id).unwrap_or_default();
            let removed_count = write_tasks_to_store(&store_dir, &config.id, legacy)?;
            if removed_count > 0 {
                info!("初始化迁移时清理配置 {} 的 {} 个旧任务", config.id, removed_count);
            }
        } else {
            let _ = load_tasks_from_config_dir(&config_dir)?;
        }

        let used_tutorial_file = config_dir.join(USED_TUTORIAL_FILE_NAME);
        if !used_tutorial_file.exists() {
            write_json_pretty(&used_tutorial_file, &Vec::<String>::new())?;
        }
    }

    Ok(store_dir)
}

/**
 将当前运行时数据持久化到 app_data_store，并将 `app_data.json` 精简为轻量索引。

 参数:
 - `base_dir`: 应用数据根目录
 - `configs`: 当前配置列表
 - `tasks`: 当前任务列表
 - `used_tutorial_by_config`: 按配置隔离的教程去重集合
 - `usage_records`: 旧格式中的额外使用记录

 返回:
 - `Ok((PathBuf, Vec<Task>))`: store 目录路径和清理后的任务列表
 - `Err(String)`: 写入失败

 异常:
 - 当文件系统操作失败时返回错误字符串。
 */
pub fn persist_runtime_store_in_dir(
    base_dir: &Path,
    configs: &[super::config::VideoConfig],
    tasks: &[super::video_processor::Task],
    used_tutorial_by_config: &HashMap<String, HashSet<String>>,
    usage_records: HashMap<String, UsageRecord>,
) -> Result<(PathBuf, Vec<super::video_processor::Task>), String> {
    let store_dir = sync_config_store_in_dir(base_dir, configs)?;

    let mut tasks_by_config: HashMap<String, Vec<super::video_processor::Task>> = HashMap::new();
    for task in tasks {
        let Some(config_id) = resolve_task_config_id(task, configs) else {
            info!("跳过无法归类配置的任务: {}", task.id);
            continue;
        };
        let mut normalized_task = task.clone();
        normalized_task.config_id = config_id.clone();
        tasks_by_config.entry(config_id).or_default().push(normalized_task);
    }

    let mut persisted_tasks: Vec<super::video_processor::Task> = Vec::new();
    for config in configs {
        let removed_count = write_tasks_to_store(
            &store_dir,
            &config.id,
            tasks_by_config.remove(&config.id).unwrap_or_default(),
        )?;
        if removed_count > 0 {
            info!("配置 {} 清理了 {} 个超过 30 天的旧任务", config.id, removed_count);
        }

        let config_dir = resolve_config_store_dir(&store_dir, &config.id);
        let config_tasks = load_tasks_from_config_dir(&config_dir)?;
        persisted_tasks.extend(config_tasks);

        let used = used_tutorial_by_config
            .get(&config.id)
            .cloned()
            .unwrap_or_default();
        write_used_tutorial_to_store(&store_dir, &config.id, &used)?;
    }

    persisted_tasks.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    let minimal_data = AppData {
        configs: configs.to_vec(),
        tasks: Vec::new(),
        usage_records,
        used_tutorial_videos: Vec::new(),
    };
    write_json_pretty(&base_dir.join("app_data.json"), &minimal_data)?;

    Ok((store_dir, persisted_tasks))
}

/**
 加载当前运行时所需的配置、任务与按配置隔离的教程去重集合。

 参数:
 - 无

 返回:
 - `Ok(RuntimeStoreData)`: 聚合后的运行时数据
 - `Err(String)`: 读取失败

 异常:
 - 当旧文件或 store 中的 JSON 解析失败时返回错误字符串。
 */
pub fn load_runtime_store() -> Result<RuntimeStoreData, String> {
    let data_file = resolve_app_data_file_path()?;
    let legacy_data = if data_file.exists() {
        let content = fs::read_to_string(&data_file).map_err(|e| e.to_string())?;
        serde_json::from_str::<AppData>(&content).map_err(|e| e.to_string())?
    } else {
        AppData::default()
    };

    let base_dir = data_file
        .parent()
        .ok_or_else(|| "无法解析 app_data.json 所在目录".to_string())?;
    initialize_app_data_store(&legacy_data.configs, &legacy_data.tasks)?;

    let mut runtime_data = load_runtime_data_from_store(base_dir, &legacy_data.configs)?;
    runtime_data.app_data.usage_records = legacy_data.usage_records;
    Ok(runtime_data)
}

#[tauri::command]
pub fn load_data(_app: tauri::AppHandle) -> Result<AppData, String> {
    load_runtime_store().map(|runtime| runtime.app_data)
}

#[tauri::command]
pub fn save_data(_app: tauri::AppHandle, data: AppData) -> Result<(), String> {
    let data_file = resolve_app_data_file_path()?;
    let base_dir = data_file
        .parent()
        .ok_or_else(|| "无法解析 app_data.json 所在目录".to_string())?;
    let used_tutorial_by_config: HashMap<String, HashSet<String>> = HashMap::new();
    let _ = persist_runtime_store_in_dir(
        base_dir,
        &data.configs,
        &data.tasks,
        &used_tutorial_by_config,
        data.usage_records,
    )?;
    Ok(())
}

#[allow(dead_code)]
pub fn get_app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(app_data_dir)
}

#[allow(dead_code)]
pub fn get_output_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = get_app_data_dir(app)?;
    let output_dir = app_data_dir.join("output");
    fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;
    Ok(output_dir)
}

#[allow(dead_code)]
pub fn get_temp_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = get_app_data_dir(app)?;
    let temp_dir = app_data_dir.join("temp");
    fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;
    Ok(temp_dir)
}

#[tauri::command]
pub fn get_data_file_path(_app: tauri::AppHandle) -> Result<String, String> {
    let data_file = resolve_app_data_file_path()?;
    Ok(data_file.to_string_lossy().to_string())
}

#[tauri::command]
pub fn save_configs(
    app: tauri::AppHandle,
    configs: Vec<super::config::VideoConfig>,
    tasks: Vec<super::video_processor::Task>,
) -> Result<(), String> {
    let data_file = resolve_app_data_file_path()?;
    if let Some(parent) = data_file.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    info!("Saving {} configs and {} tasks to {:?}", configs.len(), tasks.len(), data_file);

    let usage_records = if data_file.exists() {
        if let Ok(content) = fs::read_to_string(&data_file) {
            if let Ok(existing_data) = serde_json::from_str::<AppData>(&content) {
                info!("Preserving {} existing usage records", existing_data.usage_records.len());
                existing_data.usage_records
            } else {
                info!("Failed to parse existing usage records, starting fresh");
                HashMap::new()
            }
        } else {
            info!("Failed to read existing data file, starting fresh");
            HashMap::new()
        }
    } else {
        info!("No existing data file found, starting fresh");
        HashMap::new()
    };

    let app_state = app.state::<crate::AppState>();
    let base_dir = data_file
        .parent()
        .ok_or_else(|| "无法解析 app_data.json 所在目录".to_string())?;
    let used_tutorial_by_config = app_state
        .used_tutorial_videos
        .read()
        .map_err(|e| e.to_string())?
        .clone();
    let (store_dir, persisted_tasks) = persist_runtime_store_in_dir(
        base_dir,
        &configs,
        &tasks,
        &used_tutorial_by_config,
        usage_records,
    )?;

    if let Ok(mut state_tasks) = app_state.tasks.write() {
        *state_tasks = persisted_tasks;
    }
    info!("Successfully saved data to {:?}", data_file);
    info!("Successfully synced config store to {:?}", store_dir);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;
    use chrono::Utc;
    use std::time::{SystemTime, UNIX_EPOCH};
    use uuid::Uuid;

    /**
     构造测试用配置。

     参数:
     - `id`: 配置 ID
     - `name`: 配置名称

     返回:
     - `VideoConfig`: 最小可序列化配置对象

     异常:
     - 无。
     */
    fn build_test_config(id: &str, name: &str) -> super::super::config::VideoConfig {
        let now = Utc::now();
        super::super::config::VideoConfig {
            id: id.to_string(),
            name: name.to_string(),
            root_folder: "/tmp/root".to_string(),
            video_ratio: "9:16".to_string(),
            audio_path: "/tmp/audio.mp3".to_string(),
            audio_duration: 12.3,
            subtitle_path: String::new(),
            template_duration: 20.0,
            segment_count: 0,
            template_segments: Vec::new(),
            tutorial_folder: "/tmp/tutorial".to_string(),
            output_folder: "/tmp/output".to_string(),
            enable_transition: false,
            transition_duration: 0.2,
            created_at: now,
            updated_at: now,
        }
    }

    /**
     创建测试临时目录。

     参数:
     - 无

     返回:
     - `PathBuf`: 新建的临时目录路径

     异常:
     - 当创建目录失败时直接 panic，供单测快速暴露问题。
     */
    fn create_test_root_dir() -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root_dir = std::env::temp_dir().join(format!("video_mixer_storage_test_{unique}"));
        fs::create_dir_all(&root_dir).unwrap();
        root_dir
    }

    /**
     构造测试用任务。

     参数:
     - `config_id`: 配置 ID
     - `config_name`: 配置名称
     - `status`: 任务状态
     - `created_at`: 任务创建时间

     返回:
     - `Task`: 可序列化的测试任务对象

     异常:
     - 无。
     */
    fn build_test_task(
        config_id: &str,
        config_name: &str,
        status: super::super::video_processor::TaskStatus,
        created_at: chrono::DateTime<Utc>,
    ) -> super::super::video_processor::Task {
        super::super::video_processor::Task {
            id: Uuid::new_v4().to_string(),
            name: config_name.to_string(),
            config_id: config_id.to_string(),
            task_name: format!("{}_task", config_name),
            total_count: 1,
            completed_count: 1,
            failed_count: 0,
            failed_videos: Vec::new(),
            status,
            output_folder: "/tmp/output".to_string(),
            created_at,
            started_at: Some(created_at),
            completed_at: Some(created_at),
            error_message: None,
            current_video: 1,
            progress_steps: Vec::new(),
            logs: Vec::new(),
        }
    }

    #[test]
    fn sync_config_store_should_create_store_dirs_without_config_files() {
        let temp_dir = create_test_root_dir();
        let configs = vec![
            build_test_config("config-a", "配置A"),
            build_test_config("config-b", "配置B"),
        ];

        let store_dir = sync_config_store_in_dir(&temp_dir, &configs).unwrap();

        assert!(store_dir.exists());
        assert!(store_dir.join("config-a").is_dir());
        assert!(store_dir.join("config-b").is_dir());
        assert!(!store_dir.join("config-a").join("config.json").exists());
        assert!(!store_dir.join("config-b").join("config.json").exists());
        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn sync_config_store_should_remove_stale_config_dirs() {
        let temp_dir = create_test_root_dir();
        let store_dir = resolve_app_data_store_dir_from(&temp_dir);
        fs::create_dir_all(store_dir.join("old-config")).unwrap();
        fs::write(store_dir.join("old-config").join(TASKS_FILE_NAME), "[]").unwrap();

        let configs = vec![build_test_config("config-a", "配置A")];
        let store_dir = sync_config_store_in_dir(&temp_dir, &configs).unwrap();

        assert!(store_dir.join("config-a").is_dir());
        assert!(!store_dir.join("config-a").join("config.json").exists());
        assert!(!store_dir.join("old-config").exists());
        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn initialize_app_data_store_should_migrate_legacy_tasks_by_config() {
        let temp_dir = create_test_root_dir();
        let original_dir = std::env::current_dir().unwrap();
        std::env::set_current_dir(&temp_dir).unwrap();

        let configs = vec![build_test_config("config-a", "配置A")];
        let legacy_tasks = vec![build_test_task(
            "",
            "配置A",
            super::super::video_processor::TaskStatus::Completed,
            Utc::now(),
        )];

        let store_dir = initialize_app_data_store(&configs, &legacy_tasks).unwrap();
        let migrated_tasks: Vec<super::super::video_processor::Task> =
            read_json_or_default(&store_dir.join("config-a").join(TASKS_FILE_NAME)).unwrap();
        let migrated_used: Vec<String> =
            read_json_or_default(&store_dir.join("config-a").join(USED_TUTORIAL_FILE_NAME)).unwrap();

        assert_eq!(migrated_tasks.len(), 1);
        assert_eq!(migrated_tasks[0].config_id, "config-a");
        assert!(migrated_used.is_empty());
        assert!(!store_dir.join("config-a").join("config.json").exists());

        std::env::set_current_dir(&original_dir).unwrap();
        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn persist_runtime_store_should_prune_old_finished_tasks_and_keep_running_tasks() {
        let temp_dir = create_test_root_dir();
        let configs = vec![build_test_config("config-a", "配置A")];
        let old_time = Utc::now() - chrono::Duration::days(31);
        let now = Utc::now();
        let tasks = vec![
            build_test_task(
                "config-a",
                "配置A",
                super::super::video_processor::TaskStatus::Completed,
                old_time,
            ),
            build_test_task(
                "config-a",
                "配置A",
                super::super::video_processor::TaskStatus::Error,
                old_time,
            ),
            build_test_task(
                "config-a",
                "配置A",
                super::super::video_processor::TaskStatus::Running,
                old_time,
            ),
            build_test_task(
                "config-a",
                "配置A",
                super::super::video_processor::TaskStatus::Completed,
                now,
            ),
        ];
        let mut used_tutorial_by_config = HashMap::new();
        used_tutorial_by_config.insert(
            "config-a".to_string(),
            HashSet::from_iter(vec!["/tmp/tutorial-a.mp4".to_string()]),
        );

        let (store_dir, persisted_tasks) = persist_runtime_store_in_dir(
            &temp_dir,
            &configs,
            &tasks,
            &used_tutorial_by_config,
            HashMap::new(),
        )
        .unwrap();

        let tasks_on_disk: Vec<super::super::video_processor::Task> =
            read_json_or_default(&store_dir.join("config-a").join(TASKS_FILE_NAME)).unwrap();
        let used_on_disk: Vec<String> =
            read_json_or_default(&store_dir.join("config-a").join(USED_TUTORIAL_FILE_NAME)).unwrap();

        assert_eq!(persisted_tasks.len(), 2);
        assert_eq!(tasks_on_disk.len(), 2);
        assert!(tasks_on_disk.iter().all(|task| {
            task.status == super::super::video_processor::TaskStatus::Running
                || task.created_at == now
        }));
        assert_eq!(used_on_disk, vec!["/tmp/tutorial-a.mp4".to_string()]);
        let app_data: AppData =
            read_json_or_default(&temp_dir.join("app_data.json")).unwrap();
        assert_eq!(app_data.configs.len(), 1);
        assert_eq!(app_data.configs[0].id, "config-a");
        assert!(store_dir.join("config-a").is_dir());
        assert!(!store_dir.join("config-a").join("config.json").exists());

        fs::remove_dir_all(&temp_dir).unwrap();
    }

    #[test]
    fn write_json_pretty_should_skip_rewrite_when_content_unchanged() {
        let temp_dir = create_test_root_dir();
        let config_file = temp_dir.join("config.json");
        let config = build_test_config("config-a", "配置A");

        write_json_pretty(&config_file, &config).unwrap();
        let first_modified = fs::metadata(&config_file).unwrap().modified().unwrap();

        sleep(Duration::from_millis(1100));
        write_json_pretty(&config_file, &config).unwrap();
        let second_modified = fs::metadata(&config_file).unwrap().modified().unwrap();

        assert_eq!(first_modified, second_modified);
        fs::remove_dir_all(&temp_dir).unwrap();
    }
}
