use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};
use std::collections::HashSet;
use std::path::PathBuf;
use std::process::Command;
use std::thread;
use walkdir::WalkDir;
use rand::seq::SliceRandom;
use rand::Rng;
use log::{info, error, warn};

use crate::AppState;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Paused,
    Error,
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
}

impl Task {
    pub fn new(config_name: String, total_count: usize) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            config_id: String::new(),
            config_name: config_name.clone(),
            task_name: format!("{}-{}", config_name, total_count),
            total_count,
            completed_count: 0,
            status: TaskStatus::Pending,
            output_folder: String::new(),
            created_at: Utc::now(),
            started_at: None,
            completed_at: None,
            error_message: None,
        }
    }
}

#[derive(Debug, Clone)]
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
    for entry in WalkDir::new(&path)
        .max_depth(1)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            if let Some(ext) = entry.path().extension() {
                let ext_lower = ext.to_string_lossy().to_lowercase();
                if matches!(ext_lower.as_str(), "mp4" | "avi" | "mov" | "mkv" | "flv" | "wmv" | "webm") {
                    videos.push(entry.path().to_path_buf());
                }
            }
        }
    }

    info!("从文件夹 {} 找到 {} 个视频文件", folder, videos.len());
    Ok(videos)
}

pub fn select_random_videos(videos: &[PathBuf], count: usize, used: &HashSet<String>) -> Result<Vec<PathBuf>, String> {
    if videos.is_empty() {
        return Err("文件夹中没有视频文件".to_string());
    }

    let available: Vec<_> = videos
        .iter()
        .filter(|v| !used.contains(&v.to_string_lossy().to_string()))
        .collect();

    if available.is_empty() {
        warn!("没有可用的视频文件（可能都已使用）");
        return Err("没有可用的视频文件（可能都已使用）".to_string());
    }

    let mut rng = rand::thread_rng();
    let actual_count = count.min(available.len());

    let mut selected = Vec::new();
    let mut shuffled = available.clone();
    shuffled.shuffle(&mut rng);

    for video in shuffled.into_iter().take(actual_count) {
        selected.push(video.clone());
    }

    info!("随机选择了 {} 个视频", selected.len());
    Ok(selected)
}

pub fn calculate_video_dimensions(ratio: &str) -> (u32, u32) {
    match ratio {
        "9:16" => (1080, 1920),
        "16:9" => (1920, 1080),
        "1:1" => (1080, 1080),
        _ => {
            warn!("未知的视频比例: {}, 使用默认值 9:16", ratio);
            (1080, 1920)
        }
    }
}

pub fn calculate_scaled_dimensions(
    source_width: u32,
    source_height: u32,
    target_width: u32,
) -> (u32, u32) {
    let ratio = source_width as f64 / source_height as f64;
    let scaled_height = (target_width as f64 / ratio) as u32;
    (target_width, scaled_height)
}

#[derive(Debug, Clone)]
pub struct ProcessedSegment {
    pub output_path: PathBuf,
    pub duration: f64,
}

pub fn process_segment(
    videos: &[PathBuf],
    crop_mode: &super::config::CropMode,
    duration: u32,
    ratio: &str,
    temp_dir: &PathBuf,
) -> Result<ProcessedSegment, String> {
    let (output_width, output_height) = calculate_video_dimensions(ratio);
    info!("开始处理片段: 模式={:?}, 时长={}秒, 比例={}", crop_mode, duration, ratio);

    match crop_mode {
        super::config::CropMode::Single => {
            if videos.is_empty() {
                error!("单视频模式需要至少1个视频");
                return Err("单视频模式需要至少1个视频".to_string());
            }
            let video = &videos[0];
            let output = temp_dir.join(format!("segment_{}.mp4", Uuid::new_v4()));
            info!("处理单视频模式: {:?}", video);
            process_single_mode_optimized(video, duration, output_width, output_height, &output)?;
            Ok(ProcessedSegment { output_path: output, duration: duration as f64 })
        }

        super::config::CropMode::Dual => {
            if videos.len() < 2 {
                error!("双列模式需要至少2个视频");
                return Err("双列模式需要至少2个视频".to_string());
            }
            let output = temp_dir.join(format!("segment_{}.mp4", Uuid::new_v4()));
            info!("处理双列模式: {:?}, {:?}", videos[0], videos[1]);
            process_dual_mode_optimized(&videos[0], &videos[1], duration, output_width, output_height, &output, temp_dir)?;
            Ok(ProcessedSegment { output_path: output, duration: duration as f64 })
        }

        super::config::CropMode::Quadrant => {
            if videos.len() < 4 {
                error!("四宫格模式需要至少4个视频");
                return Err("四宫格模式需要至少4个视频".to_string());
            }
            let output = temp_dir.join(format!("segment_{}.mp4", Uuid::new_v4()));
            info!("处理四宫格模式");
            process_quadrant_mode_optimized(&videos[0], &videos[1], &videos[2], &videos[3], duration, output_width, output_height, &output, temp_dir)?;
            Ok(ProcessedSegment { output_path: output, duration: duration as f64 })
        }
    }
}

struct EncoderConfig {
    video_codec: String,
    audio_codec: String,
    extra_args: Vec<String>,
}

fn detect_best_encoder() -> EncoderConfig {
    let output = Command::new("ffmpeg")
        .args(["-hide_banner", "-encoders"])
        .output();

    if let Ok(output) = output {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);

        let combined = format!("{}\n{}", stdout, stderr);

        if combined.contains("hevc_nvenc") || combined.contains("h264_nvenc") {
            info!("检测到 NVIDIA GPU 硬件加速");
            return EncoderConfig {
                video_codec: "h264_nvenc".to_string(),
                audio_codec: "aac".to_string(),
                extra_args: vec!["-preset".to_string(), "p4".to_string(), "-tune".to_string(), "hq".to_string()],
            };
        }

        if combined.contains("hevc_qsv") || combined.contains("h264_qsv") {
            info!("检测到 Intel QSV 硬件加速");
            return EncoderConfig {
                video_codec: "h264_qsv".to_string(),
                audio_codec: "aac".to_string(),
                extra_args: vec!["-preset".to_string(), "veryfast".to_string()],
            };
        }

        if combined.contains("hevc_amf") || combined.contains("h264_amf") {
            info!("检测到 AMD GPU 硬件加速");
            return EncoderConfig {
                video_codec: "h264_amf".to_string(),
                audio_codec: "aac".to_string(),
                extra_args: vec!["-quality".to_string(), "balanced".to_string()],
            };
        }
    }

    info!("使用 CPU 软编码 (libx264 + ultrafast)");
    EncoderConfig {
        video_codec: "libx264".to_string(),
        audio_codec: "aac".to_string(),
        extra_args: vec!["-preset".to_string(), "ultrafast".to_string(), "-crf".to_string(), "28".to_string()],
    }
}

fn run_ffmpeg_fast(args: &[&str]) -> Result<(), String> {
    let status = Command::new("ffmpeg")
        .args(args)
        .output()
        .map_err(|e| format!("FFmpeg执行失败: {}", e))?;

    if !status.status.success() {
        let stderr = String::from_utf8_lossy(&status.stderr);
        return Err(format!("FFmpeg错误: {}", stderr));
    }

    Ok(())
}

fn get_quality_blur_filter(target_width: u32, target_height: u32) -> String {
    let blur_w = target_width / 4;
    let blur_h = target_height / 4;
    format!(
        "scale={}:{}:force_original_aspect_ratio=increase,crop={}:{},boxblur=5:3,scale={}:{}:force_original_aspect_ratio=decrease,setsar=1",
        target_width, target_height, blur_w, blur_h, target_width, target_height
    )
}

fn process_single_mode_optimized(
    input: &PathBuf,
    duration: u32,
    output_width: u32,
    output_height: u32,
    output: &PathBuf,
) -> Result<(), String> {
    let input_str = input.to_string_lossy().to_string();
    let output_str = output.to_string_lossy().to_string();
    let encoder = detect_best_encoder();

    info!("单视频模式(优化): {}x{}, 编码器: {}", output_width, output_height, encoder.video_codec);

    let mut args: Vec<String> = vec![
        "-y".to_string(),
        "-ss".to_string(),
        "0".to_string(),
        "-t".to_string(),
        duration.to_string(),
        "-i".to_string(),
        input_str,
    ];

    let vf_str = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,split[s0][s1];[s1]{}[b];[b][s0]overlay=0:0",
        output_width, output_height, get_quality_blur_filter(output_width, output_height)
    );

    args.push("-vf".to_string());
    args.push(vf_str);

    args.push("-c:v".to_string());
    args.push(encoder.video_codec.clone());
    args.extend(encoder.extra_args.clone());

    if encoder.video_codec == "libx264" {
        args.push("-threads".to_string());
        args.push("4".to_string());
    }

    args.push("-c:a".to_string());
    args.push(encoder.audio_codec.clone());
    args.push("-b:a".to_string());
    args.push("96k".to_string());
    args.push("-movflags".to_string());
    args.push("+faststart".to_string());
    args.push(output_str);

    let args_ref: Vec<&str> = args.iter().map(|s| s.as_ref()).collect();
    run_ffmpeg_fast(&args_ref)?;

    info!("单视频片段处理完成: {:?}", output);
    Ok(())
}

fn process_dual_mode_optimized(
    left: &PathBuf,
    right: &PathBuf,
    duration: u32,
    output_width: u32,
    output_height: u32,
    output: &PathBuf,
    temp_dir: &PathBuf,
) -> Result<(), String> {
    let half_width = output_width / 2;
    let half_height = output_height / 2;
    let encoder = detect_best_encoder();

    let encoder_video_codec = encoder.video_codec.clone();
    let encoder_audio_codec = encoder.audio_codec.clone();

    info!("双列模式(优化并行): 编码器={}", encoder_video_codec);

    let left_scaled = temp_dir.join(format!("left_{}.mp4", Uuid::new_v4()));
    let right_scaled = temp_dir.join(format!("right_{}.mp4", Uuid::new_v4()));

    let left_str = left.to_string_lossy().to_string();
    let right_str = right.to_string_lossy().to_string();
    let left_scaled_str = left_scaled.to_string_lossy().to_string();
    let right_scaled_str = right_scaled.to_string_lossy().to_string();
    let duration_str = duration.to_string();

    let blur_filter = get_quality_blur_filter(half_width, half_height);
    let vf_left = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,split[s0][s1];[s1]{}[b];[b][s0]overlay=0:0",
        half_width, half_height, blur_filter
    );

    let left_handle = {
        let left_str = left_str.clone();
        let left_scaled_str = left_scaled_str.clone();
        let duration_str = duration_str.clone();
        let vf_left = vf_left.clone();
        let video_codec = encoder_video_codec.clone();
        let audio_codec = encoder_audio_codec.clone();
        let extra_args = encoder.extra_args.clone();
        thread::spawn(move || {
            let mut args = vec!["-y", "-ss", "0", "-t", &duration_str, "-i", &left_str, "-vf", &vf_left];
            args.extend(["-c:v", &video_codec]);
            args.extend(extra_args.iter().map(|s| s.as_str()).collect::<Vec<_>>());
            if video_codec == "libx264" {
                args.extend(["-threads", "4"]);
            }
            args.extend(["-c:a", &audio_codec, "-b:a", "96k"]);
            args.push(&left_scaled_str);
            let args_ref: Vec<&str> = args.iter().map(|s| *s).collect();
            run_ffmpeg_fast(&args_ref)
        })
    };

    let blur_filter_right = get_quality_blur_filter(half_width, half_height);
    let vf_right = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,split[s0][s1];[s1]{}[b];[b][s0]overlay=0:0",
        half_width, half_height, blur_filter_right
    );

    let right_handle = {
        let right_str = right_str.clone();
        let right_scaled_str = right_scaled_str.clone();
        let duration_str = duration_str.clone();
        let vf_right = vf_right.clone();
        let video_codec = encoder_video_codec.clone();
        let audio_codec = encoder_audio_codec.clone();
        let extra_args = encoder.extra_args.clone();
        thread::spawn(move || {
            let mut args = vec!["-y", "-ss", "0", "-t", &duration_str, "-i", &right_str, "-vf", &vf_right];
            args.extend(["-c:v", &video_codec]);
            args.extend(extra_args.iter().map(|s| s.as_str()).collect::<Vec<_>>());
            if video_codec == "libx264" {
                args.extend(["-threads", "4"]);
            }
            args.extend(["-c:a", &audio_codec, "-b:a", "96k"]);
            args.push(&right_scaled_str);
            let args_ref: Vec<&str> = args.iter().map(|s| *s).collect();
            run_ffmpeg_fast(&args_ref)
        })
    };

    let _ = left_handle.join().map_err(|e| format!("线程错误: {:?}", e))??;
    let _ = right_handle.join().map_err(|e| format!("线程错误: {:?}", e))??;

    let output_str = output.to_string_lossy().to_string();

    run_ffmpeg_fast(&[
        "-y",
        "-i", &left_scaled_str,
        "-i", &right_scaled_str,
        "-filter_complex", "[0:v][1:v]hstack=inputs=2[v]",
        "-map", "[v]",
        "-c:v", &encoder_video_codec,
        "-preset", "ultrafast",
        "-crf", "28",
        "-c:a", &encoder_audio_codec,
        "-b:a", "96k",
        "-movflags", "+faststart",
        "-threads", "4",
        "-t", &duration_str,
        &output_str,
    ])?;

    let _ = std::fs::remove_file(&left_scaled);
    let _ = std::fs::remove_file(&right_scaled);

    info!("双列片段处理完成: {:?}", output);
    Ok(())
}

fn process_quadrant_mode_optimized(
    tl: &PathBuf,
    tr: &PathBuf,
    bl: &PathBuf,
    br: &PathBuf,
    duration: u32,
    output_width: u32,
    output_height: u32,
    output: &PathBuf,
    temp_dir: &PathBuf,
) -> Result<(), String> {
    let quad_width = output_width / 2;
    let quad_height = output_height / 2;
    let encoder = detect_best_encoder();

    let encoder_video_codec = encoder.video_codec.clone();
    let encoder_audio_codec = encoder.audio_codec.clone();

    info!("四宫格模式(优化并行): 编码器={}", encoder_video_codec);

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

    let vf_base = format!(
        "scale={}:{}:force_original_aspect_ratio=decrease,pad={}:{}:(ow-iw)/2:(oh-ih)/2:color=black@0",
        quad_width, quad_height, quad_width, quad_height
    );

    let handle1 = {
        let tl_str = tl_str.clone();
        let tl_scaled_str = tl_scaled_str.clone();
        let duration_str = duration_str.clone();
        let vf_base = vf_base.clone();
        let video_codec = encoder_video_codec.clone();
        let audio_codec = encoder_audio_codec.clone();
        let extra_args = encoder.extra_args.clone();
        thread::spawn(move || {
            let mut args = vec!["-y", "-ss", "0", "-t", &duration_str, "-i", &tl_str, "-vf", &vf_base];
            args.extend(["-c:v", &video_codec]);
            args.extend(extra_args.iter().map(|s| s.as_str()).collect::<Vec<_>>());
            args.extend(["-c:a", &audio_codec, "-b:a", "96k"]);
            args.push(&tl_scaled_str);
            let args_ref: Vec<&str> = args.iter().map(|s| *s).collect();
            run_ffmpeg_fast(&args_ref)
        })
    };

    let handle2 = {
        let tr_str = tr_str.clone();
        let tr_scaled_str = tr_scaled_str.clone();
        let duration_str = duration_str.clone();
        let vf_base = vf_base.clone();
        let video_codec = encoder_video_codec.clone();
        let audio_codec = encoder_audio_codec.clone();
        let extra_args = encoder.extra_args.clone();
        thread::spawn(move || {
            let mut args = vec!["-y", "-ss", "0", "-t", &duration_str, "-i", &tr_str, "-vf", &vf_base];
            args.extend(["-c:v", &video_codec]);
            args.extend(extra_args.iter().map(|s| s.as_str()).collect::<Vec<_>>());
            args.extend(["-c:a", &audio_codec, "-b:a", "96k"]);
            args.push(&tr_scaled_str);
            let args_ref: Vec<&str> = args.iter().map(|s| *s).collect();
            run_ffmpeg_fast(&args_ref)
        })
    };

    let handle3 = {
        let bl_str = bl_str.clone();
        let bl_scaled_str = bl_scaled_str.clone();
        let duration_str = duration_str.clone();
        let vf_base = vf_base.clone();
        let video_codec = encoder_video_codec.clone();
        let audio_codec = encoder_audio_codec.clone();
        let extra_args = encoder.extra_args.clone();
        thread::spawn(move || {
            let mut args = vec!["-y", "-ss", "0", "-t", &duration_str, "-i", &bl_str, "-vf", &vf_base];
            args.extend(["-c:v", &video_codec]);
            args.extend(extra_args.iter().map(|s| s.as_str()).collect::<Vec<_>>());
            args.extend(["-c:a", &audio_codec, "-b:a", "96k"]);
            args.push(&bl_scaled_str);
            let args_ref: Vec<&str> = args.iter().map(|s| *s).collect();
            run_ffmpeg_fast(&args_ref)
        })
    };

    let handle4 = {
        let br_str = br_str.clone();
        let br_scaled_str = br_scaled_str.clone();
        let duration_str = duration_str.clone();
        let vf_base = vf_base.clone();
        let video_codec = encoder_video_codec.clone();
        let audio_codec = encoder_audio_codec.clone();
        let extra_args = encoder.extra_args.clone();
        thread::spawn(move || {
            let mut args = vec!["-y", "-ss", "0", "-t", &duration_str, "-i", &br_str, "-vf", &vf_base];
            args.extend(["-c:v", &video_codec]);
            args.extend(extra_args.iter().map(|s| s.as_str()).collect::<Vec<_>>());
            args.extend(["-c:a", &audio_codec, "-b:a", "96k"]);
            args.push(&br_scaled_str);
            let args_ref: Vec<&str> = args.iter().map(|s| *s).collect();
            run_ffmpeg_fast(&args_ref)
        })
    };

    let _ = handle1.join().map_err(|e| format!("线程错误: {:?}", e))??;
    let _ = handle2.join().map_err(|e| format!("线程错误: {:?}", e))??;
    let _ = handle3.join().map_err(|e| format!("线程错误: {:?}", e))??;
    let _ = handle4.join().map_err(|e| format!("线程错误: {:?}", e))??;

    info!("四宫格视频处理完成，开始合成");

    let output_str = output.to_string_lossy().to_string();

    run_ffmpeg_fast(&[
        "-y",
        "-i", &tl_scaled_str,
        "-i", &tr_scaled_str,
        "-i", &bl_scaled_str,
        "-i", &br_scaled_str,
        "-filter_complex", "[0:v][1:v][2:v][3:v]xstack=inputs=4:layout=0_0|w0_0|0_h0|w0_h0[v]",
        "-map", "[v]",
        "-c:v", &encoder_video_codec,
        "-preset", "ultrafast",
        "-crf", "28",
        "-c:a", &encoder_audio_codec,
        "-b:a", "96k",
        "-movflags", "+faststart",
        "-threads", "4",
        "-t", &duration_str,
        &output_str,
    ])?;

    for scaled in [&tl_scaled, &tr_scaled, &bl_scaled, &br_scaled] {
        let _ = std::fs::remove_file(scaled);
    }

    info!("四宫格片段处理完成: {:?}", output);
    Ok(())
}

#[tauri::command]
pub fn create_task(state: tauri::State<AppState>, config_name: String, count: usize) -> Result<Task, String> {
    info!("创建任务: config_name={}, count={}", config_name, count);
    let task = Task::new(config_name, count);
    let mut tasks = state.tasks.write().map_err(|e: std::sync::PoisonError<std::sync::RwLockWriteGuard<'_, Vec<Task>>>| e.to_string())?;
    tasks.push(task.clone());
    info!("任务创建成功: id={}", task.id);
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
    info!("继续任务: id={}", id);
    let mut tasks = state.tasks.write().map_err(|e: std::sync::PoisonError<std::sync::RwLockWriteGuard<'_, Vec<Task>>>| e.to_string())?;
    if let Some(task) = tasks.iter_mut().find(|t| t.id == id) {
        task.status = TaskStatus::Running;
        info!("任务已继续: id={}", id);
    }
    Ok(())
}

#[tauri::command]
pub fn delete_task(state: tauri::State<AppState>, id: String) -> Result<(), String> {
    info!("删除任务: id={}", id);
    let mut tasks = state.tasks.write().map_err(|e: std::sync::PoisonError<std::sync::RwLockWriteGuard<'_, Vec<Task>>>| e.to_string())?;
    tasks.retain(|t| t.id != id);
    info!("任务已删除: id={}", id);
    Ok(())
}

#[tauri::command]
pub fn open_folder(path: String) -> Result<(), String> {
    info!("打开文件夹: {}", path);

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}
