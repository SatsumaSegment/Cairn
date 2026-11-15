const { invoke } = window.__TAURI__.core;

let createNewBtn;
let createModal;
let createBtn;
let cancelBtn;
let nameInput;


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
    }
  });
});
