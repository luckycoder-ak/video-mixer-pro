use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use crate::AppState;
use crate::scheduled_fetcher::ScheduledFetcher;
use log::info;
use crate::video_processor::apply_hidden_process_startup;
use crate::video_processor::find_ffprobe_executable;

fn default_transition_duration() -> f32 {
    0.2
}

fn default_scale_percent() -> u32 {
    51
}

/// 默认字幕字号
fn default_subtitle_fontsize() -> u32 {
    36
}

/// 默认字幕颜色（白色）
fn default_subtitle_fontcolor() -> String {
    "white".to_string()
}

/// 默认字幕描边宽度
fn default_subtitle_borderw() -> u32 {
    3
}

/// 默认字幕描边颜色
fn default_subtitle_bordercolor() -> String {
    "black".to_string()
}

/// 默认字幕水平位置
fn default_subtitle_x() -> String {
    "(w-tw)/2".to_string()
}

/// 默认字幕垂直位置（底部往上 100 像素）
fn default_subtitle_y() -> String {
    "h-th-100".to_string()
}

/// 默认字幕阴影色（白色）
fn default_subtitle_shadowcolor() -> String {
    "white".to_string()
}

/// 默认字幕阴影偏移（实现辉光）
fn default_subtitle_shadowx() -> i32 {
    2
}

fn default_subtitle_shadowy() -> i32 {
    2
}

/// 默认字幕渐变色开关（逐字渐变）
fn default_subtitle_enable_gradient() -> bool {
    false
}

/// 渐变起始色（FFmpeg drawtext 表达式格式，如 0x00FF00）
fn default_subtitle_gradient_color1() -> String {
    "0xFF6B6B".to_string()
}

/// 渐变结束色
fn default_subtitle_gradient_color2() -> String {
    "0x4ECDC4".to_string()
}

/// 字幕样式配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubtitleStyle {
    /// 字号
    #[serde(default = "default_subtitle_fontsize")]
    pub fontsize: u32,
    /// 字体颜色（FFmpeg 颜色格式：white / 0xRRGGBB）
    #[serde(default = "default_subtitle_fontcolor")]
    pub fontcolor: String,
    /// 描边宽度
    #[serde(default = "default_subtitle_borderw")]
    pub borderw: u32,
    /// 描边颜色
    #[serde(default = "default_subtitle_bordercolor")]
    pub bordercolor: String,
    /// 阴影色
    #[serde(default = "default_subtitle_shadowcolor")]
    pub shadowcolor: String,
    /// 阴影 X 偏移（正值 = 右移，负值 = 左移；配合实现辉光/发光效果）
    #[serde(default = "default_subtitle_shadowx")]
    pub shadowx: i32,
    /// 阴影 Y 偏移（正值 = 下移，负值 = 上移）
    #[serde(default = "default_subtitle_shadowy")]
    pub shadowy: i32,
    /// 水平位置（FFmpeg 表达式，如 (w-tw)/2 = 水平居中）
    #[serde(default = "default_subtitle_x")]
    pub x: String,
    /// 垂直位置（FFmpeg 表达式，如 h-th-50 = 底部）
    #[serde(default = "default_subtitle_y")]
    pub y: String,
    /// 行间距
    #[serde(default)]
    pub line_spacing: u32,
    /// 启用逐字渐变
    #[serde(default = "default_subtitle_enable_gradient")]
    pub enable_gradient: bool,
    /// 渐变起始色
    #[serde(default = "default_subtitle_gradient_color1")]
    pub gradient_color1: String,
    /// 渐变结束色
    #[serde(default = "default_subtitle_gradient_color2")]
    pub gradient_color2: String,
}

impl Default for SubtitleStyle {
    fn default() -> Self {
        Self {
            fontsize: default_subtitle_fontsize(),
            fontcolor: default_subtitle_fontcolor(),
            borderw: default_subtitle_borderw(),
            bordercolor: default_subtitle_bordercolor(),
            shadowcolor: default_subtitle_shadowcolor(),
            shadowx: default_subtitle_shadowx(),
            shadowy: default_subtitle_shadowy(),
            x: default_subtitle_x(),
            y: default_subtitle_y(),
            line_spacing: 8,
            enable_gradient: default_subtitle_enable_gradient(),
            gradient_color1: default_subtitle_gradient_color1(),
            gradient_color2: default_subtitle_gradient_color2(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSegment {
    pub segment_index: usize,
    pub source_folder: String,
    #[serde(default)]
    pub source_folder2: String,
    pub crop_mode: CropMode,
    pub duration: f32,
    #[serde(default = "default_scale_percent")]
    pub scale_percent: u32,
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
    #[serde(default)]
    pub subtitle_style: SubtitleStyle,
    pub template_duration: f32,
    pub segment_count: usize,
    pub template_segments: Vec<TemplateSegment>,
    pub tutorial_folder: String,
    pub output_folder: String,
    #[serde(default)]
    pub enable_transition: bool,
    #[serde(default = "default_transition_duration")]
    pub transition_duration: f32,
    /// 该配置下挂载的定时元数据采集任务列表（CronFetcher）。
    #[serde(default)]
    pub scheduled_fetchers: Vec<ScheduledFetcher>,
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
            subtitle_style: SubtitleStyle::default(),
            template_duration: 150.0,
            segment_count: 3,
            template_segments: Vec::new(),
            tutorial_folder: String::new(),
            output_folder: String::new(),
            enable_transition: false,
            transition_duration: default_transition_duration(),
            scheduled_fetchers: Vec::new(),
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
            // 如果配置了第二个文件夹（仅双列模式有意义），也需要验证
            if !seg.source_folder2.trim().is_empty() {
                if !std::path::Path::new(seg.source_folder2.trim()).is_dir() {
                    return Err(format!(
                        "第 {} 个片段的第二个来源文件夹不存在: {}",
                        expected_index, seg.source_folder2
                    ));
                }
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
    let mut command = std::process::Command::new(find_ffprobe_executable());
    apply_hidden_process_startup(&mut command);
    let output = command
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

#[cfg(test)]
mod compat_tests {
    use super::*;

    /// 旧版 JSON：完全没有 `scheduled_fetchers` 字段，应反序列化成功且默认空数组。
    #[test]
    fn video_config_should_deserialize_legacy_json_without_scheduled_fetchers() {
        let raw = r#"{
            "id": "abc",
            "name": "demo",
            "root_folder": "/tmp",
            "video_ratio": "9:16",
            "audio_path": "/tmp/a.mp3",
            "audio_duration": 1.0,
            "subtitle_path": "",
            "template_duration": 30.0,
            "segment_count": 1,
            "template_segments": [],
            "tutorial_folder": "",
            "output_folder": "/tmp",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z"
        }"#;
        let cfg: VideoConfig = serde_json::from_str(raw).expect("should parse legacy json");
        assert!(cfg.scheduled_fetchers.is_empty());
        assert_eq!(cfg.transition_duration, 0.2);
        assert_eq!(cfg.enable_transition, false);
    }

    /// 新版部分字段缺失：`scheduled_fetchers[*]` 缺 `max_fetch_num` / `num_meet_condition` /
    /// `tiktok_iid` / `enabled` / `cooldown_until`，全部走 default 兜底。
    #[test]
    fn scheduled_fetcher_should_deserialize_with_partial_fields() {
        let raw = r#"{
            "id": "f1",
            "name": "demo CronFetcher #1",
            "target_url": "https://www.tiktok.com/music/x",
            "window_days": 1,
            "window_hours": 0,
            "interval_days": 0,
            "interval_hours": 1,
            "output_dir": "/tmp/x",
            "created_at": "2026-01-01T00:00:00Z",
            "updated_at": "2026-01-01T00:00:00Z"
        }"#;
        let f: ScheduledFetcher = serde_json::from_str(raw).expect("should parse partial json");
        assert_eq!(f.max_fetch_num, 1000);
        assert_eq!(f.num_meet_condition, 100);
        assert_eq!(f.tiktok_iid, crate::scheduled_fetcher::DEFAULT_TIKTOK_IID);
        assert_eq!(f.enabled, true);
        assert_eq!(f.consecutive_failures, 0);
        assert!(f.cooldown_until.is_none());
    }
}
