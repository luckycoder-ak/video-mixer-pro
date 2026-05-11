use std::path::PathBuf;
use std::process::Command;
use std::fs;
use std::io::Write;
use std::sync::Mutex;
use rand::Rng;
use serde::{Deserialize, Serialize};
use tauri::Manager;

mod config;
mod storage;

pub use config::*;
pub use storage::*;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoSegment {
    pub video_path: String,
    pub start_time: f32,
    pub duration: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoConfig {
    pub id: String,
    pub name: String,
    pub mode: String,
    pub video_dir: String,
    pub output_dir: String,
    pub subtitle_path: String,
    pub audio_path: String,
    pub audio_volume: f32,
    pub template_segments: Vec<TemplateSegment>,
    pub tutorial_video: Option<TutorialVideo>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateSegment {
    pub id: String,
    pub mode: String,
    pub duration: f32,
    pub video_indices: Vec<usize>,
    pub random_range: Option<RandomRange>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RandomRange {
    pub start: f32,
    pub end: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TutorialVideo {
    pub video_path: String,
    pub duration: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VideoTask {
    pub id: String,
    pub config_id: String,
    pub config_name: String,
    pub status: String,
    pub progress: f32,
    pub output_path: String,
    pub error_message: Option<String>,
    pub created_at: String,
    pub completed_at: Option<String>,
}

static APP_STATE: Mutex<Option<AppState>> = Mutex::new(None);

struct AppState {
    tasks: Vec<VideoTask>,
}

#[derive(Clone)]
struct VideoEncoder {
    video_codec: String,
    audio_codec: String,
    extra_args: Vec<String>,
}

fn detect_best_encoder() -> VideoEncoder {
    let output = Command::new("ffmpeg")
        .args(&["-hide_banner", "-encoders"])
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            
            if stdout.contains("hevc_videotoolbox") {
                info!("检测到硬件编码器: hevc_videotoolbox (macOS)");
                return VideoEncoder {
                    video_codec: "hevc_videotoolbox".to_string(),
                    audio_codec: "aac".to_string(),
                    extra_args: vec!["-tag:v".to_string(), "hvc1".to_string()],
                };
            } else if stdout.contains("h264_videotoolbox") {
                info!("检测到硬件编码器: h264_videotoolbox (macOS)");
                return VideoEncoder {
                    video_codec: "h264_videotoolbox".to_string(),
                    audio_codec: "aac".to_string(),
                    extra_args: vec![],
                };
            } else if stdout.contains("hevc_qsv") {
                info!("检测到硬件编码器: hevc_qsv (Intel QuickSync)");
                return VideoEncoder {
                    video_codec: "hevc_qsv".to_string(),
                    audio_codec: "aac".to_string(),
                    extra_args: vec![],
                };
            } else if stdout.contains("h264_qsv") {
                info!("检测到硬件编码器: h264_qsv (Intel QuickSync)");
                return VideoEncoder {
                    video_codec: "h264_qsv".to_string(),
                    audio_codec: "aac".to_string(),
                    extra_args: vec![],
                };
            } else if stdout.contains("hevc_nvenc") {
                info!("检测到硬件编码器: hevc_nvenc (NVIDIA)");
                return VideoEncoder {
                    video_codec: "hevc_nvenc".to_string(),
                    audio_codec: "aac".to_string(),
                    extra_args: vec![],
                };
            } else if stdout.contains("h264_nvenc") {
                info!("检测到硬件编码器: h264_nvenc (NVIDIA)");
                return VideoEncoder {
                    video_codec: "h264_nvenc".to_string(),
                    audio_codec: "aac".to_string(),
                    extra_args: vec![],
                };
            } else {
                info!("未检测到硬件编码器，使用软件编码器: libx264 (速度较慢，但兼容性好)");
                return VideoEncoder {
                    video_codec: "libx264".to_string(),
                    audio_codec: "aac".to_string(),
                    extra_args: vec!["-preset".to_string(), "fast".to_string()],
                };
            }
        }
        Err(_) => {
            info!("检测编码器失败，使用默认编码器: libx264");
            return VideoEncoder {
                video_codec: "libx264".to_string(),
                audio_codec: "aac".to_string(),
                extra_args: vec!["-preset".to_string(), "fast".to_string()],
            };
        }
    }
}

fn run_ffmpeg(args: &[&str]) -> Result<(), String> {
    info!("执行 FFmpeg: {:?}", args);
    
    let output = Command::new("ffmpeg")
        .args(args)
        .output()
        .map_err(|e| format!("FFmpeg 执行失败: {}", e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!("FFmpeg 处理失败: {}", stderr));
    }

    Ok(())
}

fn run_ffmpeg_fast(args: &[&str]) -> Result<(), String> {
    run_ffmpeg(args)
}

fn get_video_duration_from_file(path: &PathBuf) -> Result<f32, String> {
    let output = Command::new("ffprobe")
        .args(&[
            "-v", "error",
            "-show_entries", "format=duration",
            "-of", "default=noprint_wrappers=1:nokey=1",
            &path.to_string_lossy(),
        ])
        .output()
        .map_err(|e| format!("获取视频时长失败: {}", e))?;

    if !output.status.success() {
        return Err("ffprobe 执行失败".to_string());
    }

    let duration_str = String::from_utf8_lossy(&output.stdout);
    duration_str.trim().parse::<f32>()
        .map_err(|e| format!("解析视频时长失败: {}", e))
}

fn trim_segment(input: &PathBuf, output: &PathBuf, start: f32, end: f32) -> Result<(), String> {
    let input_str = input.to_string_lossy().to_string();
    let output_str = output.to_string_lossy().to_string();
    
    let encoder = detect_best_encoder();
    
    let start_str = start.to_string();
    let end_str = end.to_string();
    
    let args: Vec<&str> = vec![
        "-hide_banner",
        "-loglevel", "error",
        "-i", &input_str,
        "-ss", &start_str,
        "-to", &end_str,
        "-c:v", &encoder.video_codec,
        "-c:a", "aac",
        "-pix_fmt", "yuv420p",
        "-movflags", "+faststart",
        "-threads", "4",
        "-y", &output_str,
    ];
    
    run_ffmpeg_fast(&args)
}

fn get_random_transition_type() -> String {
    let transitions = vec![
        "fade".to_string(),
        "dissolve".to_string(),
        "wipeleft".to_string(),
        "wiperight".to_string(),
        "wipeup".to_string(),
        "wipedown".to_string(),
        "hblur".to_string(),
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
            "[0:v]trim=0:{},setpts=PTS-STARTPTS,settb=AVTB,fps=30[v0];[1:v]trim=0:{},setpts=PTS-STARTPTS,settb=AVTB,fps=30[v1];[v0][v1]xfade=transition=fade:duration={}:offset={}[final]",
            half_duration_str, half_duration_str, duration_str, 0.0
        ),
        "dissolve" => format!(
            "[0:v]trim=0:{},setpts=PTS-STARTPTS,settb=AVTB,fps=30[v0];[1:v]trim=0:{},setpts=PTS-STARTPTS,settb=AVTB,fps=30[v1];[v0][v1]xfade=transition=dissolve:duration={}:offset={}[final]",
            half_duration_str, half_duration_str, duration_str, 0.0
        ),
        "wipeleft" => format!(
            "[0:v]trim=0:{},setpts=PTS-STARTPTS,settb=AVTB,fps=30[v0];[1:v]trim=0:{},setpts=PTS-STARTPTS,settb=AVTB,fps=30[v1];[v0][v1]xfade=transition=wipeleft:duration={}:offset={}[final]",
            half_duration_str, half_duration_str, duration_str, 0.0
        ),
        "wiperight" => format!(
            "[0:v]trim=0:{},setpts=PTS-STARTPTS,settb=AVTB,fps=30[v0];[1:v]trim=0:{},setpts=PTS-STARTPTS,settb=AVTB,fps=30[v1];[v0][v1]xfade=transition=wiperight:duration={}:offset={}[final]",
            half_duration_str, half_duration_str, duration_str, 0.0
        ),
        "wipeup" => format!(
            "[0:v]trim=0:{},setpts=PTS-STARTPTS,settb=AVTB,fps=30[v0];[1:v]trim=0:{},setpts=PTS-STARTPTS,settb=AVTB,fps=30[v1];[v0][v1]xfade=transition=wipeup:duration={}:offset={}[final]",
            half_duration_str, half_duration_str, duration_str, 0.0
        ),
        "wipedown" => format!(
            "[0:v]trim=0:{},setpts=PTS-STARTPTS,settb=AVTB,fps=30[v0];[1:v]trim=0:{},setpts=PTS-STARTPTS,settb=AVTB,fps=30[v1];[v0][v1]xfade=transition=wipedown:duration={}:offset={}[final]",
            half_duration_str, half_duration_str, duration_str, 0.0
        ),
        "hblur" => {
            let blur_val = "10:0".to_string();
            format!(
                "[0:v]trim=0:{},setpts=PTS-STARTPTS,settb=AVTB,fps=30,boxblur={}[v0];[1:v]trim=0:{},setpts=PTS-STARTPTS,settb=AVTB,fps=30,boxblur={}[v1];[v0][v1]xfade=transition=fade:duration={}:offset={}[final]",
                half_duration_str, blur_val, half_duration_str, blur_val, duration_str, 0.0
            )
        },
        _ => format!(
            "[0:v]trim=0:{},setpts=PTS-STARTPTS,settb=AVTB,fps=30[v0];[1:v]trim=0:{},setpts=PTS-STARTPTS,settb=AVTB,fps=30[v1];[v0][v1]xfade=transition=fade:duration={}:offset={}[final]",
            half_duration_str, half_duration_str, duration_str, 0.0
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
    
    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
    run_ffmpeg_fast(&args_refs)
}

fn escape_subtitle_path(path: &str) -> String {
    #[cfg(target_os = "windows")]
    {
        path.replace('\\', "\\\\").replace(':', "\\\\:")
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        let escaped = path.replace('\\', "\\\\").replace(':', "\\:");
        format!("'{}'", escaped.replace("'", "\\'"))
    }
}

fn to_ffmpeg_path(path: &PathBuf) -> String {
    #[cfg(target_os = "windows")]
    {
        path.to_string_lossy().replace("\\", "/")
    }
    
    #[cfg(not(target_os = "windows"))]
    {
        path.to_string_lossy().to_string()
    }
}

fn add_subtitles(input_path: &PathBuf, subtitle_path: &str, output_path: &PathBuf) -> Result<(), String> {
    info!("添加字幕: subtitle_path={}", subtitle_path);
    
    let input_str = input_path.to_string_lossy().to_string();
    let output_str = output_path.to_string_lossy().to_string();
    
    let encoder = detect_best_encoder();
    
    let temp_dir = std::env::temp_dir();
    let temp_subtitle_path = temp_dir.join("temp_subtitle.srt");
    
    let subtitle_to_use = if let Ok(_) = std::fs::copy(subtitle_path, &temp_subtitle_path) {
        info!("已复制字幕文件到临时位置");
        temp_subtitle_path.to_string_lossy().to_string()
    } else {
        subtitle_path.to_string()
    };
    
    let mut succeeded = false;
    
    // Try method 1: Use ass filter
    {
        let filter_str = format!("ass='{}'", subtitle_to_use);
        let mut args: Vec<&str> = vec![
            "-hide_banner",
            "-loglevel", "error",
            "-i", &input_str,
            "-vf", &filter_str,
            "-c:v", &encoder.video_codec,
            "-c:a", "copy",
            "-pix_fmt", "yuv420p",
            "-movflags", "+faststart",
            "-threads", "4",
            "-y", &output_str,
        ];
        
        match run_ffmpeg_fast(&args) {
            Ok(_) => {
                info!("字幕方法1 (ass) 成功");
                succeeded = true;
            }
            Err(e) => {
                info!("字幕方法1 (ass) 失败: {}", e);
            }
        }
    }
    
    // Try method 2: Use subtitles filter with filename= parameter
    if !succeeded {
        let filter_str = format!("subtitles=filename='{}'", subtitle_to_use);
        let mut args: Vec<&str> = vec![
            "-hide_banner",
            "-loglevel", "error",
            "-i", &input_str,
            "-vf", &filter_str,
            "-c:v", &encoder.video_codec,
            "-c:a", "copy",
            "-pix_fmt", "yuv420p",
            "-movflags", "+faststart",
            "-threads", "4",
            "-y", &output_str,
        ];
        
        match run_ffmpeg_fast(&args) {
            Ok(_) => {
                info!("字幕方法2 (subtitles) 成功");
                succeeded = true;
            }
            Err(e) => {
                info!("字幕方法2 (subtitles) 失败: {}", e);
            }
        }
    }
    
    if subtitle_to_use != subtitle_path {
        let _ = std::fs::remove_file(temp_subtitle_path);
    }
    
    if !succeeded {
        info!("字幕处理失败，跳过字幕，保留原始视频");
        fs::copy(input_path, output_path)
            .map_err(|e| format!("无法复制视频文件: {}", e))?;
    }
    
    Ok(())
}

#[tauri::command]
fn create_task(config_id: String, config_name: String, output_dir: String) -> Result<VideoTask, String> {
    info!("创建任务: config_name={}, output_dir={}", config_name, output_dir);

    let task = VideoTask {
        id: uuid::Uuid::new_v4().to_string(),
        config_id,
        config_name,
        status: "waiting".to_string(),
        progress: 0.0,
        output_path: output_dir,
        error_message: None,
        created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
        completed_at: None,
    };

    let mut state = APP_STATE.lock().map_err(|e| e.to_string())?;
    if let Some(ref mut app_state) = *state {
        app_state.tasks.push(task.clone());
    } else {
        *state = Some(AppState {
            tasks: vec![task.clone()],
        });
    }

    info!("任务创建成功: id={}", task.id);
    Ok(task)
}

#[tauri::command]
fn get_tasks() -> Result<Vec<VideoTask>, String> {
    let state = APP_STATE.lock().map_err(|e| e.to_string())?;
    match &*state {
        Some(app_state) => Ok(app_state.tasks.clone()),
        None => Ok(vec![]),
    }
}

#[tauri::command]
fn update_task_status(id: String, status: String, progress: f32, error_message: Option<String>) -> Result<(), String> {
    let mut state = APP_STATE.lock().map_err(|e| e.to_string())?;
    if let Some(ref mut app_state) = *state {
        if let Some(task) = app_state.tasks.iter_mut().find(|t| t.id == id) {
            task.status = status;
            task.progress = progress;
            task.error_message = error_message;
            if task.status == "completed" || task.status == "failed" {
                task.completed_at = Some(chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string());
            }
        }
    }
    Ok(())
}

#[tauri::command]
fn delete_task(id: String) -> Result<(), String> {
    let mut state = APP_STATE.lock().map_err(|e| e.to_string())?;
    if let Some(ref mut app_state) = *state {
        app_state.tasks.retain(|t| t.id != id);
    }
    Ok(())
}

#[tauri::command]
fn delete_task_files(output_path: String) -> Result<(), String> {
    if std::path::Path::new(&output_path).exists() {
        fs::remove_dir_all(&output_path)
            .map_err(|e| format!("删除文件失败: {}", e))?;
    }
    Ok(())
}

#[tauri::command]
fn refresh_tasks_from_disk() -> Result<Vec<VideoTask>, String> {
    let state = APP_STATE.lock().map_err(|e| e.to_string())?;
    match &*state {
        Some(app_state) => Ok(app_state.tasks.clone()),
        None => Ok(vec![]),
    }
}

#[tauri::command]
async fn start_video_task(
    app_handle: tauri::AppHandle,
    task_id: String,
    config: VideoConfig,
    count: usize,
) -> Result<(), String> {
    info!("开始处理任务: id={}", task_id);

    let task = {
        let state = APP_STATE.lock().map_err(|e| e.to_string())?;
        state.as_ref()
            .and_then(|app_state| app_state.tasks.iter().find(|t| t.id == task_id))
            .cloned()
    };

    let task = task.ok_or("任务不存在")?;

    update_task_status(task_id.clone(), "running".to_string(), 0.0, None)?;

    let output_dir = std::path::PathBuf::from(&task.output_path);
    if !output_dir.exists() {
        fs::create_dir_all(&output_dir)
            .map_err(|e| format!("创建输出目录失败: {}", e))?;
    }

    let temp_dir = std::env::temp_dir()
        .join(format!("video_mixer_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("创建临时目录失败: {}", e))?;

    let video_dir = std::path::PathBuf::from(&config.video_dir);
    let subtitle_path = &config.subtitle_path;
    let audio_path = &config.audio_path;
    let audio_volume = config.audio_volume;

    let video_files: Vec<_> = fs::read_dir(&video_dir)
        .map_err(|e| format!("无法读取视频目录: {}", e))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            let path = entry.path();
            if let Some(ext) = path.extension() {
                let ext_lower = ext.to_string_lossy().to_lowercase();
                ext_lower == "mp4" || ext_lower == "mov" || ext_lower == "avi" || ext_lower == "mkv"
            } else {
                false
            }
        })
        .map(|entry| entry.path())
        .collect();

    if video_files.is_empty() {
        return Err("视频目录中没有找到视频文件".to_string());
    }

    info!("找到 {} 个视频文件", video_files.len());

    let mut segment_files: Vec<PathBuf> = Vec::new();
    let mut tutorial_segments: Vec<PathBuf> = Vec::new();

    for (idx, segment) in config.template_segments.iter().enumerate() {
        info!("==========================================");
        info!("处理模板片段 {}/{}", idx + 1, config.template_segments.len());
        info!("==========================================");

        update_task_status(
            task_id.clone(),
            "running".to_string(),
            (idx as f32 / config.template_segments.len() as f32) * 100.0,
            None,
        )?;

        let mut rng = rand::thread_rng();

        match segment.mode.as_str() {
            "quadrant" => {
                info!("开始处理四宫格模式片段:");
                
                if segment.video_indices.len() < 4 {
                    return Err("四宫格模式需要至少4个视频".to_string());
                }

                let mut selected_videos: Vec<&PathBuf> = Vec::new();
                for &idx in &segment.video_indices {
                    if idx < video_files.len() {
                        selected_videos.push(&video_files[idx]);
                    }
                }

                while selected_videos.len() < 4 {
                    let random_idx = rng.gen_range(0..video_files.len());
                    selected_videos.push(&video_files[random_idx]);
                }

                let (width, height) = (1080, 1920);
                let half_width = width / 2;
                let half_height = height / 2;

                let padded_width = width;
                let padded_height = height;

                let scaled_files: Vec<(PathBuf, PathBuf)> = selected_videos.iter().enumerate()
                    .map(|(i, video_path)| {
                        let scaled_file = temp_dir.join(format!("quad_scaled_{}_{}.mp4", idx, i));
                        let input_str = video_path.to_string_lossy().to_string();
                        let output_str = scaled_file.to_string_lossy().to_string();

                        let start = if let Some(ref range) = segment.random_range {
                            rng.gen_range(range.start..range.end)
                        } else {
                            0.0
                        };
                        let duration = segment.duration;

                        info!("  缩放视频 {}: {:?}", i + 1, video_path.file_name().unwrap_or_default());
                        info!("  截取时长: {}秒，开始时间: {}秒", duration, start);

                        let encoder = detect_best_encoder();

                        let vf = format!(
                            "scale={}:{},pad={}:{}:x=(iw-{})/2:y=(ih-{})/2:color=black@0.3",
                            half_width, half_height,
                            half_width, half_height,
                            half_width, half_height
                        );

                        let args: Vec<&str> = vec![
                            "-hide_banner",
                            "-loglevel", "error",
                            "-ss", &start.to_string(),
                            "-i", &input_str,
                            "-t", &duration.to_string(),
                            "-vf", &vf,
                            "-c:v", &encoder.video_codec,
                            "-c:a", "aac",
                            "-pix_fmt", "yuv420p",
                            "-movflags", "+faststart",
                            "-threads", "4",
                            "-y", &output_str,
                        ];

                        run_ffmpeg_fast(&args).map_err(|e| format!("缩放视频失败: {}", e))?;

                        Ok((scaled_file.clone(), scaled_file))
                    })
                    .collect::<Result<Vec<_>, String>>()?;

                let tl_file = &scaled_files[0].1;
                let tr_file = &scaled_files[1].1;
                let bl_file = &scaled_files[2].1;
                let br_file = &scaled_files[3].1;

                let concat_file = temp_dir.join(format!("quad_concat_{}.txt", idx));
                let concat_content = format!(
                    "file '{}'\nfile '{}'\nfile '{}'\nfile '{}'\n",
                    to_ffmpeg_path(tl_file),
                    to_ffmpeg_path(tr_file),
                    to_ffmpeg_path(bl_file),
                    to_ffmpeg_path(br_file)
                );
                fs::write(&concat_file, concat_content).map_err(|e| e.to_string())?;

                let concat_video = temp_dir.join(format!("quad_concat_{}.mp4", idx));
                let encoder = detect_best_encoder();
                let args: Vec<&str> = vec![
                    "-hide_banner",
                    "-loglevel", "error",
                    "-f", "concat",
                    "-safe", "0",
                    "-i", &concat_file.to_string_lossy(),
                    "-c:v", &encoder.video_codec,
                    "-c:a", "aac",
                    "-pix_fmt", "yuv420p",
                    "-movflags", "+faststart",
                    "-threads", "4",
                    "-y", &concat_video.to_string_lossy(),
                ];
                run_ffmpeg_fast(&args).map_err(|e| format!("拼接四宫格失败: {}", e))?;

                let padded_video = temp_dir.join(format!("quad_padded_{}.mp4", idx));
                let vf = format!(
                    "pad={}:{}:x=(iw-{})/2:y=(ih-{})/2:color=0xF0F0F0",
                    padded_width, padded_height,
                    padded_width, padded_height
                );
                let args: Vec<&str> = vec![
                    "-hide_banner",
                    "-loglevel", "error",
                    "-i", &concat_video.to_string_lossy(),
                    "-vf", &vf,
                    "-c:v", &encoder.video_codec,
                    "-c:a", "aac",
                    "-pix_fmt", "yuv420p",
                    "-movflags", "+faststart",
                    "-threads", "4",
                    "-y", &padded_video.to_string_lossy(),
                ];
                run_ffmpeg_fast(&args).map_err(|e| format!("填充四宫格失败: {}", e))?;

                let final_video = temp_dir.join(format!("quad_final_{}.mp4", idx));
                let start = if let Some(ref range) = segment.random_range {
                    rng.gen_range(range.start..range.end)
                } else {
                    0.0
                };
                let duration = segment.duration;

                let args: Vec<&str> = vec![
                    "-hide_banner",
                    "-loglevel", "error",
                    "-ss", &start.to_string(),
                    "-i", &padded_video.to_string_lossy(),
                    "-t", &duration.to_string(),
                    "-c:v", &encoder.video_codec,
                    "-c:a", "aac",
                    "-pix_fmt", "yuv420p",
                    "-movflags", "+faststart",
                    "-threads", "4",
                    "-y", &final_video.to_string_lossy(),
                ];
                run_ffmpeg_fast(&args).map_err(|e| format!("截取四宫格片段失败: {}", e))?;

                segment_files.push(final_video);
            }
            "dual" => {
                info!("开始处理双视频模式片段:");
                
                if segment.video_indices.len() < 2 {
                    return Err("双列模式需要至少2个视频".to_string());
                }

                let left_video_idx = segment.video_indices[0];
                let right_video_idx = if segment.video_indices.len() > 1 {
                    segment.video_indices[1]
                } else {
                    rng.gen_range(0..video_files.len())
                };

                let left_video = &video_files[left_video_idx];
                let right_video = &video_files[right_video_idx];

                let (width, height) = (1080, 1920);
                let half_width = width / 2;
                let padded_height = height;

                let start = if let Some(ref range) = segment.random_range {
                    rng.gen_range(range.start..range.end)
                } else {
                    0.0
                };
                let duration = segment.duration;

                info!("  左侧视频: {:?}", left_video.file_name().unwrap_or_default());
                info!("  右侧视频: {:?}", right_video.file_name().unwrap_or_default());
                info!("  截取时长: {}秒，开始时间: {}秒", duration, start);

                let left_scaled = temp_dir.join(format!("left_scaled_{}.mp4", idx));
                let right_scaled = temp_dir.join(format!("right_scaled_{}.mp4", idx));

                let encoder = detect_best_encoder();

                let left_vf = format!(
                    "scale={}:{},pad={}:{}:x=(iw-{})/2:y=(ih-{})/2:color=0xF0F0F0@0.3",
                    half_width, padded_height,
                    half_width, padded_height,
                    half_width, padded_height
                );

                let args: Vec<&str> = vec![
                    "-hide_banner",
                    "-loglevel", "error",
                    "-ss", &start.to_string(),
                    "-i", &left_video.to_string_lossy(),
                    "-t", &duration.to_string(),
                    "-vf", &left_vf,
                    "-c:v", &encoder.video_codec,
                    "-c:a", "aac",
                    "-pix_fmt", "yuv420p",
                    "-movflags", "+faststart",
                    "-threads", "4",
                    "-y", &left_scaled.to_string_lossy(),
                ];
                run_ffmpeg_fast(&args).map_err(|e| format!("处理左侧视频失败: {}", e))?;

                let right_vf = format!(
                    "scale={}:{},pad={}:{}:x=(iw-{})/2:y=(ih-{})/2:color=0xF0F0F0@0.3",
                    half_width, padded_height,
                    half_width, padded_height,
                    half_width, padded_height
                );

                let args: Vec<&str> = vec![
                    "-hide_banner",
                    "-loglevel", "error",
                    "-ss", &start.to_string(),
                    "-i", &right_video.to_string_lossy(),
                    "-t", &duration.to_string(),
                    "-vf", &right_vf,
                    "-c:v", &encoder.video_codec,
                    "-c:a", "aac",
                    "-pix_fmt", "yuv420p",
                    "-movflags", "+faststart",
                    "-threads", "4",
                    "-y", &right_scaled.to_string_lossy(),
                ];
                run_ffmpeg_fast(&args).map_err(|e| format!("处理右侧视频失败: {}", e))?;

                let concat_file = temp_dir.join(format!("dual_concat_{}.txt", idx));
                let concat_content = format!(
                    "file '{}'\nfile '{}'\n",
                    to_ffmpeg_path(&left_scaled),
                    to_ffmpeg_path(&right_scaled)
                );
                fs::write(&concat_file, concat_content).map_err(|e| e.to_string())?;

                let concat_video = temp_dir.join(format!("dual_concat_{}.mp4", idx));
                let args: Vec<&str> = vec![
                    "-hide_banner",
                    "-loglevel", "error",
                    "-f", "concat",
                    "-safe", "0",
                    "-i", &concat_file.to_string_lossy(),
                    "-c:v", &encoder.video_codec,
                    "-c:a", "aac",
                    "-pix_fmt", "yuv420p",
                    "-movflags", "+faststart",
                    "-threads", "4",
                    "-y", &concat_video.to_string_lossy(),
                ];
                run_ffmpeg_fast(&args).map_err(|e| format!("拼接双列失败: {}", e))?;

                let padded_video = temp_dir.join(format!("dual_padded_{}.mp4", idx));
                let vf = format!(
                    "pad={}:{}:x=(iw-{})/2:y=(ih-{})/2:color=0xF0F0F0",
                    width, padded_height,
                    width, padded_height
                );
                let args: Vec<&str> = vec![
                    "-hide_banner",
                    "-loglevel", "error",
                    "-i", &concat_video.to_string_lossy(),
                    "-vf", &vf,
                    "-c:v", &encoder.video_codec,
                    "-c:a", "aac",
                    "-pix_fmt", "yuv420p",
                    "-movflags", "+faststart",
                    "-threads", "4",
                    "-y", &padded_video.to_string_lossy(),
                ];
                run_ffmpeg_fast(&args).map_err(|e| format!("填充双列失败: {}", e))?;

                segment_files.push(padded_video);
            }
            "single" | _ => {
                info!("开始处理单视频片段:");
                
                let video_idx = if !segment.video_indices.is_empty() {
                    segment.video_indices[0]
                } else {
                    rng.gen_range(0..video_files.len())
                };

                let video_path = &video_files[video_idx];
                let start = if let Some(ref range) = segment.random_range {
                    rng.gen_range(range.start..range.end)
                } else {
                    0.0
                };
                let duration = segment.duration;

                info!("  源视频: {:?}", video_path.file_name().unwrap_or_default());
                info!("  截取时长: {}秒，开始时间: {}秒", duration, start);

                let output_file = temp_dir.join(format!("segment_{}.mp4", idx));

                let encoder = detect_best_encoder();

                let args: Vec<&str> = vec![
                    "-hide_banner",
                    "-loglevel", "error",
                    "-ss", &start.to_string(),
                    "-i", &video_path.to_string_lossy(),
                    "-t", &duration.to_string(),
                    "-c:v", &encoder.video_codec,
                    "-c:a", "aac",
                    "-pix_fmt", "yuv420p",
                    "-movflags", "+faststart",
                    "-threads", "4",
                    "-y", &output_file.to_string_lossy(),
                ];
                run_ffmpeg_fast(&args).map_err(|e| format!("处理单视频片段失败: {}", e))?;

                segment_files.push(output_file);
            }
        }

        if let Some(ref tutorial) = config.tutorial_video {
            info!("处理教程视频片段:");
            
            let tutorial_path = std::path::PathBuf::from(&tutorial.video_path);
            if tutorial_path.exists() {
                let tutorial_scaled = temp_dir.join(format!("tutorial_scaled_{}.mp4", idx));

                let input_str = tutorial_path.to_string_lossy().to_string();
                let output_str = tutorial_scaled.to_string_lossy().to_string();

                let encoder = detect_best_encoder();

                let vf = if let Some(ref range) = segment.random_range {
                    let start_time = rng.gen_range(range.start..range.end);
                    format!("scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2:color=0xF0F0F0")
                } else {
                    "scale=1080:1920:force_original_aspect_ratio=decrease,pad=1080:1920:(ow-iw)/2:(oh-ih)/2:color=0xF0F0F0".to_string()
                };

                let args: Vec<&str> = vec![
                    "-hide_banner", "-loglevel", "error",
                    "-i", &input_str,
                    "-vf", &vf,
                    "-c:v", &encoder.video_codec,
                    "-an", "-y", &output_str,
                ];

                run_ffmpeg_fast(&args)?;
                tutorial_segments.push(tutorial_scaled);
            }
        }
    }

    if !tutorial_segments.is_empty() {
        segment_files.extend(tutorial_segments);
    }

    let transition_duration: f32 = 0.5;
    
    let mut segment_durations: Vec<f32> = Vec::new();
    for segment in &segment_files {
        match get_video_duration_from_file(segment) {
            Ok(duration) => {
                info!("片段时长: {}秒", duration);
                segment_durations.push(duration);
            }
            Err(e) => {
                info!("获取片段时长失败: {}", e);
                segment_durations.push(transition_duration * 2.0);
            }
        }
    }
    
    let mut transition_segment_files: Vec<PathBuf> = Vec::new();
    
    for (i, segment) in segment_files.iter().enumerate() {
        let duration = segment_durations[i];
        
        if i > 0 && i < segment_files.len() - 1 {
            let trimmed_file = temp_dir.join(format!("segment_trim_{}.mp4", i));
            trim_segment(segment, &trimmed_file, transition_duration, duration - transition_duration)?;
            transition_segment_files.push(trimmed_file);
        } else if i == 0 {
            let trimmed_file = temp_dir.join(format!("segment_trim_{}.mp4", i));
            trim_segment(segment, &trimmed_file, 0.0, duration - transition_duration)?;
            transition_segment_files.push(trimmed_file);
        } else {
            let trimmed_file = temp_dir.join(format!("segment_trim_{}.mp4", i));
            trim_segment(segment, &trimmed_file, transition_duration, duration)?;
            transition_segment_files.push(trimmed_file);
        }
        
        if i < segment_files.len() - 1 {
            let transition_type = get_random_transition_type();
            let transition_file = temp_dir.join(format!("transition_{}.mp4", i));
            
            let seg1_end = temp_dir.join(format!("seg1_end_{}.mp4", i));
            let seg2_start = temp_dir.join(format!("seg2_start_{}.mp4", i));
            
            trim_segment(segment, &seg1_end, duration - transition_duration, duration)?;
            trim_segment(&segment_files[i + 1], &seg2_start, 0.0, transition_duration)?;
            
            create_transition_effect(
                &seg1_end,
                &seg2_start,
                &transition_file,
                &transition_type,
                transition_duration,
            )?;
            
            transition_segment_files.push(transition_file);
        }
    }

    info!("转场处理完成，共 {} 个片段和 {} 个转场", 
          segment_files.len(), segment_files.len().saturating_sub(1));

    let concat_file = temp_dir.join("concat.txt");
    let mut concat_content = String::new();
    for file in &transition_segment_files {
        concat_content.push_str(&format!("file '{}'\n", to_ffmpeg_path(file)));
    }
    fs::write(&concat_file, concat_content).map_err(|e| e.to_string())?;

    let output_filename = format!("{}.mp4", config.name);
    let output_path = output_dir.join(&output_filename);
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
    } else {
        args.extend(&["-map", "0:v", "-map", "0:a"]);
    }

    args.extend(&[
        "-c:v", &encoder.video_codec,
        "-c:a", &encoder.audio_codec,
        "-pix_fmt", "yuv420p",
        "-movflags", "+faststart",
        "-threads", "4",
        "-y", &output_str,
    ]);

    if audio_volume != 1.0 && !audio_path.is_empty() {
        args.extend(&["-filter:a", &format!("volume={}", audio_volume)]);
    }

    let args_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect::<Vec<&str>>();
    run_ffmpeg(&args_refs).map_err(|e| format!("合并视频失败: {}", e))?;

    if !subtitle_path.is_empty() {
        let subtitle_output = output_path.clone();
        add_subtitles(&temp_output_path, subtitle_path, &subtitle_output)?;
    }

    let _ = fs::remove_dir_all(&temp_dir);

    update_task_status(
        task_id.clone(),
        "completed".to_string(),
        100.0,
        None,
    )?;

    info!("任务完成!");
    Ok(())
}
