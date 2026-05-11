use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::thread;

use log::{error, info};
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
    Completed,
    Paused,
    Error,
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
    pub config_id: String,
    pub config_name: String,
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
        Self {
            id: Uuid::new_v4().to_string(),
            config_id: String::new(),
            config_name: config_name.clone(),
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

    let mut files = Vec::new();
    let entries = fs::read_dir(&path).map_err(|e| e.to_string())?;

    for entry in entries {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension() {
                let ext_lower = ext.to_string_lossy().to_lowercase();
                if ["mp4", "avi", "mov", "mkv", "webm"].contains(&ext_lower.as_str()) {
                    files.push(path);
                }
            }
        }
    }

    Ok(files)
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

    for i in 0..actual_count {
        selected.push(available[i % available.len()].clone());
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

#[allow(dead_code)]
pub fn calculate_scaled_dimensions(
    source_width: u32,
    source_height: u32,
    target_width: u32,
    target_height: u32,
) -> (u32, u32) {
    let ratio = source_width as f64 / source_height as f64;
    let scaled_height = (target_width as f64 / ratio) as u32;
    (target_width, scaled_height.min(target_height))
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ProcessedSegment {
    pub output_path: PathBuf,
    pub duration: f64,
}

pub fn process_segment(
    videos: &[PathBuf],
    crop_mode: &super::config::CropMode,
    output_width: u32,
    output_height: u32,
    duration: f32,
    start_time: f32,
    temp_dir: &PathBuf,
) -> Result<ProcessedSegment, String> {
    match crop_mode {
        super::config::CropMode::Single => {
            let video = &videos[0];
            let output = temp_dir.join(format!("segment_{}.mp4", Uuid::new_v4()));
            process_single_mode_optimized(video, &output, output_width, output_height, duration, start_time)?;
            Ok(ProcessedSegment { output_path: output, duration: duration as f64 })
        }
        super::config::CropMode::Dual => {
            let output = temp_dir.join(format!("segment_{}.mp4", Uuid::new_v4()));
            process_dual_mode_optimized(&videos[0], &videos[1], &output, output_width, output_height, duration, start_time, temp_dir)?;
            Ok(ProcessedSegment { output_path: output, duration: duration as f64 })
        }
        super::config::CropMode::Quadrant => {
            let output = temp_dir.join(format!("segment_{}.mp4", Uuid::new_v4()));
            process_quadrant_mode_optimized(&videos[0], &videos[1], &videos[2], &videos[3], &output, output_width, output_height, duration, start_time, temp_dir)?;
            Ok(ProcessedSegment { output_path: output, duration: duration as f64 })
        }
    }
}

#[derive(Debug, Clone)]
struct EncoderConfig {
    video_codec: String,
    audio_codec: String,
    extra_args: Vec<String>,
}

fn detect_best_encoder() -> EncoderConfig {
    info!("正在检测可用的视频编码器...");
    
    let encoders = vec![
        ("h264_nvenc", "NVIDIA NVENC (GPU加速)"),
        ("h264_qsv", "Intel Quick Sync (GPU加速)"),
        ("h264_amf", "AMD AMF (GPU加速)"),
    ];

    for (codec, name) in encoders {
        let output = Command::new("ffmpeg")
            .args(&["-hide_banner", "-encoders"])
            .output();

        if let Ok(output) = output {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let combined = format!("{}\n{}", stdout, stderr);

            if combined.contains(codec) {
                info!("检测到硬件编码器: {}，使用该编码器加速视频处理", name);
                return match codec {
                    "h264_nvenc" => EncoderConfig {
                        video_codec: "h264_nvenc".to_string(),
                        audio_codec: "aac".to_string(),
                        extra_args: vec!["-preset".to_string(), "p4".to_string(), "-tune".to_string(), "hq".to_string()],
                    },
                    "h264_qsv" => EncoderConfig {
                        video_codec: "h264_qsv".to_string(),
                        audio_codec: "aac".to_string(),
                        extra_args: vec!["-preset".to_string(), "veryfast".to_string()],
                    },
                    "h264_amf" => EncoderConfig {
                        video_codec: "h264_amf".to_string(),
                        audio_codec: "aac".to_string(),
                        extra_args: vec!["-quality".to_string(), "balanced".to_string()],
                    },
                    _ => unreachable!(),
                };
            }
        }
    }

    info!("未检测到硬件编码器，使用软件编码器: libx264 (速度较慢，但兼容性好)");
    EncoderConfig {
        video_codec: "libx264".to_string(),
        audio_codec: "aac".to_string(),
        extra_args: vec!["-preset".to_string(), "ultrafast".to_string(), "-crf".to_string(), "28".to_string()],
    }
}

fn run_ffmpeg_fast(args: &[&str]) -> Result<(), String> {
    let status = Command::new("ffmpeg")
        .args(args)
        .status()
        .map_err(|e| format!("FFmpeg 执行失败: {}", e))?;

    if !status.success() {
        return Err("FFmpeg 处理失败".to_string());
    }

    Ok(())
}

fn process_single_mode_optimized(
    input: &PathBuf,
    output: &PathBuf,
    output_width: u32,
    output_height: u32,
    duration: f32,
    start_time: f32,
) -> Result<(), String> {
    let input_str = input.to_string_lossy().to_string();
    let output_str = output.to_string_lossy().to_string();
    let encoder = detect_best_encoder();
    let vf_str = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black",
        output_width, output_height, output_width, output_height
    );
    let duration_str = duration.to_string();
    let start_time_str = start_time.to_string();
    let video_codec = encoder.video_codec.clone();

    info!("开始处理单视频片段:");
    info!("  源视频: {}", input.file_name().unwrap_or_default().to_string_lossy());
    info!("  输出视频: {}", output.file_name().unwrap_or_default().to_string_lossy());
    info!("  输出尺寸: {}x{}", output_width, output_height);
    info!("  截取时长: {}秒，开始时间: {}秒", duration, start_time);

    let mut args: Vec<String> = vec![
        "-hide_banner".to_string(),
        "-loglevel".to_string(), "error".to_string(),
        "-ss".to_string(), start_time_str,
        "-i".to_string(), input_str,
        "-vf".to_string(), vf_str,
        "-c:v".to_string(), video_codec,
        "-an".to_string(),
        "-pix_fmt".to_string(), "yuv420p".to_string(),
        "-movflags".to_string(), "+faststart".to_string(),
        "-threads".to_string(), "4".to_string(),
        "-t".to_string(), duration_str,
        "-y".to_string(),
        output_str,
    ];

    if encoder.video_codec == "libx264" {
        args.extend(vec!["-preset".to_string(), "ultrafast".to_string(), "-crf".to_string(), "28".to_string()]);
    }

    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_ffmpeg_fast(&args_ref)
}

fn process_dual_mode_optimized(
    left: &PathBuf,
    right: &PathBuf,
    output: &PathBuf,
    output_width: u32,
    output_height: u32,
    duration: f32,
    start_time: f32,
    temp_dir: &PathBuf,
) -> Result<(), String> {
    info!("开始处理双视频模式片段:");
    info!("  左侧视频: {}", left.file_name().unwrap_or_default().to_string_lossy());
    info!("  右侧视频: {}", right.file_name().unwrap_or_default().to_string_lossy());
    info!("  输出视频: {}", output.file_name().unwrap_or_default().to_string_lossy());
    info!("  输出尺寸: {}x{}", output_width, output_height);
    info!("  截取时长: {}秒，开始时间: {}秒", duration, start_time);
    info!("  临时目录: {}", temp_dir.to_string_lossy());
    info!("  使用柔和背景色处理...");
    
    let encoder = detect_best_encoder();
    let encoder_video_codec = encoder.video_codec.clone();

    let left_scaled = temp_dir.join(format!("left_{}.mp4", Uuid::new_v4()));
    let right_scaled = temp_dir.join(format!("right_{}.mp4", Uuid::new_v4()));

    let left_str = left.to_string_lossy().to_string();
    let right_str = right.to_string_lossy().to_string();
    let left_scaled_str = left_scaled.to_string_lossy().to_string();
    let right_scaled_str = right_scaled.to_string_lossy().to_string();
    let duration_str = duration.to_string();
    let start_time_str = start_time.to_string();

    let half_height = output_height * 3 / 4;
    let half_width = output_width / 2;
    
    // 左右视频缩放，使用柔和浅灰色背景填充
    let vf_left = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:color=0xF0F0F0",
        half_width, half_height, half_width, half_height
    );
    let vf_right = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:color=0xF0F0F0",
        half_width, half_height, half_width, half_height
    );

    let left_y = (output_height - half_height) / 2;
    let right_y = left_y;

    let left_handle = {
        let left_str = left_str.clone();
        let left_scaled_str = left_scaled_str.clone();
        let duration_str = duration_str.clone();
        let start_time_str = start_time_str.clone();
        let vf_left = vf_left.clone();
        let video_codec = encoder_video_codec.clone();
        thread::spawn(move || {
            let args: Vec<String> = vec![
                "-hide_banner".to_string(), "-loglevel".to_string(), "error".to_string(),
                "-ss".to_string(), start_time_str,
                "-i".to_string(), left_str,
                "-vf".to_string(), vf_left,
                "-c:v".to_string(), video_codec,
                "-an".to_string(),
                "-t".to_string(), duration_str,
                "-y".to_string(),
                left_scaled_str,
            ];
            let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            run_ffmpeg_fast(&args_ref)
        })
    };

    let right_handle = {
        let right_str = right_str.clone();
        let right_scaled_str = right_scaled_str.clone();
        let duration_str = duration_str.clone();
        let start_time_str = start_time_str.clone();
        let vf_right = vf_right.clone();
        let video_codec = encoder_video_codec.clone();
        thread::spawn(move || {
            let args: Vec<String> = vec![
                "-hide_banner".to_string(), "-loglevel".to_string(), "error".to_string(),
                "-ss".to_string(), start_time_str,
                "-i".to_string(), right_str,
                "-vf".to_string(), vf_right,
                "-c:v".to_string(), video_codec,
                "-an".to_string(),
                "-t".to_string(), duration_str,
                "-y".to_string(),
                right_scaled_str,
            ];
            let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            run_ffmpeg_fast(&args_ref)
        })
    };

    let _ = left_handle.join().map_err(|e| format!("线程错误: {:?}", e))??;
    let _ = right_handle.join().map_err(|e| format!("线程错误: {:?}", e))??;

    let output_str = output.to_string_lossy().to_string();
    
    // 使用 filter_complex 创建柔和浅灰色背景并合成左右视频
    let filter_complex_str = format!(
        "color=c=0xF0F0F0:s={}x{}:d={}[bg];[bg][0:v]overlay=0:{}[bg_left];[bg_left][1:v]overlay={}:{}[final]",
        output_width, output_height, duration, left_y, half_width, right_y
    );

    let mut args: Vec<String> = vec![
        "-hide_banner".to_string(), "-loglevel".to_string(), "error".to_string(),
        "-i".to_string(), left_scaled_str,
        "-i".to_string(), right_scaled_str,
        "-filter_complex".to_string(), filter_complex_str,
        "-map".to_string(), "[final]".to_string(),
        "-c:v".to_string(), encoder.video_codec.clone(),
        "-an".to_string(),
        "-pix_fmt".to_string(), "yuv420p".to_string(),
        "-movflags".to_string(), "+faststart".to_string(),
        "-threads".to_string(), "4".to_string(),
        "-y".to_string(),
        output_str,
    ];
    args.extend(encoder.extra_args.clone());
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_ffmpeg_fast(&args_ref)?;

    let _ = std::fs::remove_file(&left_scaled);
    let _ = std::fs::remove_file(&right_scaled);

    Ok(())
}

fn process_quadrant_mode_optimized(
    tl: &PathBuf,
    tr: &PathBuf,
    bl: &PathBuf,
    br: &PathBuf,
    output: &PathBuf,
    output_width: u32,
    output_height: u32,
    duration: f32,
    start_time: f32,
    temp_dir: &PathBuf,
) -> Result<(), String> {
    info!("开始处理四宫格模式片段:");
    info!("  左上视频: {}", tl.file_name().unwrap_or_default().to_string_lossy());
    info!("  右上视频: {}", tr.file_name().unwrap_or_default().to_string_lossy());
    info!("  左下视频: {}", bl.file_name().unwrap_or_default().to_string_lossy());
    info!("  右下视频: {}", br.file_name().unwrap_or_default().to_string_lossy());
    info!("  输出视频: {}", output.file_name().unwrap_or_default().to_string_lossy());
    info!("  输出尺寸: {}x{}", output_width, output_height);
    info!("  截取时长: {}秒，开始时间: {}秒", duration, start_time);
    info!("  临时目录: {}", temp_dir.to_string_lossy());
    info!("  开始并行处理四个视频...");
    
    let quad_width = output_width / 2;
    let quad_height = output_height / 2;
    let encoder = detect_best_encoder();

    let encoder_video_codec = encoder.video_codec.clone();

    let tl_scaled = temp_dir.join(format!("tl_{}.mp4", Uuid::new_v4()));
    let tr_scaled = temp_dir.join(format!("tr_{}.mp4", Uuid::new_v4()));
    let bl_scaled = temp_dir.join(format!("bl_{}.mp4", Uuid::new_v4()));
    let br_scaled = temp_dir.join(format!("br_{}.mp4", Uuid::new_v4()));

    let tl_str = tl.to_string_lossy().to_string();
    let tr_str = tr.to_string_lossy().to_string();
    let bl_str = bl.to_string_lossy().to_string();
    let br_str = br.to_string_lossy().to_string();

    let tl_scaled_str = tl_scaled.to_string_lossy().to_string();
    let tr_scaled_str = tr_scaled.to_string_lossy().to_string();
    let bl_scaled_str = bl_scaled.to_string_lossy().to_string();
    let br_scaled_str = br_scaled.to_string_lossy().to_string();
    let duration_str = duration.to_string();
    let start_time_str = start_time.to_string();

    let vf_base = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:black",
        quad_width, quad_height, quad_width, quad_height
    );

    let handle1 = {
        let tl_str = tl_str.clone();
        let tl_scaled_str = tl_scaled_str.clone();
        let duration_str = duration_str.clone();
        let start_time_str = start_time_str.clone();
        let vf_base = vf_base.clone();
        let video_codec = encoder_video_codec.clone();
        let extra_args = encoder.extra_args.clone();
        thread::spawn(move || {
            let mut args: Vec<String> = vec![
                "-hide_banner".to_string(), "-loglevel".to_string(), "error".to_string(),
                "-ss".to_string(), start_time_str,
                "-i".to_string(), tl_str,
                "-vf".to_string(), vf_base,
                "-c:v".to_string(), video_codec,
                "-an".to_string(),
                "-t".to_string(), duration_str,
                "-y".to_string(),
                tl_scaled_str,
            ];
            args.extend(extra_args);
            let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            run_ffmpeg_fast(&args_ref)
        })
    };

    let handle2 = {
        let tr_str = tr_str.clone();
        let tr_scaled_str = tr_scaled_str.clone();
        let duration_str = duration_str.clone();
        let start_time_str = start_time_str.clone();
        let vf_base = vf_base.clone();
        let video_codec = encoder_video_codec.clone();
        let extra_args = encoder.extra_args.clone();
        thread::spawn(move || {
            let mut args: Vec<String> = vec![
                "-hide_banner".to_string(), "-loglevel".to_string(), "error".to_string(),
                "-ss".to_string(), start_time_str,
                "-i".to_string(), tr_str,
                "-vf".to_string(), vf_base,
                "-c:v".to_string(), video_codec,
                "-an".to_string(),
                "-t".to_string(), duration_str,
                "-y".to_string(),
                tr_scaled_str,
            ];
            args.extend(extra_args);
            let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            run_ffmpeg_fast(&args_ref)
        })
    };

    let handle3 = {
        let bl_str = bl_str.clone();
        let bl_scaled_str = bl_scaled_str.clone();
        let duration_str = duration_str.clone();
        let start_time_str = start_time_str.clone();
        let vf_base = vf_base.clone();
        let video_codec = encoder_video_codec.clone();
        let extra_args = encoder.extra_args.clone();
        thread::spawn(move || {
            let mut args: Vec<String> = vec![
                "-hide_banner".to_string(), "-loglevel".to_string(), "error".to_string(),
                "-ss".to_string(), start_time_str,
                "-i".to_string(), bl_str,
                "-vf".to_string(), vf_base,
                "-c:v".to_string(), video_codec,
                "-an".to_string(),
                "-t".to_string(), duration_str,
                "-y".to_string(),
                bl_scaled_str,
            ];
            args.extend(extra_args);
            let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            run_ffmpeg_fast(&args_ref)
        })
    };

    let handle4 = {
        let br_str = br_str.clone();
        let br_scaled_str = br_scaled_str.clone();
        let duration_str = duration_str.clone();
        let start_time_str = start_time_str.clone();
        let vf_base = vf_base.clone();
        let video_codec = encoder_video_codec.clone();
        let extra_args = encoder.extra_args.clone();
        thread::spawn(move || {
            let mut args: Vec<String> = vec![
                "-hide_banner".to_string(), "-loglevel".to_string(), "error".to_string(),
                "-ss".to_string(), start_time_str,
                "-i".to_string(), br_str,
                "-vf".to_string(), vf_base,
                "-c:v".to_string(), video_codec,
                "-an".to_string(),
                "-t".to_string(), duration_str,
                "-y".to_string(),
                br_scaled_str,
            ];
            args.extend(extra_args);
            let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            run_ffmpeg_fast(&args_ref)
        })
    };

    let _ = handle1.join().map_err(|e| format!("线程错误: {:?}", e))??;
    let _ = handle2.join().map_err(|e| format!("线程错误: {:?}", e))??;
    let _ = handle3.join().map_err(|e| format!("线程错误: {:?}", e))??;
    let _ = handle4.join().map_err(|e| format!("线程错误: {:?}", e))??;

    let output_str = output.to_string_lossy().to_string();
    let filter_complex_str = format!(
        "[0:v][1:v]hstack[top];[2:v][3:v]hstack[bottom];[top][bottom]vstack[grid]",
    );

    let mut args: Vec<String> = vec![
        "-hide_banner".to_string(), "-loglevel".to_string(), "error".to_string(),
        "-i".to_string(), tl_scaled_str,
        "-i".to_string(), tr_scaled_str,
        "-i".to_string(), bl_scaled_str,
        "-i".to_string(), br_scaled_str,
        "-filter_complex".to_string(), filter_complex_str,
        "-map".to_string(), "[grid]".to_string(),
        "-c:v".to_string(), encoder.video_codec.clone(),
        "-an".to_string(),
        "-pix_fmt".to_string(), "yuv420p".to_string(),
        "-movflags".to_string(), "+faststart".to_string(),
        "-threads".to_string(), "4".to_string(),
        "-t".to_string(), duration_str,
        "-y".to_string(),
        output_str,
    ];
    args.extend(encoder.extra_args.clone());
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_ffmpeg_fast(&args_ref)?;

    for scaled in [&tl_scaled, &tr_scaled, &bl_scaled, &br_scaled] {
        let _ = std::fs::remove_file(scaled);
    }

    Ok(())
}

#[cfg(target_os = "windows")]
fn escape_subtitle_path(path: &str) -> String {
    let escaped = path.replace('\\', "\\\\").replace(':', "\\\\:");
    format!("'{}'", escaped)
}

#[cfg(not(target_os = "windows"))]
fn escape_subtitle_path(path: &str) -> String {
    let escaped = path.replace("'", "'\\''");
    format!("'{}'", escaped)
}

#[cfg(target_os = "windows")]
fn to_ffmpeg_path(path: &PathBuf) -> String {
    let path_str = path.to_string_lossy();
    if path_str.starts_with("\\\\?\\") {
        path_str[4..].replace('\\', "/")
    } else {
        path_str.replace('\\', "/")
    }
}

#[cfg(not(target_os = "windows"))]
fn to_ffmpeg_path(path: &PathBuf) -> String {
    path.to_string_lossy().to_string()
}

fn get_random_transition_type() -> String {
    let transitions = vec![
        "fade".to_string(),
        "dissolve".to_string(),
        "wipeleft".to_string(),
        "wiperight".to_string(),
        "wipeup".to_string(),
        "wipedown".to_string(),
    ];
    
    let index = rand::random::<usize>() % transitions.len();
    transitions[index].clone()
}

fn create_transition_effect(
    video1: &PathBuf,
    video2: &PathBuf,
    output: &PathBuf,
    transition_type: &str,
    duration: f32,
) -> Result<(), String> {
    info!("创建转场效果: type={}, duration={}秒", transition_type, duration);
    
    let video1_str = video1.to_string_lossy().to_string();
    let video2_str = video2.to_string_lossy().to_string();
    let output_str = output.to_string_lossy().to_string();
    let duration_str = duration.to_string();
    
    let half_duration = duration / 2.0;
    let half_duration_str = half_duration.to_string();
    
    let encoder = detect_best_encoder();
    
    let filter_complex = match transition_type {
        "fade" => format!(
            "[0:v]trim=0:{},setpts=PTS-STARTPTS[v0];[1:v]trim={}:,setpts=PTS-STARTPTS+{}/TB,setpts=PTS-STARTPTS[v1];[v0][v1]xfade=transition=fade:duration={}:offset={}[final]",
            half_duration_str, half_duration_str, half_duration_str, duration_str, half_duration_str
        ),
        "dissolve" => format!(
            "[0:v]trim=0:{},setpts=PTS-STARTPTS[v0];[1:v]trim={}:,setpts=PTS-STARTPTS+{}/TB,setpts=PTS-STARTPTS[v1];[v0][v1]xfade=transition=dissolve:duration={}:offset={}[final]",
            half_duration_str, half_duration_str, half_duration_str, duration_str, half_duration_str
        ),
        "wipeleft" => format!(
            "[0:v]trim=0:{},setpts=PTS-STARTPTS[v0];[1:v]trim={}:,setpts=PTS-STARTPTS+{}/TB,setpts=PTS-STARTPTS[v1];[v0][v1]xfade=transition=wipeleft:duration={}:offset={}[final]",
            half_duration_str, half_duration_str, half_duration_str, duration_str, half_duration_str
        ),
        "wiperight" => format!(
            "[0:v]trim=0:{},setpts=PTS-STARTPTS[v0];[1:v]trim={}:,setpts=PTS-STARTPTS+{}/TB,setpts=PTS-STARTPTS[v1];[v0][v1]xfade=transition=wiperight:duration={}:offset={}[final]",
            half_duration_str, half_duration_str, half_duration_str, duration_str, half_duration_str
        ),
        "wipeup" => format!(
            "[0:v]trim=0:{},setpts=PTS-STARTPTS[v0];[1:v]trim={}:,setpts=PTS-STARTPTS+{}/TB,setpts=PTS-STARTPTS[v1];[v0][v1]xfade=transition=wipeup:duration={}:offset={}[final]",
            half_duration_str, half_duration_str, half_duration_str, duration_str, half_duration_str
        ),
        "wipedown" => format!(
            "[0:v]trim=0:{},setpts=PTS-STARTPTS[v0];[1:v]trim={}:,setpts=PTS-STARTPTS+{}/TB,setpts=PTS-STARTPTS[v1];[v0][v1]xfade=transition=wipedown:duration={}:offset={}[final]",
            half_duration_str, half_duration_str, half_duration_str, duration_str, half_duration_str
        ),
        _ => format!(
            "[0:v]trim=0:{},setpts=PTS-STARTPTS[v0];[1:v]trim={}:,setpts=PTS-STARTPTS+{}/TB,setpts=PTS-STARTPTS[v1];[v0][v1]xfade=transition=fade:duration={}:offset={}[final]",
            half_duration_str, half_duration_str, half_duration_str, duration_str, half_duration_str
        ),
    };
    
    let mut args: Vec<String> = vec![
        "-hide_banner".to_string(), "-loglevel".to_string(), "error".to_string(),
        "-i".to_string(), video1_str,
        "-i".to_string(), video2_str,
        "-filter_complex".to_string(), filter_complex,
        "-map".to_string(), "[final]".to_string(),
        "-c:v".to_string(), encoder.video_codec.clone(),
        "-an".to_string(),
        "-pix_fmt".to_string(), "yuv420p".to_string(),
        "-movflags".to_string(), "+faststart".to_string(),
        "-threads".to_string(), "4".to_string(),
        "-y".to_string(),
        output_str,
    ];
    args.extend(encoder.extra_args.clone());
    
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    run_ffmpeg_fast(&args_ref)?;
    
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

    let mut segment_files: Vec<PathBuf> = Vec::new();
    let mut start_time: f32 = 0.0;
    let transition_duration: f32 = 1.0;

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
        
        let processed = process_segment(
            &selected,
            &segment.crop_mode,
            output_width,
            output_height,
            actual_duration,
            start_time,
            &temp_dir,
        )?;

        segment_files.push(processed.output_path);
        start_time += actual_duration;
    }

    if !tutorial_folder.is_empty() {
        let tutorial_videos = get_video_files(tutorial_folder)?;
        if !tutorial_videos.is_empty() {
            if let Some(tutorial) = tutorial_videos.iter().next() {
                let tutorial_scaled = temp_dir.join(format!("tutorial_{}.mp4", Uuid::new_v4()));
                
                let input_str = tutorial.to_string_lossy().to_string();
                let output_str = tutorial_scaled.to_string_lossy().to_string();
                let encoder = detect_best_encoder();

                let vf = format!(
                    "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:color=0xF0F0F0",
                    output_width, output_height, output_width, output_height
                );

                let args: Vec<&str> = vec![
                    "-hide_banner", "-loglevel", "error",
                    "-i", &input_str,
                    "-vf", &vf,
                    "-c:v", &encoder.video_codec,
                    "-an", "-y", &output_str,
                ];

                run_ffmpeg_fast(&args)?;
                segment_files.push(tutorial_scaled);
            }
        }
    }

    let mut transition_segment_files: Vec<PathBuf> = Vec::new();
    
    for i in 0..segment_files.len() {
        transition_segment_files.push(segment_files[i].clone());
        
        if i < segment_files.len() - 1 {
            let transition_type = get_random_transition_type();
            let transition_file = temp_dir.join(format!("transition_{}_{}.mp4", i, Uuid::new_v4()));
            
            create_transition_effect(
                &segment_files[i],
                &segment_files[i + 1],
                &transition_file,
                &transition_type,
                transition_duration,
            )?;
            
            transition_segment_files.push(transition_file);
        }
    }

    let concat_file = temp_dir.join("concat.txt");
    let mut concat_content = String::new();
    for file in &transition_segment_files {
        concat_content.push_str(&format!("file '{}'\n", to_ffmpeg_path(file)));
    }
    fs::write(&concat_file, concat_content).map_err(|e| e.to_string())?;

    let output_path = output_dir.join(output_filename);
    let temp_output_path = if !subtitle_path.is_empty() {
        temp_dir.join(format!("temp_{}", output_filename))
    } else {
        output_path.clone()
    };
    
    let output_str = temp_output_path.to_string_lossy().to_string();
    let concat_str = to_ffmpeg_path(&concat_file);

    let encoder = detect_best_encoder();
    let mut args = vec![
        "-hide_banner",
        "-loglevel", "error",
        "-f", "concat",
        "-safe", "0",
        "-i", &concat_str,
    ];

    if !audio_path.is_empty() {
        args.extend(&["-i", audio_path]);
    }

    if !audio_path.is_empty() {
        args.extend(&["-map", "0:v", "-map", "1:a"]);
    }

    args.extend(&[
        "-c:v", &encoder.video_codec,
        "-c:a", &encoder.audio_codec,
        "-pix_fmt", "yuv420p",
        "-movflags", "+faststart",
        "-threads", "4",
    ]);

    if !audio_path.is_empty() {
        args.extend(&["-shortest"]);
    }

    args.extend(&["-y", &output_str]);
    args.extend(encoder.extra_args.iter().map(|s| s.as_str()));

    let args_ref: Vec<&str> = args.iter().map(|s| s.as_ref()).collect();
    run_ffmpeg_fast(&args_ref)?;

    if !subtitle_path.is_empty() {
        add_subtitles(&temp_output_path, subtitle_path, &output_path)?;
    }

    for file in &segment_files {
        let _ = fs::remove_file(file);
    }
    for file in &transition_segment_files {
        let _ = fs::remove_file(file);
    }
    let _ = fs::remove_file(&concat_file);
    let _ = fs::remove_dir(&temp_dir);

    Ok(())
}

fn add_subtitles(input_path: &PathBuf, subtitle_path: &str, output_path: &PathBuf) -> Result<(), String> {
    info!("添加字幕: subtitle_path={}", subtitle_path);
    
    let input_str = input_path.to_string_lossy().to_string();
    let output_str = output_path.to_string_lossy().to_string();
    
    let subtitle_path_escaped = escape_subtitle_path(subtitle_path);
    
    let encoder = detect_best_encoder();
    let vf = format!("subtitles={}", subtitle_path_escaped);
    
    let mut args = vec![
        "-hide_banner",
        "-loglevel", "error",
        "-i", &input_str,
        "-vf", &vf,
        "-c:v", &encoder.video_codec,
        "-c:a", "copy",
        "-pix_fmt", "yuv420p",
        "-movflags", "+faststart",
        "-threads", "4",
        "-y", &output_str,
    ];
    args.extend(encoder.extra_args.iter().map(|s| s.as_str()));
    
    let args_ref: Vec<&str> = args.iter().map(|s| s.as_ref()).collect();
    run_ffmpeg_fast(&args_ref)?;
    
    Ok(())
}

#[tauri::command]
pub fn create_task(state: tauri::State<AppState>, config_name: String, count: usize) -> Result<Task, String> {
    info!("创建任务: config_name={}, count={}", config_name, count);
    
    let configs = state.configs.read().map_err(|e: std::sync::PoisonError<std::sync::RwLockReadGuard<'_, Vec<config::VideoConfig>>>| e.to_string())?;
    let config = configs.iter().find(|c| c.name == config_name).cloned();
    drop(configs);
    
    let config_id = config.as_ref().map(|c| c.id.clone()).unwrap_or_default();
    let mut task = Task::new(config_name.clone(), count);
    task.config_id = config_id;
    
    let mut tasks = state.tasks.write().map_err(|e: std::sync::PoisonError<std::sync::RwLockWriteGuard<'_, Vec<Task>>>| e.to_string())?;
    tasks.push(task.clone());
    info!("任务创建成功: id={}", task.id);
    
    let configs_clone = state.configs.clone();
    let tasks_clone = state.tasks.clone();
    let task_id = task.id.clone();
    
    thread::spawn(move || {
        info!("开始处理任务: id={}", task_id);
        
        let configs = match configs_clone.read() {
            Ok(c) => c.clone(),
            Err(e) => {
                error!("获取配置失败: {}", e);
                return;
            }
        };
        
        let config = match configs.iter().find(|c| c.name == config_name) {
            Some(c) => c.clone(),
            None => {
                error!("找不到配置: {}", config_name);
                return;
            }
        };
        
        let output_base_dir = if !config.output_folder.is_empty() {
            PathBuf::from(&config.output_folder)
        } else {
            dirs::download_dir().unwrap_or_else(|| std::env::current_dir().unwrap())
        };
        let output_dir = output_base_dir.join(&task_id);
        if let Err(e) = fs::create_dir_all(&output_dir) {
            error!("创建输出目录失败: {}", e);
            return;
        }
        info!("输出目录: {}", output_dir.display());
        
        if let Ok(mut tasks) = tasks_clone.write() {
            if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
                task.status = TaskStatus::Running;
                task.started_at = Some(Utc::now());
                task.current_video = 1;
                task.output_folder = output_dir.to_string_lossy().to_string();
                task.progress_steps = vec![
                    TaskStep { id: "init".to_string(), name: "初始化".to_string(), status: StepStatus::Completed, error: None },
                    TaskStep { id: "video_1".to_string(), name: format!("处理视频 1/{}", count), status: StepStatus::Running, error: None },
                ];
                for i in 2..=count {
                    task.progress_steps.push(TaskStep { 
                        id: format!("video_{}", i), 
                        name: format!("处理视频 {}/{}", i, count), 
                        status: StepStatus::Pending, 
                        error: None 
                    });
                }
                task.progress_steps.push(TaskStep { id: "finish".to_string(), name: "完成".to_string(), status: StepStatus::Pending, error: None });
                info!("任务开始运行: id={}", task_id);
            }
        }
        
        for i in 0..count {
            info!("============================================");
            info!("开始处理第 {}/{} 个视频", i + 1, count);
            info!("============================================");
            
            let step_scan_id = format!("segment_{}_scan", i + 1);
            let step_process_id = format!("segment_{}_process", i + 1);
            let step_merge_id = format!("segment_{}_merge", i + 1);
            let step_subtitle_id = format!("segment_{}_subtitle", i + 1);
            
            if let Ok(mut tasks) = tasks_clone.write() {
                if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
                    task.current_video = i + 1;
                    if let Some(step) = task.progress_steps.iter_mut().find(|s| s.id == format!("video_{}", i + 1)) {
                        step.status = StepStatus::Running;
                    }
                }
            }
            
            // 步骤 1: 扫描视频文件
            info!("  - 步骤 1/4: 扫描视频文件");
            if let Ok(mut tasks) = tasks_clone.write() {
                if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
                    task.progress_steps.push(TaskStep {
                        id: step_scan_id.clone(),
                        name: "步骤 1/4: 扫描视频文件".to_string(),
                        status: StepStatus::Running,
                        error: None,
                    });
                }
            }
            
            // 实际扫描视频
            let scan_result: Result<(), String> = (|| {
                for segment in &config.template_segments {
                    let videos = get_video_files(&segment.source_folder)?;
                    if videos.is_empty() {
                        return Err(format!("片段源文件夹中没有视频文件: {}", segment.source_folder));
                    }
                }
                if !config.tutorial_folder.is_empty() {
                    let _ = get_video_files(&config.tutorial_folder);
                }
                Ok(())
            })();
            
            match scan_result {
                Ok(_) => {
                    if let Ok(mut tasks) = tasks_clone.write() {
                        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
                            if let Some(step) = task.progress_steps.iter_mut().find(|s| s.id == step_scan_id) {
                                step.status = StepStatus::Completed;
                            }
                        }
                    }
                }
                Err(e) => {
                    if let Ok(mut tasks) = tasks_clone.write() {
                        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
                            task.status = TaskStatus::Error;
                            task.error_message = Some(e.clone());
                            if let Some(step) = task.progress_steps.iter_mut().find(|s| s.id == step_scan_id) {
                                step.status = StepStatus::Error;
                                step.error = Some(e.clone());
                            }
                        }
                    }
                    return;
                }
            }
            
            // 步骤 2: 处理片段
            info!("  - 步骤 2/4: 处理片段");
            if let Ok(mut tasks) = tasks_clone.write() {
                if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
                    task.progress_steps.push(TaskStep {
                        id: step_process_id.clone(),
                        name: "步骤 2/4: 处理片段".to_string(),
                        status: StepStatus::Running,
                        error: None,
                    });
                }
            }
            
            let result = process_single_mode(
                &config.template_segments,
                &config.tutorial_folder,
                &config.video_ratio,
                &config.audio_path,
                config.audio_duration,
                &config.subtitle_path,
                &output_dir,
                &format!("{}-{}.mp4", config.name, i + 1),
            );
            
            match result {
                Ok(_) => {
                    // 步骤 3: 合成视频 (process_single_mode 已完成所有工作)
                    if let Ok(mut tasks) = tasks_clone.write() {
                        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
                            if let Some(step) = task.progress_steps.iter_mut().find(|s| s.id == step_process_id) {
                                step.status = StepStatus::Completed;
                            }
                            task.progress_steps.push(TaskStep {
                                id: step_merge_id.clone(),
                                name: "步骤 3/4: 合成视频".to_string(),
                                status: StepStatus::Completed,
                                error: None,
                            });
                            // 如果有字幕，添加字幕步骤
                            if !config.subtitle_path.is_empty() {
                                task.progress_steps.push(TaskStep {
                                    id: step_subtitle_id.clone(),
                                    name: "步骤 4/4: 添加字幕".to_string(),
                                    status: StepStatus::Completed,
                                    error: None,
                                });
                            }
                        }
                    }
                    
                    info!("✅ 第 {} 个视频处理成功！", i + 1);
                    info!("   已保存到: {}", output_dir.display());
                    if let Ok(mut tasks) = tasks_clone.write() {
                        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
                            task.completed_count += 1;
                            if let Some(step) = task.progress_steps.iter_mut().find(|s| s.id == format!("video_{}", i + 1)) {
                                step.status = StepStatus::Completed;
                            }
                            if i + 1 < count {
                                if let Some(next_step) = task.progress_steps.iter_mut().find(|s| s.id == format!("video_{}", i + 2)) {
                                    next_step.status = StepStatus::Running;
                                }
                            }
                        }
                    }
                }
                Err(e) => {
                    error!("❌ 第 {} 个视频处理失败: {}", i + 1, e);
                    error!("   详细错误信息: {}", e);
                    if let Ok(mut tasks) = tasks_clone.write() {
                        if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
                            task.status = TaskStatus::Error;
                            task.error_message = Some(e.clone());
                            if let Some(step) = task.progress_steps.iter_mut().find(|s| s.id == format!("video_{}", i + 1)) {
                                step.status = StepStatus::Error;
                                step.error = Some(e.clone());
                            }
                        }
                    }
                    return;
                }
            }
        }
        
        if let Ok(mut tasks) = tasks_clone.write() {
            if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
                task.status = TaskStatus::Completed;
                task.completed_at = Some(Utc::now());
                if let Some(step) = task.progress_steps.iter_mut().find(|s| s.id == "finish") {
                    step.status = StepStatus::Completed;
                }
                info!("============================================");
                info!("🎉 任务全部完成！");
                info!("============================================");
                info!("任务ID: {}", task_id);
                info!("生成视频数: {}", count);
                info!("输出目录: {}", output_dir.display());
                info!("============================================");
            }
        }
    });
    
    Ok(task)
}

#[tauri::command]
pub fn get_tasks(state: tauri::State<AppState>) -> Result<Vec<Task>, String> {
    let tasks = state.tasks.read().map_err(|e: std::sync::PoisonError<std::sync::RwLockReadGuard<'_, Vec<Task>>>| e.to_string())?;
    Ok(tasks.clone())
}

#[tauri::command]
pub fn get_task_status(state: tauri::State<AppState>, id: String) -> Result<Option<Task>, String> {
    let tasks = state.tasks.read().map_err(|e: std::sync::PoisonError<std::sync::RwLockReadGuard<'_, Vec<Task>>>| e.to_string())?;
    Ok(tasks.iter().find(|t| t.id == id).cloned())
}

#[tauri::command]
pub fn refresh_tasks_from_disk(app: tauri::AppHandle, state: tauri::State<AppState>) -> Result<(), String> {
    let app_data_dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    let data_file = app_data_dir.join("app_data.json");
    
    if data_file.exists() {
        if let Ok(content) = std::fs::read_to_string(&data_file) {
            if let Ok(data) = serde_json::from_str::<super::storage::AppData>(&content) {
                let tasks_count = data.tasks.len();
                if let Ok(mut tasks) = state.tasks.write() {
                    *tasks = data.tasks;
                }
                info!("Refreshed {} tasks from disk", tasks_count);
            }
        }
    }
    Ok(())
}

#[tauri::command]
pub fn pause_task(state: tauri::State<AppState>, id: String) -> Result<(), String> {
    info!("暂停任务: id={}", id);
    let mut tasks = state.tasks.write().map_err(|e: std::sync::PoisonError<std::sync::RwLockWriteGuard<'_, Vec<Task>>>| e.to_string())?;
    if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
        task.status = TaskStatus::Paused;
        info!("任务已暂停: id={}", id);
    }
    Ok(())
}

#[tauri::command]
pub fn resume_task(state: tauri::State<AppState>, id: String) -> Result<(), String> {
    info!("恢复任务: id={}", id);
    let mut tasks = state.tasks.write().map_err(|e: std::sync::PoisonError<std::sync::RwLockWriteGuard<'_, Vec<Task>>>| e.to_string())?;
    if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
        task.status = TaskStatus::Running;
        info!("任务已恢复: id={}", id);
    }
    Ok(())
}

#[tauri::command]
pub fn delete_task(state: tauri::State<AppState>, id: String, delete_videos: bool) -> Result<(), String> {
    info!("删除任务: id={}, delete_videos={}", id, delete_videos);
    
    let mut tasks = state.tasks.write().map_err(|e: std::sync::PoisonError<std::sync::RwLockWriteGuard<'_, Vec<Task>>>| e.to_string())?;
    
    if let Some(task) = tasks.iter().find(|t| t.id == id) {
        if delete_videos && !task.output_folder.is_empty() {
            let output_path = std::path::PathBuf::from(&task.output_folder);
            if output_path.exists() {
                if let Err(e) = std::fs::remove_dir_all(&output_path) {
                    info!("删除输出文件夹失败: {}", e);
                } else {
                    info!("已删除输出文件夹: {}", task.output_folder);
                }
            }
        }
    }
    
    tasks.retain(|t| t.id != id);
    info!("任务已删除: id={}", id);
    Ok(())
}

#[tauri::command]
pub fn open_folder(path: String) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "windows")]
    {
        Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}