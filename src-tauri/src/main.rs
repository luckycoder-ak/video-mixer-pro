#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod storage;
mod video_processor;

use log::{error, info};
use std::sync::Mutex;
use tauri::{Manager, State};

pub struct AppState {
    pub configs: Mutex<Vec<config::Config>>,
    pub tasks: Mutex<Vec<video_processor::Task>>,
}

fn main() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("Starting VideoMixer Pro...");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            configs: Mutex::new(Vec::new()),
            tasks: Mutex::new(Vec::new()),
        })
        .setup(|app| {
            info!("Application setup complete");
            let app_data_dir = app.path().app_data_dir().expect("Failed to get app data dir");
            std::fs::create_dir_all(&app_data_dir).expect("Failed to create app data directory");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            config::get_configs,
            config::save_config,
            config::delete_config,
            config::get_config,
            storage::load_data,
            storage::save_data,
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
