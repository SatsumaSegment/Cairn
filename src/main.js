const { invoke } = window.__TAURI__.core;

let createNewBtn;
let createModal;
let createBtn;
let cancelBtn;
let nameInput;
let libsBox;
let libsList;
let backBtn;


function navigate(viewId) {
  document.querySelectorAll('.view').forEach(view => {
    view.classList.add('hidden');
  });

  const targetView = document.querySelector("."+viewId);
  if (targetView) {
    targetView.classList.remove('hidden');
  }
}


async function list_libraries() {
  try {
    const p = window.__TAURI__.path;
    const fs = window.__TAURI__.fs;
    const appDataPath = await p.appDataDir();
    const dataDirPath = await p.join(appDataPath, "data");
    const libraries = await fs.readDir(dataDirPath);
    if (libraries) {
      libsBox.innerHTML = "";
      for (const lib of libraries) {
        const lib_name = lib.name.replace(".json", "");
        libsBox.insertAdjacentHTML('beforeend', `<p class="lib-el" data-name="${lib_name}">${lib_name}</p>`);
      }
    }
    return [];
  } catch (err) {
    console.error("Error fetching libraries:", err);
    return [];
  }
}


async function create_library(name) {
  try {
    await invoke("create_library", { name: name });
    
    createModal.style.display = "none";
    nameInput.value = "";
  } catch (error) {
    console.error("Failed to create library:", error);
  }
}


window.addEventListener("DOMContentLoaded", () => {
  createNewBtn = document.querySelector(".create-new-btn");
  createModal = document.querySelector(".create-modal");
  createBtn = document.querySelector(".create-btn");
  cancelBtn = document.querySelector(".cancel-btn");
  nameInput = document.querySelector(".name-lib-input");
  libsBox = document.querySelector(".libs-list");
  libsList = document.querySelector(".libs-list");
  backBtn = document.querySelector('.back-btn');

  createNewBtn.addEventListener("click", () => {
    createModal.style.display = "flex";
  });
  cancelBtn.addEventListener("click", () => {
    createModal.style.display = "none";
  });

  createBtn.addEventListener("click", () => {
    let name = nameInput.value;
    if (name.length == 0) {
      nameInput.classList.add("shake");

      setTimeout(() => {
        nameInput.classList.remove("shake");
      }, 500);
    } else {
      create_library(name);
      list_libraries();
    }
  });

  libsList.addEventListener('click', (event) => {
    const card = event.target.closest('.lib-el');
    if (!card) return;
    const libraryName = card.dataset.name;
    navigate('workspace-view');
  });


  backBtn.addEventListener('click', () => {
    navigate('home-view');
  });


  navigate('home-view');
  list_libraries();
});
