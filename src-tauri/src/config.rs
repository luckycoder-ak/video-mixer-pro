use serde::{Deserialize, Serialize};
use std::fs;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use crate::AppState;
use log::info;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSegment {
    pub segment_index: usize,
    pub source_folder: String,
    pub crop_mode: CropMode,
    pub duration: f32,
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
    pub audio_duration: f32,
    pub subtitle_path: String,
    pub template_duration: f32,
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
            audio_duration: 0.0,
            subtitle_path: String::new(),
            template_duration: 150.0,
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

        if self.root_folder.trim().is_empty() {
            return Err("主目录不能为空".to_string());
        }
        if !std::path::Path::new(self.root_folder.trim()).is_dir() {
            return Err(format!("主目录不存在或不是目录: {}", self.root_folder));
        }

        const ALLOWED_RATIOS: &[&str] = &["9:16", "16:9", "1:1", "4:5"];
        if !ALLOWED_RATIOS.contains(&self.video_ratio.trim()) {
            return Err(format!(
                "不支持的视频比例: {}，允许的值: {}",
                self.video_ratio,
                ALLOWED_RATIOS.join(", ")
            ));
        }

        if self.audio_path.trim().is_empty() {
            return Err("音频文件必须选择".to_string());
        }
        if !std::path::Path::new(self.audio_path.trim()).is_file() {
            return Err(format!("音频文件不存在: {}", self.audio_path));
        }

        if !self.subtitle_path.trim().is_empty()
            && !std::path::Path::new(self.subtitle_path.trim()).is_file()
        {
            return Err(format!("字幕文件不存在: {}", self.subtitle_path));
        }

        if self.template_duration <= 0.0 {
            return Err("模板片段总时长必须大于 0".to_string());
        }

        if self.segment_count == 0 {
            return Err("片段数量必须大于 0".to_string());
        }
        if self.segment_count != self.template_segments.len() {
            return Err(format!(
                "片段数量不一致: segment_count={} 与 template_segments.len()={}",
                self.segment_count,
                self.template_segments.len()
            ));
        }

        for (idx, seg) in self.template_segments.iter().enumerate() {
            let expected_index = idx + 1;
            if seg.segment_index != expected_index {
                return Err(format!(
                    "第 {} 个片段的 segment_index 应为 {}，实际为 {}",
                    expected_index, expected_index, seg.segment_index
                ));
            }
            if seg.duration <= 0.0 {
                return Err(format!("第 {} 个片段的时长必须大于 0", expected_index));
            }
            if seg.source_folder.trim().is_empty() {
                return Err(format!("第 {} 个片段未选择来源文件夹", expected_index));
            }
            if !std::path::Path::new(seg.source_folder.trim()).is_dir() {
                return Err(format!(
                    "第 {} 个片段的来源文件夹不存在: {}",
                    expected_index, seg.source_folder
                ));
            }
        }

        if !self.tutorial_folder.trim().is_empty()
            && !std::path::Path::new(self.tutorial_folder.trim()).is_dir()
        {
            return Err(format!("教程素材文件夹不存在: {}", self.tutorial_folder));
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
pub fn get_audio_duration(audio_path: String) -> Result<f32, String> {
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
    Ok(duration_secs as f32)
}

#[tauri::command]
pub fn import_config(state: tauri::State<AppState>, config: VideoConfig) -> Result<VideoConfig, String> {
    info!("import_config called with config name: {}", config.name);

    let mut new_config = config.clone();
    new_config.id = Uuid::new_v4().to_string();
    new_config.created_at = Utc::now();
    new_config.updated_at = Utc::now();

    let mut configs = state.configs.write().map_err(|e| e.to_string())?;

    if configs.iter().any(|c| c.name == new_config.name) {
        return Err(format!("配置名称 '{}' 已存在", new_config.name));
    }

    if !new_config.root_folder.is_empty() {
        if let Err(e) = fs::create_dir_all(&new_config.root_folder) {
            info!("创建主目录失败: {}", e);
        }
    }

    if !new_config.output_folder.is_empty() {
        if let Err(e) = fs::create_dir_all(&new_config.output_folder) {
            info!("创建输出目录失败: {}", e);
        }
    }

    if !new_config.tutorial_folder.is_empty() {
        if let Err(e) = fs::create_dir_all(&new_config.tutorial_folder) {
            info!("创建教程目录失败: {}", e);
        }
    }

    for segment in &new_config.template_segments {
        if !segment.source_folder.is_empty() {
            if let Err(e) = fs::create_dir_all(&segment.source_folder) {
                info!("创建片段目录 {} 失败: {}", segment.source_folder, e);
            }
        }
    }

    configs.push(new_config.clone());
    info!("配置导入成功: {}", new_config.name);

    Ok(new_config)
}
