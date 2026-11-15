use std::{fs::{self, create_dir_all}};

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn create_library(name: String) -> bool {
    let dir: &str = "../.data";
    let path: String = format!("{}/{}.json", dir, name);

    // TODO: Get safe app dir for saving data.
    if create_dir_all(dir).is_err() {
        return false;
    }
    if fs::write(&path, "{}").is_err() {
        return false;
    }

    true
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![create_library])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
