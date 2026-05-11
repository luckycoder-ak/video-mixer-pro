#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod storage;
mod video_processor;

use log::info;
use std::sync::RwLock;
use tauri::Manager;

pub struct AppState {
    pub configs: std::sync::Arc<std::sync::RwLock<Vec<config::VideoConfig>>>,
    pub tasks: std::sync::Arc<std::sync::RwLock<Vec<video_processor::Task>>>,
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("Starting VideoMixer Pro...");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            configs: std::sync::Arc::new(RwLock::new(Vec::new())),
            tasks: std::sync::Arc::new(RwLock::new(Vec::new())),
        })
        .setup(|app| {
            info!("Application setup complete");
            let app_data_dir = app.path().app_data_dir().expect("Failed to get app data dir");
            std::fs::create_dir_all(&app_data_dir).expect("Failed to create app data directory");
            
            let data_file = app_data_dir.join("app_data.json");
            if data_file.exists() {
                if let Ok(content) = std::fs::read_to_string(&data_file) {
                    if let Ok(data) = serde_json::from_str::<storage::AppData>(&content) {
                        let configs_len = data.configs.len();
                        let tasks_len = data.tasks.len();
                        let state = app.state::<AppState>();
                        if let Ok(mut configs) = state.configs.write() {
                            *configs = data.configs;
                        }
                        if let Ok(mut tasks) = state.tasks.write() {
                            *tasks = data.tasks;
                        }
                        info!("Loaded {} configs and {} tasks from file", configs_len, tasks_len);
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
            storage::load_data,
            storage::save_data,
            storage::save_configs,
            storage::get_data_file_path,
            video_processor::create_task,
            video_processor::get_tasks,
            video_processor::get_task_status,
            video_processor::pause_task,
            video_processor::resume_task,
            video_processor::delete_task,
            video_processor::open_folder,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
