use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppData {
    pub configs: Vec<super::config::VideoConfig>,
    pub tasks: Vec<super::video_processor::Task>,
    pub usage_records: HashMap<String, UsageRecord>,
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

pub fn get_app_data_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(app_data_dir)
}

pub fn get_output_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = get_app_data_dir(app)?;
    let output_dir = app_data_dir.join("output");
    fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;
    Ok(output_dir)
}

pub fn get_temp_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let app_data_dir = get_app_data_dir(app)?;
    let temp_dir = app_data_dir.join("temp");
    fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;
    Ok(temp_dir)
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

    let data = AppData {
        configs,
        tasks,
        usage_records: HashMap::new(),
    };

    let content = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())?;
    fs::write(&data_file, content).map_err(|e| e.to_string())?;
    Ok(())
}
