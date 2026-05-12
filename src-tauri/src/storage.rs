use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tauri::Manager;
use log::{info, error};

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

#[tauri::command]
pub fn load_data(app: tauri::AppHandle) -> Result<AppData, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    let data_file = app_data_dir.join("app_data.json");

    if !data_file.exists() {
        return Ok(AppData::default());
    }

    let content = fs::read_to_string(&data_file).map_err(|e| e.to_string())?;
    let data: AppData = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    Ok(data)
}

#[tauri::command]
pub fn save_data(app: tauri::AppHandle, data: AppData) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    fs::create_dir_all(&app_data_dir).map_err(|e| e.to_string())?;
    let data_file = app_data_dir.join("app_data.json");

    let content = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
    fs::write(&data_file, content).map_err(|e| e.to_string())?;
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
pub fn get_data_file_path(app: tauri::AppHandle) -> Result<String, String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    let data_file = app_data_dir.join("app_data.json");
    Ok(data_file.to_string_lossy().to_string())
}

#[tauri::command]
pub fn save_configs(
    app: tauri::AppHandle,
    configs: Vec<super::config::VideoConfig>,
    tasks: Vec<super::video_processor::Task>,
) -> Result<(), String> {
    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?;
    fs::create_dir_all(&app_data_dir).map_err(|e| e.to_string())?;
    let data_file = app_data_dir.join("app_data.json");

    info!("Saving {} configs and {} tasks to {:?}", configs.len(), tasks.len(), data_file);

    let (usage_records, used_tutorial_videos) = if data_file.exists() {
        if let Ok(content) = fs::read_to_string(&data_file) {
            if let Ok(existing_data) = serde_json::from_str::<AppData>(&content) {
                info!("Preserving {} existing usage records and {} used tutorial videos",
                    existing_data.usage_records.len(),
                    existing_data.used_tutorial_videos.len());
                (existing_data.usage_records, existing_data.used_tutorial_videos)
            } else {
                info!("Failed to parse existing usage records, starting fresh");
                (HashMap::new(), Vec::new())
            }
        } else {
            info!("Failed to read existing data file, starting fresh");
            (HashMap::new(), Vec::new())
        }
    } else {
        info!("No existing data file found, starting fresh");
        (HashMap::new(), Vec::new())
    };

    // 与运行中内存里的教程已用集合合并（如果调用方已经在内存中追加了新条目）
    let app_state = app.state::<crate::AppState>();
    let merged_used_tutorial: Vec<String> = {
        let mut set: std::collections::HashSet<String> = used_tutorial_videos.into_iter().collect();
        if let Ok(in_memory) = app_state.used_tutorial_videos.read() {
            for v in in_memory.iter() {
                set.insert(v.clone());
            }
        }
        set.into_iter().collect()
    };

    let data = AppData {
        configs,
        tasks,
        usage_records,
        used_tutorial_videos: merged_used_tutorial,
    };

    let content = serde_json::to_string_pretty(&data).map_err(|e| {
        error!("Failed to serialize data: {}", e);
        e.to_string()
    })?;
    
    fs::write(&data_file, &content).map_err(|e| {
        error!("Failed to write file: {}", e);
        e.to_string()
    })?;
    
    info!("Successfully saved data to {:?}", data_file);
    Ok(())
}
