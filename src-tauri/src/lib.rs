use std::{fs::{self, create_dir_all}, path::{PathBuf}};
use tauri::Manager;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn create_library(app: tauri::AppHandle, name: String) -> bool {

    let app_dir:PathBuf = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(_) => return false,
    };
    

    let data_dir = app_dir.join("data");
    if create_dir_all(&data_dir).is_err() {
        println!("Opps1");
        return false;
    }

    let path: PathBuf = data_dir.join(format!("{}.json", name));
    if fs::write(&path, "{}").is_err() {
        println!("Opps2");
        return false;
    }

    true
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .invoke_handler(tauri::generate_handler![create_library])
        .setup(|app| {
            let app_dir = app.path().app_data_dir().expect("failed to get app data dir");
            let _ = create_dir_all(app_dir);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
