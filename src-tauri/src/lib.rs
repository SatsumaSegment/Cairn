use std::{fs::{self, create_dir_all}, path::{PathBuf, Path}, ffi::OsStr};
use tauri::Manager;

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


fn extract_text(doc: &str) -> Vec<String> {
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


fn embed_text(text: String) -> Vec<f32> {
    vec![]
}



#[tauri::command]
fn process_documents(docs: Vec<String>) -> Vec<(String, Vec<String>)> {
    let mut processed_docs: Vec<(String, Vec<String>)> = vec![];
    for doc in docs {
        let words = extract_text(&doc);

        let mut chunks: Vec<String> = vec![];
        let mut current_chunk: Vec<String> = vec![];

        for word in words {
            if current_chunk.len() >= 200 && current_chunk.len() <= 500 &&
            (word.ends_with('.') ||word.ends_with('!') || word.ends_with('?')) ||
            ( current_chunk.len() >= 500) {
                current_chunk.push(word);
                chunks.push(current_chunk.join(" "));
                current_chunk.drain(..current_chunk.len()-20);
            } else {
                current_chunk.push(word);
            }
        }

        if !current_chunk.is_empty() {
            chunks.push(current_chunk.join(" "));
        }
        processed_docs.push((doc, chunks));
    }
    processed_docs
}




#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(
            tauri::generate_handler![
                create_library, 
                process_documents
            ])
        .setup(|app| {
            let app_dir = app.path().app_data_dir().expect("failed to get app data dir");
            let _ = create_dir_all(app_dir);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
