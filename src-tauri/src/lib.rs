use std::{fs::{self, create_dir_all}, path::{PathBuf, Path}, ffi::OsStr, sync::{OnceLock, Mutex, Arc}};
use fastembed::{TextEmbedding, TextInitOptions, EmbeddingModel};
use lancedb::arrow::arrow_array::{Float32Array, StringArray, RecordBatch, FixedSizeListArray};
use lancedb::arrow::arrow_schema::{Schema, Field, DataType};
use tauri::{Manager};


struct AppState {
    db: lancedb::Connection,
}


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

    if ext.as_deref() == Some("txt") || ext.is_none() {
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


fn get_embedding_model() -> &'static Mutex<TextEmbedding> {
    static MODEL: OnceLock<Mutex<TextEmbedding>> = OnceLock::new();
    
    MODEL.get_or_init(|| {
        let embedding_model = TextEmbedding::try_new(
            TextInitOptions::new(EmbeddingModel::AllMiniLML6V2)
                .with_show_download_progress(true)
        ).expect("Failed to load local embedding model!");
        
        Mutex::new(embedding_model)
    })
}


fn embed_text(processed_docs: Vec<(String, Vec<String>)>) -> Vec<(String, Vec<String>, Vec<Vec<f32>>)> {
    let model_mute = get_embedding_model();
    let mut embedded_docs: Vec<(String, Vec<String>, Vec<Vec<f32>>)> = vec![];

    for (path, chunks) in processed_docs {
        if chunks.is_empty() {
            embedded_docs.push((path, chunks, vec![]));
            continue;
        }
        let mut model = model_mute.lock().expect("Embedding model mutex poisoned");
        let chunk_embeddings = model.embed(chunks.clone(), None).expect("Failed to generate embeddings for document chunks");
        embedded_docs.push((path, chunks, chunk_embeddings));
    }

    embedded_docs
}


async fn store_embeddings(db: &lancedb::Connection, embedded_docs: Vec<(String, Vec<String>, Vec<Vec<f32>>)>) -> Result<(), String> {
    let mut all_paths = Vec::new();
    let mut all_texts = Vec::new();
    let mut all_vectors = Vec::new();

    for (path, texts, embeddings) in embedded_docs {
        for (text, embedding) in texts.into_iter().zip(embeddings.into_iter()) {
            all_paths.push(path.clone());
            all_texts.push(text);
            all_vectors.extend(embedding);
        }
    }

    if all_paths.is_empty() {
        return Ok(());
    }

    let vector_dim = 384;

    let path_array = StringArray::from(all_paths);
    let text_array = StringArray::from(all_texts);
    
    let vector_flat_array = Float32Array::from(all_vectors);
    let vector_array = FixedSizeListArray::try_new(
        Arc::new(Field::new("item", DataType::Float32, true)),
        vector_dim,
        Arc::new(vector_flat_array),
        None,
    ).map_err(|e| e.to_string())?;

    let schema = Arc::new(Schema::new(vec![
        Field::new("path", DataType::Utf8, false),
        Field::new("text", DataType::Utf8, false),
        Field::new(
            "embedding",
            DataType::FixedSizeList(
                Arc::new(Field::new("item", DataType::Float32, true)),
                vector_dim,
            ),
            false,
        ),
    ]));

    let batch = RecordBatch::try_new(
        schema,
        vec![
            Arc::new(path_array),
            Arc::new(text_array),
            Arc::new(vector_array),
        ],
    ).map_err(|e| e.to_string())?;

    match db.open_table("documents").execute().await {
        Ok(existing_table) => {
            existing_table.add(batch).execute().await.map_err(|e| e.to_string())?;
        }
        Err(_) => {
            db.create_table("documents", batch).execute().await.map_err(|e| e.to_string())?;
        }
    }

    Ok(())

}


#[tauri::command]
async fn process_documents(state: tauri::State<'_, AppState>, docs: Vec<String>) -> Result<(), String> {
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
    
    let embedded_docs: Vec<(String, Vec<String>, Vec<Vec<f32>>)> = embed_text(processed_docs);

    if let Err(e) = store_embeddings(&state.db, embedded_docs).await {
        eprintln!("Failed to store embeddings in database: {}", e);
        Err(e)
    } else {
        println!("Successfully saved embeddings to LanceDB!");
        Ok(())
    }

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
            
            let handle = app.handle().clone();

            tauri::async_runtime::block_on(async move {
                let db_path = handle.path().app_data_dir()
                    .expect("Failed to get app data dir")
                    .join("db");

                let db = lancedb::connect(db_path.to_string_lossy().as_ref())
                    .execute()
                    .await
                    .expect("Failed to connect to LanceDB");

                handle.manage(AppState { db });
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
