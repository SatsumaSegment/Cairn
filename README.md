# Carin

A fast, local RAG system built in Rust.

## Workspace settings

Open **Settings** in a library workspace to configure the FastEmbed model and its
vector dimensions, the Ollama LLM model, minimum and maximum chunk sizes in words,
sentence-ending chunk boundaries, and the maximum number of search results.
Settings are saved locally and reused on the next launch. Embedding dimensions
must match the selected FastEmbed model. Changing the embedding model or vector
dimensions does not re-embed documents already in the database.