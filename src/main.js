const { invoke } = window.__TAURI__.core;

let createNewBtn;
let createModal;
let createBtn;
let cancelBtn;
let nameInput;

async function create_library(name) {
  
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
      return;
    } else {
      create_library(name);
    }
  });
});
