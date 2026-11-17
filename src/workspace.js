const { invoke } = window.__TAURI__.core;
const { getCurrentWebviewWindow } = window.__TAURI__.webviewWindow;
const { open } = window.__TAURI__.dialog;

const dropZone = document.getElementById('fileDropZone');
const uploadFilesBtn = document.getElementById('uploadFilesBtn');
const filesList = document.getElementById('uploadedFilesList');
const filesStatus = document.getElementById('documentListStatus');
const libraryTitle = document.querySelector('.sidebar-library-name');
const chatHistory = document.getElementById('chatHistory');
const chatInput = document.getElementById('chatInput');
const sendBtn = document.getElementById('sendBtn');
const chatModeOptions = document.querySelectorAll('.chat-mode-option');
const settingsButton = document.querySelector('.settings-btn');
const settingsDialog = document.getElementById('settingsDialog');
const settingsForm = document.getElementById('settingsForm');
const settingsError = document.getElementById('settingsError');
const embeddingModelNameInput = document.getElementById('embeddingModelName');
const vectorDimensionsInput = document.getElementById('vectorDimensions');
const embeddingModelOptions = document.getElementById('embeddingModelOptions');
const embeddingModelDimensions = new Map();
const { Channel } = window.__TAURI__.core;
let chatMode = 'chat';

chatModeOptions.forEach((option) => {
  option.addEventListener('click', () => {
    chatMode = option.dataset.mode;
    chatModeOptions.forEach((modeOption) => {
      const isActive = modeOption === option;
      modeOption.classList.toggle('active', isActive);
      modeOption.setAttribute('aria-pressed', String(isActive));
    });
  });
});

uploadFilesBtn.addEventListener('click', async () => {
  try {
    const selected = await open({
      multiple: true,
      filters: [{ name: 'Text Documents', extensions: ['txt', 'md'] }]
    });

    if (selected === null) return;

    const paths = Array.isArray(selected) ? selected : [selected];
    await handleFilePaths(paths);
  } catch (error) {
    console.error('Could not select or process files:', error);
  }
});


async function initTauriDrop() {
  try {
    const appWindow = getCurrentWebviewWindow();

    await appWindow.onDragDropEvent(async (event) => {
      const payload = event.payload;
      
      if (payload.type === 'over') {
        dropZone.classList.add('drag-over');
      } else if (payload.type === 'leave') {
        dropZone.classList.remove('drag-over');
      } else if (payload.type === 'drop') {
        dropZone.classList.remove('drag-over');
        
        const paths = payload.paths;
        if (paths && paths.length > 0) {
          await handleFilePaths(paths);
        }
      }
    });
  } catch (err) {
    console.error("Failed to initialize Tauri drop listener:", err);
  }
}

initTauriDrop();

function getFileName(path) {
  return path.split(/[\\/]/).pop() || path;
}

function renderLibraryDocuments(paths) {
  filesList.replaceChildren();

  if (paths.length === 0) {
    const emptyMessage = document.createElement('p');
    emptyMessage.className = 'document-list-empty';
    emptyMessage.textContent = 'No documents in this library yet.';
    filesList.appendChild(emptyMessage);
    return;
  }

  for (const path of paths) {
    const fileItem = document.createElement('div');
    fileItem.className = 'file-item';
    fileItem.setAttribute('role', 'listitem');
    fileItem.title = path;

    const fileName = document.createElement('span');
    fileName.className = 'file-item-name';
    fileName.textContent = getFileName(path);

    const removeButton = document.createElement('button');
    removeButton.className = 'remove-document-btn';
    removeButton.type = 'button';
    removeButton.textContent = '×';
    removeButton.title = `Remove ${getFileName(path)} from this library`;
    removeButton.setAttribute('aria-label', `Remove ${getFileName(path)} from this library`);
    removeButton.dataset.path = path;

    fileItem.append(fileName, removeButton);
    filesList.appendChild(fileItem);
  }
}

async function refreshLibraryDocuments(library) {
  filesStatus.textContent = 'Loading documents…';
  const paths = await invoke('list_library_documents', { library });
  renderLibraryDocuments(paths);
  filesStatus.textContent = '';
}

window.addEventListener('library-selected', async (event) => {
  const { library } = event.detail;
  try {
    await refreshLibraryDocuments(library);
  } catch (error) {
    console.error('Could not load library documents:', error);
    filesStatus.textContent = `Could not load documents: ${error}`;
  }
});

filesList.addEventListener('click', async (event) => {
  const removeButton = event.target.closest('.remove-document-btn');
  if (!removeButton || removeButton.disabled) return;

  const library = libraryTitle.dataset.name;
  const path = removeButton.dataset.path;
  if (!library || !path) return;

  removeButton.disabled = true;
  filesStatus.textContent = `Removing ${getFileName(path)}…`;
  try {
    await invoke('remove_library_document', { library, path });
    await refreshLibraryDocuments(library);
  } catch (error) {
    console.error('Could not remove library document:', error);
    filesStatus.textContent = `Could not remove ${getFileName(path)}: ${error}`;
    removeButton.disabled = false;
  }
});

function showSettingsError(error) {
  settingsError.textContent = `Settings error: ${error}`;
  settingsError.hidden = false;
}

settingsButton.addEventListener('click', async () => {
  settingsError.hidden = true;
  settingsDialog.showModal();

  try {
    const response = await invoke('get_settings');
    const settings = response.settings;
    embeddingModelDimensions.clear();
    embeddingModelOptions.replaceChildren();
    for (const model of response.embeddingModels) {
      embeddingModelDimensions.set(model.name, model.dimensions);
      const option = document.createElement('option');
      option.value = model.name;
      embeddingModelOptions.appendChild(option);
    }

    embeddingModelNameInput.value = settings.embeddingModelName;
    vectorDimensionsInput.value = settings.vectorDimensions;
    document.getElementById('llmModelName').value = settings.llmModelName;
    document.getElementById('minChunkSize').value = settings.minChunkSize;
    document.getElementById('maxChunkSize').value = settings.maxChunkSize;
    document.getElementById('sentenceEnders').checked = settings.sentenceEnders;
    document.getElementById('maxResults').value = settings.maxResults;
  } catch (error) {
    showSettingsError(error);
  }
});

embeddingModelNameInput.addEventListener('change', () => {
  const dimensions = embeddingModelDimensions.get(embeddingModelNameInput.value);
  if (dimensions !== undefined) vectorDimensionsInput.value = dimensions;
});

document.getElementById('closeSettingsBtn').addEventListener('click', () => settingsDialog.close());
document.getElementById('cancelSettingsBtn').addEventListener('click', () => settingsDialog.close());
settingsDialog.addEventListener('click', (event) => {
  if (event.target === settingsDialog) settingsDialog.close();
});

settingsForm.addEventListener('submit', async (event) => {
  event.preventDefault();
  if (!settingsForm.reportValidity()) return;

  const saveButton = document.getElementById('saveSettingsBtn');
  saveButton.disabled = true;
  settingsError.hidden = true;

  const settings = {
    embeddingModelName: embeddingModelNameInput.value.trim(),
    vectorDimensions: Number(vectorDimensionsInput.value),
    llmModelName: document.getElementById('llmModelName').value.trim(),
    minChunkSize: Number(document.getElementById('minChunkSize').value),
    maxChunkSize: Number(document.getElementById('maxChunkSize').value),
    sentenceEnders: document.getElementById('sentenceEnders').checked,
    maxResults: Number(document.getElementById('maxResults').value)
  };

  try {
    await invoke('save_settings', { settings });
    settingsDialog.close();
  } catch (error) {
    showSettingsError(error);
  } finally {
    saveButton.disabled = false;
  }
});

async function handleFilePaths(paths) {
  const library = libraryTitle.dataset.name;
  if (!library) {
    console.error('Cannot process files because no library is selected.');
    return;
  }

  filesStatus.textContent = 'Adding documents…';
  try {
    console.log("Sending file paths to Rust:", paths);
    await invoke("process_documents", { library, docs: paths });
    await refreshLibraryDocuments(library);
  } catch (error) {
    console.error("Error calling Rust backend:", error);
    filesStatus.textContent = `Could not add documents: ${error}`;
  }
}

function appendChatMessage(role, content) {
  const message = document.createElement('div');
  message.className = `chat-message ${role}`;

  const paragraph = document.createElement('p');
  paragraph.textContent = content;
  message.appendChild(paragraph);
  chatHistory.appendChild(message);
  chatHistory.scrollTop = chatHistory.scrollHeight;
  return message;
}

function renderSearchResults(message, results) {
  message.replaceChildren();

  if (results.length === 0) {
    const paragraph = document.createElement('p');
    paragraph.textContent = 'I could not find matching document chunks in this library.';
    message.appendChild(paragraph);
    return;
  }

  const heading = document.createElement('p');
  heading.textContent = `Top ${results.length} relevant document chunk${results.length === 1 ? '' : 's'}:`;
  message.appendChild(heading);

  for (const result of results) {
    const chunk = document.createElement('section');
    chunk.className = 'chat-search-result';

    const source = document.createElement('p');
    source.className = 'chat-search-source';
    source.textContent = result.path.split(/[\\/]/).pop();

    const text = document.createElement('p');
    text.className = 'chat-search-text';
    text.textContent = result.text;

    chunk.append(source, text);
    message.appendChild(chunk);
  }
}

async function searchDocuments() {
  const query = chatInput.value.trim();
  if (!query || sendBtn.disabled) return;

  const library = libraryTitle.dataset.name;
  if (!library) {
    appendChatMessage('assistant', 'Select a library before searching its documents.');
    return;
  }

  appendChatMessage('user', query);
  chatInput.value = '';
  sendBtn.disabled = true;
  const response = appendChatMessage(
    'assistant',
    chatMode === 'search' ? 'Searching this library...' : 'Thinking...'
  );

  try {
    if (chatMode === 'search') {
      const results = await invoke('search_documents', {
        library,
        queryText: query
      });
      renderSearchResults(response, results);
    } else {
      await chatWithDocuments(library, query, response);
    }
  } catch (error) {
    console.error('Document request failed:', error);
    response.replaceChildren();
    const paragraph = document.createElement('p');
    paragraph.textContent = `Request failed: ${error}`;
    response.appendChild(paragraph);
  } finally {
    sendBtn.disabled = false;
    chatInput.focus();
  }
}

async function chatWithDocuments(library, query, response) {
  response.replaceChildren();

  const typingIndicator = document.createElement('div');
  typingIndicator.className = 'typing-indicator';
  typingIndicator.setAttribute('role', 'status');
  typingIndicator.setAttribute('aria-label', 'Waiting for model response');
  typingIndicator.innerHTML = '<span></span><span></span><span></span>';

  const paragraph = document.createElement('p');
  paragraph.className = 'chat-answer-text';
  response.append(typingIndicator, paragraph);

  let receivedText = false;
  const onChunk = new Channel((chunk) => {
    if (chunk.text) {
      typingIndicator.remove();
      receivedText = true;
      paragraph.textContent += chunk.text;
      chatHistory.scrollTop = chatHistory.scrollHeight;
    }

    if (chunk.done && !receivedText) {
      typingIndicator.remove();
      paragraph.textContent = 'The chat service returned an empty response.';
    }
  });

  await invoke('chat_with_documents', {
    library,
    userQuery: query,
    onChunk
  });
}

sendBtn.addEventListener('click', searchDocuments);
chatInput.addEventListener('keydown', (event) => {
  if (event.key === 'Enter') {
    event.preventDefault();
    searchDocuments();
  }
});