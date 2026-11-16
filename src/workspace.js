const { invoke } = window.__TAURI__.core;
const { getCurrentWebviewWindow } = window.__TAURI__.webviewWindow;
const { open } = window.__TAURI__.dialog;

const dropZone = document.getElementById('fileDropZone');
const filesList = document.getElementById('uploadedFilesList');

dropZone.addEventListener('click', async () => {
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

async function handleFilePaths(paths) {
  filesList.innerHTML = '';

  paths.forEach(filePath => {
    const fileItem = document.createElement('div');
    fileItem.className = 'file-item';
    fileItem.textContent = filePath.split(/[\\/]/).pop();
    filesList.appendChild(fileItem);
  });

  try {
    console.log("Sending file paths to Rust:", paths);
    const res = await invoke("process_documents", { docs: paths });
    console.log("Backend response:", res);
  } catch (error) {
    console.error("Error calling Rust backend:", error);
  }
}