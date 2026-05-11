use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use crate::AppState;
use log::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSegment {
    pub segment_index: usize,
    pub source_folder: String,
    pub crop_mode: CropMode,
    pub duration: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CropMode {
    Single,
    Dual,
    Quadrant,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoConfig {
    pub id: String,
    pub name: String,
    pub root_folder: String,
    pub video_ratio: String,
    pub audio_path: String,
    pub audio_duration: u32,
    pub template_duration: u32,
    pub segment_count: usize,
    pub template_segments: Vec<TemplateSegment>,
    pub tutorial_folder: String,
    pub output_folder: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl VideoConfig {
    pub fn new(name: String) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            name,
            root_folder: String::new(),
            video_ratio: "9:16".to_string(),
            audio_path: String::new(),
            audio_duration: 0,
            template_duration: 150,
            segment_count: 3,
            template_segments: Vec::new(),
            tutorial_folder: String::new(),
            output_folder: String::new(),
            created_at: now,
            updated_at: now,
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("配置名称不能为空".to_string());
        }

        if self.audio_path.trim().is_empty() {
            return Err("音频文件必须选择".to_string());
        }

        let total_segment_duration: u32 = self
            .template_segments
            .iter()
            .map(|s| s.duration)
            .sum();

        if total_segment_duration != self.template_duration {
            return Err(format!(
                "片段时长之和不等于模板片段总时长: {} != {}",
                total_segment_duration, self.template_duration
            ));
        }

        Ok(())
    }
}

#[tauri::command]
pub fn get_configs(state: tauri::State<AppState>) -> Result<Vec<VideoConfig>, String> {
    let configs = state.configs.read().map_err(|e: std::sync::PoisonError<std::sync::RwLockReadGuard<'_, Vec<VideoConfig>>>| e.to_string())?;
    Ok(configs.clone())
}

#[tauri::command]
pub fn get_config(state: tauri::State<AppState>, id: String) -> Result<Option<VideoConfig>, String> {
    let configs = state.configs.read().map_err(|e: std::sync::PoisonError<std::sync::RwLockReadGuard<'_, Vec<VideoConfig>>>| e.to_string())?;
    Ok(configs.iter().find(|c| c.id == id).cloned())
}

#[tauri::command]
pub fn save_config(state: tauri::State<AppState>, config: VideoConfig) -> Result<VideoConfig, String> {
    info!("save_config called with config name: {}, id: {}", config.name, config.id);
    config.validate()?;

    let mut configs = state.configs.write().map_err(|e: std::sync::PoisonError<std::sync::RwLockWriteGuard<'_, Vec<VideoConfig>>>| e.to_string())?;

    if let Some(existing) = configs.iter().find(|c| c.id == config.id) {
        if existing.name != config.name {
            return Err("配置名称不能重复".to_string());
        }
    }

    let mut config = config;
    config.updated_at = Utc::now();

    if let Some(pos) = configs.iter().position(|c| c.id == config.id) {
        info!("Updating existing config at position {}", pos);
        configs[pos] = config.clone();
    } else {
        config.id = Uuid::new_v4().to_string();
        config.created_at = Utc::now();
        config.updated_at = Utc::now();
        info!("Creating new config with id: {}", config.id);
        configs.push(config.clone());
    }
    
    info!("Total configs in state: {}", configs.len());

    Ok(config)
}

#[tauri::command]
pub fn delete_config(state: tauri::State<AppState>, id: String) -> Result<(), String> {
    let mut configs = state.configs.write().map_err(|e: std::sync::PoisonError<std::sync::RwLockWriteGuard<'_, Vec<VideoConfig>>>| e.to_string())?;
    configs.retain(|c| c.id != id);
    Ok(())
}

#[tauri::command]
pub fn get_audio_duration(audio_path: String) -> Result<u32, String> {
    let output = std::process::Command::new("ffprobe")
        .args([
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            &audio_path,
        ])
        .output()
        .map_err(|e| format!("获取音频时长失败: {}", e))?;

    if !output.status.success() {
        return Err("ffprobe 执行失败".to_string());
    }

    let duration_str = String::from_utf8_lossy(&output.stdout);
    let duration_secs: f64 = duration_str.trim().parse().map_err(|_| "解析时长失败".to_string())?;
    Ok(duration_secs as u32)
}
