use std::{fs::{self, create_dir_all}, path::{PathBuf}};
use std::ffi::OsStr;
use tauri::Manager;
use std::path::Path;

#[tauri::command]
fn create_library(app: tauri::AppHandle, name: String) -> bool {
    let app_dir:PathBuf = match app.path().app_data_dir() {
        Ok(dir) => dir,
        Err(_) => return false,
    };

    let data_dir = app_dir.join("data");
    if create_dir_all(&data_dir).is_err() {
        return false;
    }

    let path: PathBuf = data_dir.join(format!("{}.json", name));
    if fs::write(&path, "{}").is_err() {
        return false;
    }

    true
}


fn extract_text(doc: String) -> Vec<String> {
    let ext = Path::new(&doc)
        .extension()
        .and_then(OsStr::to_str)
        .map(|ext| ext.to_lowercase());

    let mut results = Vec::new();

    if ext.as_deref() == Some("txt") {
        match fs::read_to_string(doc) {
            Ok(content) => {
                results = content
                    .split_whitespace()
                    .map(|s| s.to_string())
                    .collect();
            }
            Err(e) => {
                eprintln!("Failed to read text file: {}", e);
            }
        }
    }

    results
}


fn process_documents(docs: Vec<String>) {
    for doc in docs {
        let words = extract_text(doc);

        let mut chunks: Vec<String>;
        let mut current_chunk: Vec<String>;
        let mut n: i32 = 0;
        for word in words {

        }
    }
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
