use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;
use std::thread;

use log::{error, info, warn};
use rand::Rng;
use serde::{Deserialize, Serialize};
use tauri::Manager;
use uuid::Uuid;
use chrono::{DateTime, Utc};

use crate::AppState;
use crate::config;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Running,
    Paused,
    Completed,
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
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub name: String,
    pub config_id: String,
    pub task_name: String,
    pub total_count: usize,
    pub completed_count: usize,
    pub status: TaskStatus,
    pub output_folder: String,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub current_video: usize,
    pub progress_steps: Vec<TaskStep>,
}

impl Task {
    pub fn new(config_name: String, total_count: usize) -> Self {
        let timestamp = Utc::now().timestamp();
        Task {
            id: Uuid::new_v4().to_string(),
            name: config_name,
            config_id: String::new(),
            task_name: format!("{}-{}", config_name, timestamp),
            total_count,
            completed_count: 0,
            status: TaskStatus::Pending,
            output_folder: String::new(),
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            error_message: None,
            current_video: 0,
            progress_steps: Vec::new(),
        }
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

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct VideoInfo {
    pub path: PathBuf,
    pub duration: f64,
}

pub fn get_video_files(folder: &str) -> Result<Vec<PathBuf>, String> {
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

    if available.is_empty() {
        return Err("没有可用的视频文件".to_string());
    }

    let actual_count = count.min(available.len());
    let mut selected = Vec::new();
    
    let mut indices: Vec<usize> = (0..available.len()).collect();
    let mut rng = rand::thread_rng();
    
    for i in 0..actual_count {
        if i < indices.len() {
            let random_index = rng.gen_range(0..indices.len());
            let video_index = indices.remove(random_index);
            selected.push(available[video_index].clone());
        } else {
            break;
        }
    }

    Ok(selected)
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

fn detect_best_encoder() -> &'static EncoderConfig {
    static ENCODER: OnceLock<EncoderConfig> = OnceLock::new();
    
    ENCODER.get_or_init(|| {
        let output = Command::new("ffmpeg")
            .args(["-hide_banner", "-encoders"])
            .output();
        
        match output {
            Ok(out) => {
                let stdout = String::from_utf8_lossy(&out.stdout);
                
                if stdout.contains("h264_nvenc") {
                    info!("使用 NVIDIA GPU 加速编码器");
                    &EncoderConfig {
                        video_codec: "h264_nvenc",
                        audio_codec: "aac",
                        use_hw_accel: true,
                    }
                } else if stdout.contains("h264_vaapi") {
                    info!("使用 VAAPI 硬件加速编码器");
                    &EncoderConfig {
                        video_codec: "h264_vaapi",
                        audio_codec: "aac",
                        use_hw_accel: true,
                    }
                } else if stdout.contains("h264_qsv") {
                    info!("使用 Intel QSV 硬件加速编码器");
                    &EncoderConfig {
                        video_codec: "h264_qsv",
                        audio_codec: "aac",
                        use_hw_accel: true,
                    }
                } else {
                    info!("使用软件编码器 (libx264)");
                    &EncoderConfig {
                        video_codec: "libx264",
                        audio_codec: "aac",
                        use_hw_accel: false,
                    }
                }
            }
            Err(e) => {
                warn!("检测编码器失败: {}, 使用默认软件编码器", e);
                &EncoderConfig {
                    video_codec: "libx264",
                    audio_codec: "aac",
                    use_hw_accel: false,
                }
            }
        }
    })
}

pub fn get_random_transition_type() -> String {
    let transitions = vec![
        "fade".to_string(),
        "dissolve".to_string(),
        "wipeleft".to_string(),
        "wiperight".to_string(),
        "wipeup".to_string(),
        "wipedown".to_string(),
    ];
    
    let mut rng = rand::thread_rng();
    let index = rng.gen_range(0..transitions.len());
    transitions[index].clone()
}

fn trim_segment(
    input: &PathBuf,
    output: &PathBuf,
    start: f32,
    duration: f32,
) -> Result<(), String> {
    let input_str = input.to_string_lossy().to_string();
    let output_str = output.to_string_lossy().to_string();
    let encoder = detect_best_encoder();
    
    let args: Vec<&str> = vec![
        "-hide_banner",
        "-loglevel", "error",
        "-ss", &start.to_string(),
        "-i", &input_str,
        "-t", &duration.to_string(),
        "-c", "copy",
        "-y", &output_str,
    ];
    
    run_ffmpeg_fast(&args)?;
    Ok(())
}

fn create_transition_effect(
    input1: &PathBuf,
    input2: &PathBuf,
    output: &PathBuf,
    transition_type: &str,
    duration: f32,
) -> Result<(), String> {
    let input1_str = input1.to_string_lossy().to_string();
    let input2_str = input2.to_string_lossy().to_string();
    let output_str = output.to_string_lossy().to_string();
    
    let encoder = detect_best_encoder();
    
    let filter_complex = format!(
        "[0:v]settb=AVTB,setpts=PTS-STARTPTS[v0];[1:v]settb=AVTB,setpts=PTS-STARTPTS[v1];[v0][v1]xfade=transition={}:duration={}:offset=0[final]",
        transition_type, duration
    );
    
    let num_cpus = num_cpus::get().to_string();
    
    let args: Vec<&str> = vec![
        "-hide_banner",
        "-loglevel", "error",
        "-i", &input1_str,
        "-i", &input2_str,
        "-filter_complex", &filter_complex,
        "-map", "[final]",
        "-c:v", encoder.video_codec,
        "-pix_fmt", "yuv420p",
        "-movflags", "+faststart",
        "-threads", &num_cpus,
        "-y", &output_str,
    ];
    
    run_ffmpeg_fast(&args)?;
    Ok(())
}

fn process_segment(
    videos: &[PathBuf],
    output: &PathBuf,
    crop_mode: &config::CropMode,
    (output_width, output_height): (u32, u32),
) -> Result<(), String> {
    match crop_mode {
        config::CropMode::Single => {
            process_single_mode_segment(videos, output, output_width, output_height)
        }
        config::CropMode::Dual => {
            process_dual_mode_optimized(videos, output, output_width, output_height)
        }
        config::CropMode::Quadrant => {
            process_quadrant_mode_optimized(videos, output, output_width, output_height)
        }
    }
}

fn process_single_mode_segment(
    videos: &[PathBuf],
    output: &PathBuf,
    output_width: u32,
    output_height: u32,
) -> Result<(), String> {
    let video = &videos[0];
    let input_str = video.to_string_lossy().to_string();
    let output_str = output.to_string_lossy().to_string();
    
    let encoder = detect_best_encoder();
    
    let filter_str = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black",
        output_width, output_height, output_width, output_height
    );
    
    let num_cpus = num_cpus::get().to_string();
    
    let args: Vec<&str> = vec![
        "-hide_banner",
        "-loglevel", "error",
        "-i", &input_str,
        "-vf", &filter_str,
        "-c:v", encoder.video_codec,
        "-c:a", "copy",
        "-pix_fmt", "yuv420p",
        "-movflags", "+faststart",
        "-threads", &num_cpus,
        "-y", &output_str,
    ];
    
    run_ffmpeg_fast(&args)?;
    Ok(())
}

fn process_dual_mode_optimized(
    videos: &[PathBuf],
    output: &PathBuf,
    output_width: u32,
    output_height: u32,
) -> Result<(), String> {
    let video = &videos[0];
    let input1_str = video.to_string_lossy().to_string();
    
    let video = &videos[1];
    let input2_str = video.to_string_lossy().to_string();
    let output_str = output.to_string_lossy().to_string();
    
    let temp_dir = std::env::temp_dir();
    let left_scaled = temp_dir.join(format!("left_scaled_{}.mp4", Uuid::new_v4()));
    let right_scaled = temp_dir.join(format!("right_scaled_{}.mp4", Uuid::new_v4()));
    
    let left_scaled_str = left_scaled.to_string_lossy().to_string();
    let right_scaled_str = right_scaled.to_string_lossy().to_string();
    
    let half_width = output_width / 2;
    let encoder = detect_best_encoder();
    let num_cpus = num_cpus::get().to_string();
    
    let left_scale_filter = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black",
        half_width, output_height, half_width, output_height
    );
    
    let right_scale_filter = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black",
        half_width, output_height, half_width, output_height
    );
    
    let left_args: Vec<&str> = vec![
        "-hide_banner",
        "-loglevel", "error",
        "-i", &input1_str,
        "-vf", &left_scale_filter,
        "-c:v", encoder.video_codec,
        "-pix_fmt", "yuv420p",
        "-threads", &num_cpus,
        "-y", &left_scaled_str,
    ];
    
    let right_args: Vec<&str> = vec![
        "-hide_banner",
        "-loglevel", "error",
        "-i", &input2_str,
        "-vf", &right_scale_filter,
        "-c:v", encoder.video_codec,
        "-pix_fmt", "yuv420p",
        "-threads", &num_cpus,
        "-y", &right_scaled_str,
    ];
    
    let left_handle = thread::spawn(move || {
        run_ffmpeg_fast(&left_args)
    });
    
    let right_handle = thread::spawn(move || {
        run_ffmpeg_fast(&right_args)
    });
    
    let left_result = left_handle.join().map_err(|e| format!("线程 {} 合并失败: {:?}", "left", e));
    let right_result = right_handle.join().map_err(|e| format!("线程 {} 合并失败: {:?}", "right", e));
    
    match (left_result, right_result) {
        (Ok(Ok(())), Ok(Ok(()))) => {
            info!("双视频片段处理成功");
        }
        (Ok(Err(e)), Ok(Ok(()))) => {
            error!("左视频处理失败: {}", e);
            return Err(format!("左视频处理失败: {}", e));
        }
        (Ok(Ok(())), Ok(Err(e))) => {
            error!("右视频处理失败: {}", e);
            return Err(format!("右视频处理失败: {}", e));
        }
        (Err(e), _) => {
            error!("左线程崩溃: {:?}", e);
            return Err(format!("左线程崩溃: {:?}", e));
        }
        (_, Err(e)) => {
            error!("右线程崩溃: {:?}", e);
            return Err(format!("右线程崩溃: {:?}", e));
        }
    }
    
    let output_str = output.to_string_lossy().to_string();
    
    let merge_filter = format!(
        "[0:v][1:v]hstack=inputs=2[out]",
    );
    
    let merge_args: Vec<&str> = vec![
        "-hide_banner",
        "-loglevel", "error",
        "-i", &left_scaled_str,
        "-i", &right_scaled_str,
        "-filter_complex", &merge_filter,
        "-map", "[out]",
        "-c:v", encoder.video_codec,
        "-pix_fmt", "yuv420p",
        "-threads", &num_cpus,
        "-y", &output_str,
    ];
    
    run_ffmpeg_fast(&merge_args)?;
    
    let _ = fs::remove_file(&left_scaled);
    let _ = fs::remove_file(&right_scaled);
    
    Ok(())
}

fn process_quadrant_mode_optimized(
    videos: &[PathBuf],
    output: &PathBuf,
    output_width: u32,
    output_height: u32,
) -> Result<(), String> {
    let temp_dir = std::env::temp_dir();
    let tl_scaled = temp_dir.join(format!("tl_{}.mp4", Uuid::new_v4()));
    let tr_scaled = temp_dir.join(format!("tr_{}.mp4", Uuid::new_v4()));
    let bl_scaled = temp_dir.join(format!("bl_{}.mp4", Uuid::new_v4()));
    let br_scaled = temp_dir.join(format!("br_{}.mp4", Uuid::new_v4()));
    
    let tl_scaled_str = tl_scaled.to_string_lossy().to_string();
    let tr_scaled_str = tr_scaled.to_string_lossy().to_string();
    let bl_scaled_str = bl_scaled.to_string_lossy().to_string();
    let br_scaled_str = br_scaled.to_string_lossy().to_string();
    
    let half_width = output_width / 2;
    let half_height = output_height / 2;
    let encoder = detect_best_encoder();
    let num_cpus = num_cpus::get().to_string();
    
    let tl_filter = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black",
        half_width, half_height, half_width, half_height
    );
    let tr_filter = tl_filter.clone();
    let bl_filter = tl_filter.clone();
    let br_filter = tl_filter.clone();
    
    let tl_input = videos[0].to_string_lossy().to_string();
    let tr_input = videos[1].to_string_lossy().to_string();
    let bl_input = videos[2].to_string_lossy().to_string();
    let br_input = videos[3].to_string_lossy().to_string();
    
    let tl_args: Vec<&str> = vec![
        "-hide_banner", "-loglevel", "error",
        "-i", &tl_input, "-vf", &tl_filter,
        "-c:v", encoder.video_codec, "-pix_fmt", "yuv420p",
        "-threads", &num_cpus, "-y", &tl_scaled_str,
    ];
    
    let tr_args: Vec<&str> = vec![
        "-hide_banner", "-loglevel", "error",
        "-i", &tr_input, "-vf", &tr_filter,
        "-c:v", encoder.video_codec, "-pix_fmt", "yuv420p",
        "-threads", &num_cpus, "-y", &tr_scaled_str,
    ];
    
    let bl_args: Vec<&str> = vec![
        "-hide_banner", "-loglevel", "error",
        "-i", &bl_input, "-vf", &bl_filter,
        "-c:v", encoder.video_codec, "-pix_fmt", "yuv420p",
        "-threads", &num_cpus, "-y", &bl_scaled_str,
    ];
    
    let br_args: Vec<&str> = vec![
        "-hide_banner", "-loglevel", "error",
        "-i", &br_input, "-vf", &br_filter,
        "-c:v", encoder.video_codec, "-pix_fmt", "yuv420p",
        "-threads", &num_cpus, "-y", &br_scaled_str,
    ];
    
    let handle1 = thread::spawn(move || run_ffmpeg_fast(&tl_args));
    let handle2 = thread::spawn(move || run_ffmpeg_fast(&tr_args));
    let handle3 = thread::spawn(move || run_ffmpeg_fast(&bl_args));
    let handle4 = thread::spawn(move || run_ffmpeg_fast(&br_args));
    
    let handle1_result = handle1.join().map_err(|e| format!("线程 {} 合并失败: {:?}", "top-left", e));
    let handle2_result = handle2.join().map_err(|e| format!("线程 {} 合并失败: {:?}", "top-right", e));
    let handle3_result = handle3.join().map_err(|e| format!("线程 {} 合并失败: {:?}", "bottom-left", e));
    let handle4_result = handle4.join().map_err(|e| format!("线程 {} 合并失败: {:?}", "bottom-right", e));
    
    match (handle1_result, handle2_result, handle3_result, handle4_result) {
        (Ok(Ok(())), Ok(Ok(())), Ok(Ok(())), Ok(Ok(()))) => {
            info!("四宫格视频片段处理成功");
        }
        (Err(e), _, _, _) => {
            error!("左上视频线程崩溃: {:?}", e);
            return Err(format!("左上视频线程崩溃: {:?}", e));
        }
        (_, Err(e), _, _) => {
            error!("右上视频线程崩溃: {:?}", e);
            return Err(format!("右上视频线程崩溃: {:?}", e));
        }
        (_, _, Err(e), _) => {
            error!("左下视频线程崩溃: {:?}", e);
            return Err(format!("左下视频线程崩溃: {:?}", e));
        }
        (_, _, _, Err(e)) => {
            error!("右下视频线程崩溃: {:?}", e);
            return Err(format!("右下视频线程崩溃: {:?}", e));
        }
        (Ok(Err(e)), _, _, _) => {
            error!("左上视频处理失败: {}", e);
            return Err(format!("左上视频处理失败: {}", e));
        }
        (_, Ok(Err(e)), _, _) => {
            error!("右上视频处理失败: {}", e);
            return Err(format!("右上视频处理失败: {}", e));
        }
        (_, _, Ok(Err(e)), _) => {
            error!("左下视频处理失败: {}", e);
            return Err(format!("左下视频处理失败: {}", e));
        }
        (_, _, _, Ok(Err(e))) => {
            error!("右下视频处理失败: {}", e);
            return Err(format!("右下视频处理失败: {}", e));
        }
    }
    
    let output_str = output.to_string_lossy().to_string();
    
    let merge_filter = "[0:v][1:v]hstack[top];[2:v][3:v]hstack[bottom];[top][bottom]vstack=inputs=2[out]";
    
    let merge_args: Vec<&str> = vec![
        "-hide_banner", "-loglevel", "error",
        "-i", &tl_scaled_str, "-i", &tr_scaled_str,
        "-i", &bl_scaled_str, "-i", &br_scaled_str,
        "-filter_complex", merge_filter,
        "-map", "[out]",
        "-c:v", encoder.video_codec,
        "-pix_fmt", "yuv420p",
        "-threads", &num_cpus,
        "-y", &output_str,
    ];
    
    run_ffmpeg_fast(&merge_args)?;
    
    let _ = fs::remove_file(&tl_scaled);
    let _ = fs::remove_file(&tr_scaled);
    let _ = fs::remove_file(&bl_scaled);
    let _ = fs::remove_file(&br_scaled);
    
    Ok(())
}

pub fn run_ffmpeg_fast(args: &[&str]) -> Result<(), String> {
    let output = Command::new("ffmpeg")
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

pub fn probe_video_duration(video_path: &str) -> Result<f32, String> {
    let output = Command::new("ffprobe")
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

fn apply_chained_xfade(
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
        fs::copy(&segment_files[0], output)
            .map_err(|e| format!("复制文件失败: {}", e))?;
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
    
    let num_cpus = num_cpus::get().to_string();
    let output_str = output.to_string_lossy().to_string();
    
    let mut filter_parts: Vec<String> = Vec::new();
    
    for i in 0..segment_files.len() {
        filter_parts.push(format!(
            "[{}:v]settb=AVTB,setpts=PTS-STARTPTS,scale={}:{},setsar=sar=1,fps=30,format=yuv420p[v{}]",
            i, output_width, output_height, i
        ));
    }
    
    let mut offset = 0.0;
    for i in 1..segment_files.len() {
        if i == 1 {
            offset = segment_durations[0] - transition_duration;
        } else {
            offset += segment_durations[i - 1] - transition_duration;
        }
        
        filter_parts.push(format!(
            "[v{}][v{}]xfade=transition={}:duration={}:offset={}[v{}_out]",
            i - 1, i, transition_types[i - 1], transition_duration, offset, i
        ));
    }
    
    let last_idx = segment_files.len() - 1;
    let filter_complex_base = format!(
        "{};[v{}_out]format=yuv420p[final_video]",
        filter_parts.join(";"),
        last_idx
    );
    
    if !background_audio_path.is_empty() {
        args.extend(["-i".to_string(), background_audio_path.to_string()]);
        
        let total_duration = segment_durations.iter().fold(0.0, |acc, &d| acc + d) 
            - (transition_duration * (segment_files.len() - 1) as f32);
        
        let audio_input_index = segment_files.len();
        
        let filter_complex = format!(
            "{}[{}:a]volume=0.8,afade=t=in:ss=0:d=0.5,afade=t=out:st={}:d=1.0[a]",
            filter_complex_base,
            audio_input_index,
            (total_duration - 1.0).max(0.0)
        );
        
        args.extend([
            "-filter_complex".to_string(), filter_complex,
            "-map".to_string(), "[final_video]".to_string(),
            "-map".to_string(), "[a]".to_string(),
            "-c:v".to_string(), encoder.video_codec.clone(),
            "-c:a".to_string(), encoder.audio_codec.clone(),
            "-pix_fmt".to_string(), "yuv420p".to_string(),
            "-movflags".to_string(), "+faststart".to_string(),
            "-threads".to_string(), num_cpus,
            "-y".to_string(),
            output_str,
        ]);
    } else {
        args.extend([
            "-filter_complex".to_string(), filter_complex_base,
            "-map".to_string(), "[final_video]".to_string(),
            "-c:v".to_string(), encoder.video_codec.clone(),
            "-an".to_string(),
            "-pix_fmt".to_string(), "yuv420p".to_string(),
            "-movflags".to_string(), "+faststart".to_string(),
            "-threads".to_string(), num_cpus,
            "-y".to_string(),
            output_str,
        ]);
    }
    
    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    
    run_ffmpeg_fast(&args_refs)?;
    
    info!("转场处理完成");
    Ok(())
}

fn process_single_mode(
    template_segments: &[super::config::TemplateSegment],
    tutorial_folder: &str,
    video_ratio: &str,
    audio_path: &str,
    _audio_duration: f32,
    subtitle_path: &str,
    output_dir: &PathBuf,
    output_filename: &str,
) -> Result<(), String> {
    let (output_width, output_height) = calculate_video_dimensions(video_ratio);
    let temp_dir = std::env::temp_dir().join(format!("video_mixer_{}", Uuid::new_v4()));
    fs::create_dir_all(&temp_dir).map_err(|e| e.to_string())?;
    let _temp_dir_guard = TempDirGuard(temp_dir.clone());

    let mut segment_files: Vec<PathBuf> = Vec::new();
    let mut start_time: f32 = 0.0;
    let transition_duration: f32 = 0.5;

    for (i, segment) in template_segments.iter().enumerate() {
        let videos = get_video_files(&segment.source_folder)?;
        if videos.is_empty() {
            return Err(format!("片段 {} 的源文件夹中没有视频文件", i + 1));
        }

        let video_count = match segment.crop_mode {
            super::config::CropMode::Single => 1,
            super::config::CropMode::Dual => 2,
            super::config::CropMode::Quadrant => 4,
        };

        if videos.len() < video_count {
            return Err(format!("片段 {} 需要 {} 个视频文件，但源文件夹中只有 {} 个", i + 1, video_count, videos.len()));
        }

        let selected = select_random_videos(&videos, video_count, &HashSet::new())?;
        
        let actual_duration = if i < template_segments.len() - 1 {
            (segment.duration - transition_duration).max(0.5)
        } else {
            segment.duration
        };
        
        let trimmed_segment = temp_dir.join(format!("segment_{}.mp4", i));
        
        if selected.len() == 1 {
            process_segment(&selected, &trimmed_segment, &segment.crop_mode, (output_width, output_height))?;
        } else {
            process_segment(&selected, &trimmed_segment, &segment.crop_mode, (output_width, output_height))?;
        }
        
        segment_files.push(trimmed_segment);
        start_time += actual_duration;
    }

    if !tutorial_folder.is_empty() {
        let tutorial_videos = get_video_files(tutorial_folder)?;
        if !tutorial_videos.is_empty() {
            let tutorial_video = &tutorial_videos[0];
            let tutorial_scaled = temp_dir.join(format!("tutorial_scaled.mp4"));
            
            let tutorial_scale_filter = format!(
                "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black,fps=30",
                output_width, output_height, output_width, output_height
            );
            
            let tutorial_scaled_str = tutorial_scaled.to_string_lossy().to_string();
            let tutorial_input_str = tutorial_video.to_string_lossy().to_string();
            let encoder = detect_best_encoder();
            let num_cpus = num_cpus::get().to_string();
            
            let tutorial_args: Vec<&str> = vec![
                "-hide_banner",
                "-loglevel", "error",
                "-i", &tutorial_input_str,
                "-vf", &tutorial_scale_filter,
                "-c:v", encoder.video_codec,
                "-pix_fmt", "yuv420p",
                "-threads", &num_cpus,
                "-y", &tutorial_scaled_str,
            ];
            
            run_ffmpeg_fast(&tutorial_args)?;
            segment_files.insert(0, tutorial_scaled);
        }
    }

    let output_path = output_dir.join(output_filename);
    let temp_output_path = temp_dir.join("temp_output.mp4");
    
    let mut segment_durations: Vec<f32> = Vec::new();
    for (i, segment_file) in segment_files.iter().enumerate() {
        let duration = probe_video_duration(&segment_file.to_string_lossy())?;
        segment_durations.push(duration);
        info!("片段 {} 时长: {:.2}秒", i + 1, duration);
    }
    
    let mut transition_types: Vec<String> = Vec::new();
    for i in 0..segment_files.len().saturating_sub(1) {
        let transition = get_random_transition_type();
        transition_types.push(transition.clone());
        info!("转场 {} 类型: {}", i + 1, transition);
    }
    
    info!("开始应用链式转场...");
    apply_chained_xfade(
        &segment_files,
        &segment_durations,
        &transition_types,
        &temp_output_path,
        transition_duration,
        output_width,
        output_height,
        audio_path,
    )?;
    
    info!("转场处理完成，所有转场一次性完成！");
    info!("共 {} 个片段，{} 个转场", segment_files.len(), segment_files.len().saturating_sub(1));

    if !subtitle_path.is_empty() {
        add_subtitles(&temp_output_path, subtitle_path, &output_path)?;
    } else {
        fs::copy(&temp_output_path, &output_path)
            .map_err(|e| format!("复制视频文件失败: {}", e))?;
    }

    for file in &segment_files {
        match fs::remove_file(file) {
            Ok(()) => info!("已清理临时文件: {:?}", file),
            Err(e) => warn!("清理临时文件失败: {:?}, 错误: {}", file, e),
        }
    }
    
    if temp_output_path.exists() && temp_output_path != output_path {
        match fs::remove_file(&temp_output_path) {
            Ok(()) => info!("已清理中间输出文件: {:?}", temp_output_path),
            Err(e) => warn!("清理中间输出文件失败: {:?}, 错误: {}", temp_output_path, e),
        }
    }
    
    match fs::remove_dir(&temp_dir) {
        Ok(()) => info!("已清理临时目录: {:?}", temp_dir),
        Err(e) => warn!("清理临时目录失败: {:?}, 错误: {}", temp_dir, e),
    }

    Ok(())
}

fn add_subtitles(input_path: &PathBuf, subtitle_path: &str, output_path: &PathBuf) -> Result<(), String> {
    info!("添加字幕: subtitle_path={}", subtitle_path);
    
    if subtitle_path.trim().is_empty() {
        info!("字幕路径为空，跳过字幕添加");
        fs::copy(input_path, output_path)
            .map_err(|e| format!("复制视频文件失败: {}", e))?;
        return Ok(());
    }
    
    let subtitle_path_buf = PathBuf::from(subtitle_path);
    if !subtitle_path_buf.exists() {
        error!("字幕文件不存在: {}", subtitle_path);
        fs::copy(input_path, output_path)
            .map_err(|e| format!("复制视频文件失败: {}", e))?;
        return Ok(());
    }
    
    let input_str = input_path.to_string_lossy().to_string();
    let output_str = output_path.to_string_lossy().to_string();
    
    let encoder = detect_best_encoder();
    
    let temp_dir = std::env::temp_dir();
    let temp_subtitle_path = temp_dir.join(format!("temp_subtitle_{}.srt", Uuid::new_v4()));
    
    fs::copy(&subtitle_path_buf, &temp_subtitle_path)
        .map_err(|e| format!("复制字幕文件失败: {}", e))?;
    info!("已复制字幕文件到临时位置: {:?}", temp_subtitle_path);
    
    let subtitle_ext = subtitle_path_buf.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_lowercase();
    
    let is_ass_format = subtitle_ext == "ass" || subtitle_ext == "ssa";
    
    let filter_str = if is_ass_format {
        let escaped_path = escape_ffmpeg_filter(&temp_subtitle_path.to_string_lossy());
        format!("ass='{}'", escaped_path)
    } else {
        let escaped_path = escape_ffmpeg_filter(&temp_subtitle_path.to_string_lossy());
        format!("subtitles='{}':si=0", escaped_path)
    };
    
    info!("字幕格式: {}, 滤镜: {}", 
        if is_ass_format { "ASS" } else { "SRT" }, 
        filter_str);
    
    let mut args: Vec<&str> = vec![
        "-hide_banner",
        "-loglevel", "error",
        "-i", &input_str,
        "-vf", &filter_str,
        "-c:v", &encoder.video_codec,
        "-c:a", "copy",
        "-pix_fmt", "yuv420p",
        "-movflags", "+faststart",
        "-y", &output_str,
    ];
    
    match run_ffmpeg_fast(&args) {
        Ok(_) => {
            info!("字幕添加成功: {:?}", output_path);
        }
        Err(e) => {
            error!("字幕添加失败: {}", e);
            error!("尝试备用方案...");
            
            let backup_filter = if is_ass_format {
                format!("ass={}", temp_subtitle_path.to_string_lossy())
            } else {
                format!("subtitles={}:si=0", temp_subtitle_path.to_string_lossy())
            };
            
            let mut backup_args: Vec<&str> = vec![
                "-hide_banner",
                "-loglevel", "error",
                "-i", &input_str,
                "-vf", &backup_filter,
                "-c:v", &encoder.video_codec,
                "-c:a", "copy",
                "-pix_fmt", "yuv420p",
                "-movflags", "+faststart",
                "-y", &output_str,
            ];
            
            match run_ffmpeg_fast(&backup_args) {
                Ok(_) => {
                    info!("备用字幕方案成功");
                }
                Err(e2) => {
                    error!("备用字幕方案也失败: {}", e2);
                    error!("无法添加字幕，保留原始视频");
                    fs::copy(input_path, output_path)
                        .map_err(|e| format!("复制视频文件失败: {}", e))?;
                }
            }
        }
    }
    
    if temp_subtitle_path.exists() {
        match fs::remove_file(&temp_subtitle_path) {
            Ok(()) => info!("已清理临时字幕文件: {:?}", temp_subtitle_path),
            Err(e) => warn!("清理临时字幕文件失败: {:?}, 错误: {}", temp_subtitle_path, e),
        }
    }
    
    Ok(())
}

fn escape_ffmpeg_filter(path: &str) -> String {
    let escaped = path.replace("\\", "\\\\").replace(":", "\\:").replace("'", "\\'");
    escaped
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
    task.output_folder = config.output_folder.clone();
    
    let mut tasks = state.tasks.write().map_err(|e| e.to_string())?;
    tasks.push(task.clone());
    info!("任务创建成功: id={}", task.id);
    
    let configs_clone = state.configs.clone();
    let tasks_clone = state.tasks.clone();
    let task_id = task.id.clone();
    let config_clone = config.clone();
    
    thread::spawn(move || {
        info!("开始处理任务: id={}", task_id);
        
        {
            let mut tasks = tasks_clone.write().map_err(|e| {
                error!("更新任务状态失败: {}", e);
                e.to_string()
            });
            if let Some(t) = tasks.iter_mut().find(|t| t.id == task_id) {
                t.status = super::video_processor::TaskStatus::Running;
                t.started_at = Some(chrono::Utc::now());
            }
        }
        
        let output_dir = PathBuf::from(&config_clone.output_folder);
        if !output_dir.exists() {
            if let Err(e) = fs::create_dir_all(&output_dir) {
                error!("创建输出目录失败: {}", e);
                let mut tasks = tasks_clone.write().map_err(|e| e.to_string());
                if let Some(t) = tasks.iter_mut().find(|t| t.id == task_id) {
                    t.status = super::video_processor::TaskStatus::Error;
                    t.error_message = Some(format!("创建输出目录失败: {}", e));
                }
                return;
            }
        }
        
        for i in 0..count {
            info!("处理第 {} 个视频，共 {} 个", i + 1, count);
            
            {
                let mut tasks = tasks_clone.write().map_err(|e| e.to_string());
                if let Some(t) = tasks.iter_mut().find(|t| t.id == task_id) {
                    t.current_video = i + 1;
                }
            }
            
            let output_filename = format!("{}_{}.mp4", config_clone.name, i + 1);
            
            match process_single_mode(
                &config_clone.template_segments,
                &config_clone.tutorial_folder,
                &config_clone.video_ratio,
                &config_clone.audio_path,
                config_clone.audio_duration,
                &config_clone.subtitle_path,
                &output_dir,
                &output_filename,
            ) {
                Ok(()) => {
                    info!("视频 {} 处理成功", i + 1);
                    
                    let mut tasks = tasks_clone.write().map_err(|e| e.to_string());
                    if let Some(t) = tasks.iter_mut().find(|t| t.id == task_id) {
                        t.completed_count += 1;
                    }
                }
                Err(e) => {
                    error!("视频 {} 处理失败: {}", i + 1, e);
                    
                    let mut tasks = tasks_clone.write().map_err(|e| e.to_string());
                    if let Some(t) = tasks.iter_mut().find(|t| t.id == task_id) {
                        t.status = super::video_processor::TaskStatus::Error;
                        t.error_message = Some(format!("视频 {} 处理失败: {}", i + 1, e));
                    }
                    return;
                }
            }
        }
        
        let mut tasks = tasks_clone.write().map_err(|e| e.to_string());
        if let Some(t) = tasks.iter_mut().find(|t| t.id == task_id) {
            t.status = super::video_processor::TaskStatus::Completed;
            t.completed_at = Some(chrono::Utc::now());
        }
        
        info!("任务 {} 全部完成", task_id);
    });
    
    Ok(task)
}

#[tauri::command]
pub fn pause_task(state: tauri::State<AppState>, id: String) -> Result<(), String> {
    info!("暂停任务: id={}", id);
    
    let mut tasks = state.tasks.write().map_err(|e| e.to_string())?;
    if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
        task.status = TaskStatus::Paused;
        info!("任务 {} 已暂停", id);
        Ok(())
    } else {
        Err(format!("任务 {} 不存在", id))
    }
}

#[tauri::command]
pub fn resume_task(state: tauri::State<AppState>, id: String) -> Result<(), String> {
    info!("恢复任务: id={}", id);
    
    let mut tasks = state.tasks.write().map_err(|e| e.to_string())?;
    if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
        task.status = TaskStatus::Running;
        info!("任务 {} 已恢复", id);
        Ok(())
    } else {
        Err(format!("任务 {} 不存在", id))
    }
}

#[tauri::command]
pub fn delete_task(state: tauri::State<AppState>, id: String) -> Result<(), String> {
    info!("删除任务: id={}", id);
    
    let mut tasks = state.tasks.write().map_err(|e| e.to_string())?;
    tasks.retain(|t| t.id != id);
    info!("任务 {} 已删除", id);
    Ok(())
}

#[tauri::command]
pub fn get_task(state: tauri::State<AppState>, id: String) -> Result<Task, String> {
    info!("获取任务: id={}", id);
    
    let tasks = state.tasks.read().map_err(|e| e.to_string())?;
    tasks
        .iter()
        .find(|t| t.id == id)
        .cloned()
        .ok_or_else(|| format!("任务 {} 不存在", id))
}
