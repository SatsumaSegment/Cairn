use std::{fs::{self, create_dir_all}, path::{PathBuf, Path}, ffi::OsStr, str::FromStr, sync::{OnceLock, Mutex, Arc}};
use std::collections::BTreeSet;
use fastembed::{TextEmbedding, TextInitOptions, EmbeddingModel};
use lancedb::arrow::arrow_array::{Float32Array, StringArray, RecordBatch, FixedSizeListArray};
use lancedb::arrow::arrow_schema::{Schema, Field, DataType};
use lancedb::query::{QueryBase, ExecutableQuery, Select};
use tauri::{Manager, ipc::Channel};
use ollama_rs::{generation::completion::request::GenerationRequest, Ollama};


struct AppState {
    db: lancedb::Connection,
    settings: Mutex<AppSettings>,
    settings_path: PathBuf,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct AppSettings {
    embedding_model_name: String,
    vector_dimensions: usize,
    llm_model_name: String,
    min_chunk_size: usize,
    max_chunk_size: usize,
    sentence_enders: bool,
    max_results: usize,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            embedding_model_name: "AllMiniLML6V2".to_string(),
            vector_dimensions: 384,
            llm_model_name: "phi4-mini:latest".to_string(),
            min_chunk_size: 200,
            max_chunk_size: 500,
            sentence_enders: true,
            max_results: 5,
        }
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct EmbeddingModelOption {
    name: String,
    dimensions: usize,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SettingsResponse {
    settings: AppSettings,
    embedding_models: Vec<EmbeddingModelOption>,
}


#[derive(serde::Serialize, serde::Deserialize, Debug)]
struct SearchResult {
    path: String,
    text: String,
    distance: f32
}


#[derive(serde::Serialize, Clone)]
pub struct ChatChunk {
    pub text: String,
    pub done: bool
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


struct CachedEmbeddingModel {
    name: String,
    model: TextEmbedding,
}

fn with_embedding_model<T>(
    model_name: &str,
    operation: impl FnOnce(&mut TextEmbedding) -> Result<T, String>,
) -> Result<T, String> {
    static MODEL: OnceLock<Mutex<Option<CachedEmbeddingModel>>> = OnceLock::new();
    let mut cached_model = MODEL
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|error| format!("Embedding model mutex poisoned: {error}"))?;

    if cached_model.as_ref().map(|cached| cached.name.as_str()) != Some(model_name) {
        let embedding_model = EmbeddingModel::from_str(model_name)
            .map_err(|error| format!("Invalid embedding model: {error}"))?;
        let model = TextEmbedding::try_new(
            TextInitOptions::new(embedding_model).with_show_download_progress(true)
        )
        .map_err(|error| format!("Failed to load embedding model: {error}"))?;
        *cached_model = Some(CachedEmbeddingModel {
            name: model_name.to_string(),
            model,
        });
    }

    let cached = cached_model
        .as_mut()
        .ok_or_else(|| "Embedding model was not initialized".to_string())?;
    operation(&mut cached.model)
}

fn validate_settings(settings: &AppSettings) -> Result<(), String> {
    let model = EmbeddingModel::from_str(&settings.embedding_model_name)
        .map_err(|error| format!("Invalid embedding model: {error}"))?;
    let model_dimensions = TextEmbedding::get_model_info(&model)
        .map_err(|error| error.to_string())?
        .dim;

    if settings.vector_dimensions != model_dimensions {
        return Err(format!(
            "The selected embedding model produces {model_dimensions} dimensions."
        ));
    }
    if settings.llm_model_name.trim().is_empty() {
        return Err("LLM model name cannot be empty.".to_string());
    }
    if settings.min_chunk_size == 0 || settings.max_chunk_size < settings.min_chunk_size {
        return Err("Chunk sizes must be positive, and max chunk size must be at least the min chunk size.".to_string());
    }
    if !(1..=100).contains(&settings.max_results) {
        return Err("Max results must be between 1 and 100.".to_string());
    }

    Ok(())
}

fn current_settings(state: &AppState) -> Result<AppSettings, String> {
    state
        .settings
        .lock()
        .map(|settings| settings.clone())
        .map_err(|error| format!("Settings mutex poisoned: {error}"))
}

fn load_settings(path: &Path) -> Result<AppSettings, String> {
    let contents = match fs::read_to_string(path) {
        Ok(contents) => contents,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(AppSettings::default());
        }
        Err(error) => return Err(format!("Failed to read settings: {error}")),
    };
    let settings: AppSettings = serde_json::from_str(&contents)
        .map_err(|error| format!("Failed to parse settings: {error}"))?;
    validate_settings(&settings)?;
    Ok(settings)
}


fn embed_text(
    processed_docs: Vec<(String, Vec<String>)>,
    model_name: &str,
) -> Result<Vec<(String, Vec<String>, Vec<Vec<f32>>)>, String> {
    let mut embedded_docs: Vec<(String, Vec<String>, Vec<Vec<f32>>)> = vec![];

    let batch_size = 32;

    let mut n_batches = 1;

    for (path, chunks) in processed_docs {
        if chunks.is_empty() {
            embedded_docs.push((path, chunks, vec![]));
            continue;
        }

        let mut all_chunk_embeddings = Vec::new();

        for chunk_batch in chunks.chunks(batch_size) {
            println!("Embedding batch #{}", n_batches);
            let batch_embeddings = with_embedding_model(model_name, |model| {
                model.embed(chunk_batch.to_vec(), None).map_err(|error| error.to_string())
            })?;
            
            all_chunk_embeddings.extend(batch_embeddings);
            n_batches += 1;
        }

        embedded_docs.push((path, chunks, all_chunk_embeddings));
    }

    Ok(embedded_docs)
}

async fn store_embeddings(
    db: &lancedb::Connection,
    library: String,
    embedded_docs: Vec<(String, Vec<String>, Vec<Vec<f32>>)>,
    vector_dim: usize,
) -> Result<(), String> {
    let mut all_libraries = Vec::new();
    let mut all_paths = Vec::new();
    let mut all_texts = Vec::new();
    let mut all_vectors = Vec::new();

    for (path, texts, embeddings) in embedded_docs {
        for (text, embedding) in texts.into_iter().zip(embeddings.into_iter()) {
            if embedding.len() != vector_dim {
                return Err(format!(
                    "Embedding has {} dimensions, but settings specify {vector_dim}.",
                    embedding.len()
                ));
            }
            all_libraries.push(library.clone());
            all_paths.push(path.clone());
            all_texts.push(text);
            all_vectors.extend(embedding);
        }
    }

    if all_paths.is_empty() {
        return Ok(());
    }

    let vector_dim = i32::try_from(vector_dim)
        .map_err(|error| format!("Invalid vector dimension: {error}"))?;

    let library_array = StringArray::from(all_libraries);
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
        Field::new("library", DataType::Utf8, false),
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
            Arc::new(library_array),
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

fn chunk_words(words: Vec<String>, settings: &AppSettings) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current_chunk = Vec::new();
    let mut has_new_content = false;
    let overlap_size = (settings.max_chunk_size / 10).min(20);

    for word in words {
        let is_sentence_end = word.ends_with('.') || word.ends_with('!') || word.ends_with('?');
        current_chunk.push(word);
        has_new_content = true;

        if current_chunk.len() >= settings.max_chunk_size
            || (settings.sentence_enders
                && current_chunk.len() >= settings.min_chunk_size
                && is_sentence_end)
        {
            chunks.push(current_chunk.join(" "));
            let overlap_start = current_chunk.len().saturating_sub(overlap_size);
            current_chunk = current_chunk
                .into_iter()
                .skip(overlap_start)
                .collect();
            has_new_content = false;
        }
    }

    if has_new_content {
        chunks.push(current_chunk.join(" "));
    }

    chunks
}

#[tauri::command]
async fn process_documents(state: tauri::State<'_, AppState>, library: String, docs: Vec<String>) -> Result<(), String> {
    let settings = current_settings(&state)?;
    let mut processed_docs: Vec<(String, Vec<String>)> = vec![];
    for doc in docs {
        println!("Processing document: {}", &doc);

        let words = extract_text(&doc);
        processed_docs.push((doc, chunk_words(words, &settings)));
    }
    
    let embedded_docs = embed_text(processed_docs, &settings.embedding_model_name)?;

    println!("Storing embeddings...");
    if let Err(e) = store_embeddings(
        &state.db,
        library,
        embedded_docs,
        settings.vector_dimensions,
    )
    .await
    {
        eprintln!("Failed to store embeddings in database: {}", e);
        Err(e)
    } else {
        println!("Success!");
        Ok(())
    }

}

fn document_filter(library: &str, path: Option<&str>) -> String {
    let library = library.replace('\'', "''");
    match path {
        Some(path) => format!("library = '{library}' AND path = '{}'", path.replace('\'', "''")),
        None => format!("library = '{library}'"),
    }
}

#[tauri::command]
async fn list_library_documents(
    state: tauri::State<'_, AppState>,
    library: String,
) -> Result<Vec<String>, String> {
    let tables = state
        .db
        .table_names()
        .execute()
        .await
        .map_err(|error| format!("Failed to list database tables: {error}"))?;
    if !tables.iter().any(|name| name == "documents") {
        return Ok(Vec::new());
    }

    let table = state
        .db
        .open_table("documents")
        .execute()
        .await
        .map_err(|error| format!("Failed to open documents table: {error}"))?;
    let mut stream = table
        .query()
        .only_if(document_filter(&library, None))
        .select(Select::Columns(vec!["path".to_string()]))
        .execute()
        .await
        .map_err(|error| format!("Failed to list library documents: {error}"))?;
    let mut paths = BTreeSet::new();

    while let Some(batch_result) = futures::StreamExt::next(&mut stream).await {
        let batch = batch_result.map_err(|error| error.to_string())?;
        let column = batch
            .column_by_name("path")
            .ok_or_else(|| "Missing path column in documents table".to_string())?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| "Failed to read document paths from database".to_string())?;
        paths.extend((0..batch.num_rows()).map(|index| column.value(index).to_string()));
    }

    Ok(paths.into_iter().collect())
}

#[tauri::command]
async fn remove_library_document(
    state: tauri::State<'_, AppState>,
    library: String,
    path: String,
) -> Result<(), String> {
    if path.is_empty() {
        return Err("Document path cannot be empty.".to_string());
    }

    let table = state
        .db
        .open_table("documents")
        .execute()
        .await
        .map_err(|error| format!("Failed to open documents table: {error}"))?;
    let predicate = document_filter(&library, Some(&path));
    table
        .delete(&predicate)
        .await
        .map_err(|error| format!("Failed to remove document from library: {error}"))?;

    Ok(())
}

#[tauri::command]
fn get_settings(state: tauri::State<'_, AppState>) -> Result<SettingsResponse, String> {
    let settings = current_settings(&state)?;
    let mut embedding_models = TextEmbedding::list_supported_models()
        .into_iter()
        .map(|model| EmbeddingModelOption {
            name: format!("{:?}", model.model),
            dimensions: model.dim,
        })
        .collect::<Vec<_>>();
    embedding_models.sort_by(|left, right| left.name.cmp(&right.name));

    Ok(SettingsResponse {
        settings,
        embedding_models,
    })
}

#[tauri::command]
fn save_settings(state: tauri::State<'_, AppState>, settings: AppSettings) -> Result<(), String> {
    validate_settings(&settings)?;
    let serialized = serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
    let mut active_settings = state
        .settings
        .lock()
        .map_err(|error| format!("Settings mutex poisoned: {error}"))?;

    fs::write(&state.settings_path, serialized)
        .map_err(|error| format!("Failed to save settings: {error}"))?;
    *active_settings = settings;

    Ok(())
}

#[tauri::command]
async fn chat_with_documents(
    state: tauri::State<'_, AppState>,
    library: String,
    user_query: String,
    on_chunk: Channel<ChatChunk>,
) -> Result<(), String> {
    let settings = current_settings(&state)?;
    let started_at = std::time::Instant::now();
    println!(
        "[chat_with_documents] Started: library={:?}, query_chars={}, limit={}",
        library,
        user_query.chars().count(),
        settings.max_results
    );

    println!("[chat_with_documents] Generating query embedding");
    let query_vector = with_embedding_model(&settings.embedding_model_name, |model| {
        let embeddings = model
            .embed(vec![user_query.clone()], None)
            .map_err(|error| error.to_string())?;
        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| "Failed to generate embedding for query".to_string())
    })?;
    if query_vector.len() != settings.vector_dimensions {
        return Err(format!(
            "Query embedding has {} dimensions, but settings specify {}.",
            query_vector.len(),
            settings.vector_dimensions
        ));
    }
    println!(
        "[chat_with_documents] Query embedding generated in {:?}",
        started_at.elapsed()
    );

    println!("[chat_with_documents] Opening documents table");
    let table = state
        .db
        .open_table("documents")
        .execute()
        .await
        .map_err(|e| {
            let error = e.to_string();
            eprintln!("[chat_with_documents] Failed to open documents table: {error}");
            error
        })?;

    let escaped_library = library.replace('\'', "''");
    let mut stream = table
        .vector_search(query_vector)
        .map_err(|e| {
            let error = e.to_string();
            eprintln!("[chat_with_documents] Failed to create vector search: {error}");
            error
        })?
        .only_if(format!("library = '{escaped_library}'"))
        .limit(settings.max_results)
        .execute()
        .await
        .map_err(|e| {
            let error = e.to_string();
            eprintln!("[chat_with_documents] Vector search failed: {error}");
            error
        })?;

    let mut context_text = String::new();
    let mut result_count = 0;

    while let Some(batch_result) = futures::StreamExt::next(&mut stream).await {
        let batch = batch_result.map_err(|e| {
            let error = e.to_string();
            eprintln!("[chat_with_documents] Failed to read search results: {error}");
            error
        })?;

        let texts = batch
            .column_by_name("text")
            .ok_or_else(|| {
                let error = "Missing text column".to_string();
                eprintln!("[chat_with_documents] {error}");
                error
            })?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| {
                let error = "Failed to downcast text column".to_string();
                eprintln!("[chat_with_documents] {error}");
                error
            })?;

        let paths = batch
            .column_by_name("path")
            .ok_or_else(|| {
                let error = "Missing path column".to_string();
                eprintln!("[chat_with_documents] {error}");
                error
            })?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or_else(|| {
                let error = "Failed to downcast path column".to_string();
                eprintln!("[chat_with_documents] {error}");
                error
            })?;

        for i in 0..batch.num_rows() {
            context_text.push_str(&format!(
                "Source [{}]:\n{}\n\n",
                paths.value(i),
                texts.value(i)
            ));
            result_count += 1;
        }
    }
    println!(
        "[chat_with_documents] Retrieved {} chunks ({} context chars) in {:?}",
        result_count,
        context_text.chars().count(),
        started_at.elapsed()
    );
    if result_count == 0 {
        eprintln!(
            "[chat_with_documents] No matching chunks found for library={:?}; continuing with empty context",
            library
        );
    }


    let prompt = format!(
        "You are a helpful assistant. Use the following retrieved context documents to answer the user's question accurately.\n\n\
        === CONTEXT ===\n\
        {context_text}\n\
        === USER QUESTION ===\n\
        {user_query}\n\n\
        Answer:"
    );


    let ollama = Ollama::default();
    let model = settings.llm_model_name;
    println!(
        "[chat_with_documents] Starting Ollama stream: model={}, prompt_chars={}",
        model,
        prompt.chars().count()
    );

    let mut llm_stream = ollama
        .generate_stream(GenerationRequest::new(model, prompt))
        .await
        .map_err(|e| {
            let error = e.to_string();
            eprintln!("[chat_with_documents] Failed to start Ollama generation: {error}");
            error
        })?;
    println!(
        "[chat_with_documents] Ollama stream started after {:?}",
        started_at.elapsed()
    );

    let mut chunk_count = 0;
    let mut response_chars = 0;
    while let Some(res) = tokio_stream::StreamExt::next(&mut llm_stream).await {
        let responses = res.map_err(|e| {
            let error = e.to_string();
            eprintln!("[chat_with_documents] Ollama stream failed: {error}");
            error
        })?;
        for resp in responses {
            chunk_count += 1;
            response_chars += resp.response.chars().count();
            if chunk_count == 1 {
                println!("[chat_with_documents] Received first Ollama chunk");
            }
            on_chunk.send(ChatChunk {
                text: resp.response.clone(),
                done: false,
            }).map_err(|e| {
                let error = e.to_string();
                eprintln!("[chat_with_documents] Failed to send response chunk to frontend: {error}");
                error
            })?;
        }
    }

    println!(
        "[chat_with_documents] Ollama stream ended: chunks={}, response_chars={}, elapsed={:?}",
        chunk_count,
        response_chars,
        started_at.elapsed()
    );
    on_chunk.send(ChatChunk {
        text: "".to_string(),
        done: true,
    }).map_err(|e| {
        let error = e.to_string();
        eprintln!("[chat_with_documents] Failed to send completion signal to frontend: {error}");
        error
    })?;
    println!("[chat_with_documents] Completed successfully");

    Ok(())
}

#[tauri::command]
async fn search_documents(
    state: tauri::State<'_, AppState>,
    library: String,
    query_text: String,
) -> Result<Vec<SearchResult>, String> {
    let settings = current_settings(&state)?;
    let query_vector = with_embedding_model(&settings.embedding_model_name, |model| {
        let embeddings = model.embed(vec![query_text], None).map_err(|error| error.to_string())?;
        embeddings
            .into_iter()
            .next()
            .ok_or_else(|| "Failed to generate embedding for query".to_string())
    })?;
    if query_vector.len() != settings.vector_dimensions {
        return Err(format!(
            "Query embedding has {} dimensions, but settings specify {}.",
            query_vector.len(),
            settings.vector_dimensions
        ));
    }

    let table = state.db.open_table("documents").execute().await.map_err(|e| e.to_string())?;

    let escaped_library = library.replace('\'', "''");
    let mut stream = table
        .vector_search(query_vector)
        .map_err(|e| e.to_string())?
        .only_if(format!("library = '{escaped_library}'"))
        .limit(settings.max_results)
        .execute()
        .await
        .map_err(|e| e.to_string())?;

    let mut search_results = Vec::new();

    while let Some(batch_result) = futures::StreamExt::next(&mut stream).await {
        let batch = batch_result.map_err(|e| e.to_string())?;

        let paths = batch
            .column_by_name("path")
            .ok_or("Missing path column")?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or("Failed to downcast path column")?;

        let texts = batch
            .column_by_name("text")
            .ok_or("Missing text column")?
            .as_any()
            .downcast_ref::<StringArray>()
            .ok_or("Failed to downcast text column")?;

        let distances = batch
            .column_by_name("_distance")
            .ok_or("Missing _distance column")?
            .as_any()
            .downcast_ref::<Float32Array>()
            .ok_or("Failed to downcast _distance column")?;

        for i in 0..batch.num_rows() {
            search_results.push(SearchResult {
                path: paths.value(i).to_string(),
                text: texts.value(i).to_string(),
                distance: distances.value(i),
            });
        }
    }

    Ok(search_results)
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
                process_documents,
                list_library_documents,
                remove_library_document,
                search_documents,
                chat_with_documents,
                get_settings,
                save_settings
            ])
        .setup(|app| {
            let app_dir = app.path().app_data_dir()?;
            create_dir_all(&app_dir)?;
            let settings_path = app_dir.join("settings.json");
            let settings = load_settings(&settings_path).map_err(std::io::Error::other)?;
            
            let handle = app.handle().clone();

            tauri::async_runtime::block_on(async move {
                let db_path = handle.path().app_data_dir()
                    .expect("Failed to get app data dir")
                    .join("db");

                let db = lancedb::connect(db_path.to_string_lossy().as_ref())
                    .execute()
                    .await
                    .expect("Failed to connect to LanceDB");

                handle.manage(AppState {
                    db,
                    settings: Mutex::new(settings),
                    settings_path,
                });
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_words_splits_at_sentence_end_after_minimum() {
        let settings = AppSettings {
            min_chunk_size: 2,
            max_chunk_size: 5,
            ..AppSettings::default()
        };
        let words = ["One", "two.", "Three", "four"]
            .into_iter()
            .map(str::to_string)
            .collect();

        assert_eq!(
            chunk_words(words, &settings),
            vec!["One two.", "Three four"]
        );
    }

    #[test]
    fn chunk_words_respects_maximum_and_can_disable_sentence_enders() {
        let settings = AppSettings {
            min_chunk_size: 1,
            max_chunk_size: 3,
            sentence_enders: false,
            ..AppSettings::default()
        };
        let words = ["One.", "two", "three", "four"]
            .into_iter()
            .map(str::to_string)
            .collect();

        assert_eq!(
            chunk_words(words, &settings),
            vec!["One. two three", "four"]
        );
    }

    #[test]
    fn settings_require_model_dimensions_to_match() {
        let settings = AppSettings {
            vector_dimensions: 768,
            ..AppSettings::default()
        };

        assert!(validate_settings(&settings).is_err());
    }

    #[test]
    fn document_filter_escapes_library_and_path_values() {
        assert_eq!(
            document_filter("library's docs", Some("folder/file's.txt")),
            "library = 'library''s docs' AND path = 'folder/file''s.txt'"
        );
    }
}
