#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod storage;
mod video_processor;

use log::info;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tauri::Manager;

pub struct AppState {
    pub configs: Arc<RwLock<Vec<config::VideoConfig>>>,
    pub tasks: Arc<RwLock<Vec<video_processor::Task>>>,
    /// 教程素材全局已用集合（持久化，跨任务生效）
    pub used_tutorial_videos: Arc<RwLock<HashSet<String>>>,
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
            used_tutorial_videos: Arc::new(RwLock::new(HashSet::new())),
            app_data_file: Arc::new(RwLock::new(None)),
        })
        .setup(|app| {
            info!("Application setup complete");
            let app_data_dir = app.path().app_data_dir().expect("Failed to get app data dir");
            std::fs::create_dir_all(&app_data_dir).expect("Failed to create app data directory");

            let data_file = app_data_dir.join("app_data.json");
            let state = app.state::<AppState>();
            if let Ok(mut slot) = state.app_data_file.write() {
                *slot = Some(data_file.clone());
            }
            if data_file.exists() {
                if let Ok(content) = std::fs::read_to_string(&data_file) {
                    if let Ok(data) = serde_json::from_str::<storage::AppData>(&content) {
                        let configs_len = data.configs.len();
                        let tasks_len = data.tasks.len();
                        let tutorial_used_len = data.used_tutorial_videos.len();
                        if let Ok(mut configs) = state.configs.write() {
                            *configs = data.configs;
                        }
                        if let Ok(mut tasks) = state.tasks.write() {
                            *tasks = data.tasks;
                        }
                        if let Ok(mut used) = state.used_tutorial_videos.write() {
                            *used = data.used_tutorial_videos.into_iter().collect();
                        }
                        info!(
                            "Loaded {} configs, {} tasks, {} used tutorial videos from file",
                            configs_len, tasks_len, tutorial_used_len
                        );
                    }
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
            config::import_config,
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
            video_processor::delete_task,
            video_processor::open_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
