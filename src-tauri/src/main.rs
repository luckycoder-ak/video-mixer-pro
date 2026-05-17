#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod storage;
mod video_processor;

use log::info;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tauri::Manager;

pub struct AppState {
    pub configs: Arc<RwLock<Vec<config::VideoConfig>>>,
    pub tasks: Arc<RwLock<Vec<video_processor::Task>>>,
    /// 教程素材按配置隔离的已用集合（持久化，跨任务生效）
    pub used_tutorial_videos: Arc<RwLock<HashMap<String, HashSet<String>>>>,
    /// app_data.json 的绝对路径，供后台线程写入使用
    pub app_data_file: Arc<RwLock<Option<PathBuf>>>,
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("Starting VideoMixer Pro...");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            configs: Arc::new(RwLock::new(Vec::new())),
            tasks: Arc::new(RwLock::new(Vec::new())),
            used_tutorial_videos: Arc::new(RwLock::new(HashMap::new())),
            app_data_file: Arc::new(RwLock::new(None)),
        })
        .setup(|app| {
            info!("Application setup complete");
            let data_file = storage::resolve_app_data_file_path().expect("Failed to resolve app data file path");
            if let Some(parent) = data_file.parent() {
                std::fs::create_dir_all(parent).expect("Failed to create app data directory");
            }
            info!("Using app_data.json at {}", data_file.display());
            let state = app.state::<AppState>();
            if let Ok(mut slot) = state.app_data_file.write() {
                *slot = Some(data_file.clone());
            }
            match storage::load_runtime_store() {
                Ok(runtime) => {
                    let configs_len = runtime.app_data.configs.len();
                    let tasks_len = runtime.app_data.tasks.len();
                    let tutorial_used_len: usize = runtime
                        .used_tutorial_by_config
                        .values()
                        .map(|set| set.len())
                        .sum();
                    if let Ok(mut configs) = state.configs.write() {
                        *configs = runtime.app_data.configs;
                    }
                    if let Ok(mut tasks) = state.tasks.write() {
                        *tasks = runtime.app_data.tasks;
                    }
                    if let Ok(mut used) = state.used_tutorial_videos.write() {
                        *used = runtime.used_tutorial_by_config;
                    }
                    info!(
                        "Loaded {} configs, {} tasks, {} used tutorial videos from app_data_store",
                        configs_len, tasks_len, tutorial_used_len
                    );
                }
                Err(err) => {
                    info!("Failed to load runtime store: {}", err);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            config::get_configs,
            config::save_config,
            config::delete_config,
            config::get_config,
            config::get_audio_duration,
            storage::load_data,
            storage::save_data,
            storage::save_configs,
            storage::get_data_file_path,
            video_processor::create_task,
            video_processor::get_tasks,
            video_processor::refresh_tasks_from_disk,
            video_processor::get_task_status,
            video_processor::get_task,
            video_processor::pause_task,
            video_processor::resume_task,
            video_processor::retry_task,
            video_processor::delete_task,
            video_processor::open_folder,
            video_processor::check_tutorial_available,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
