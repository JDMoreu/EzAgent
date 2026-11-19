// Lista y formulario de espacios (§10 del diseño).

const { invoke } = window.__TAURI__.core;

import { TEXTS, agentLabel } from "./texts.js";

export async function loadSpaces() {
  return invoke("spaces_list");
}

export async function createSpace({ nombre, carpeta, agente }) {
  return invoke("space_create", { nombre, carpeta, agente });
}

export function renderSpaces(spaces, container, onSelect) {
  container.replaceChildren();
  if (spaces.length === 0) {
    const empty = document.createElement("p");
    empty.className = "empty";
    empty.textContent = TEXTS.emptySpaces;
    container.appendChild(empty);
    return;
  }
  for (const space of spaces) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "space-item";
    button.dataset.id = space.id;
    const name = document.createElement("strong");
    name.textContent = space.name;
    const detail = document.createElement("span");
    detail.textContent = agentLabel(space.elephant.agent);
    const folder = document.createElement("small");
    folder.textContent = space.folder;
    button.append(name, detail, folder);
    button.addEventListener("click", () => onSelect(space));
    container.appendChild(button);
  }
}

export function markSelected(container, id) {
  for (const item of container.querySelectorAll(".space-item")) {
    item.classList.toggle("selected", item.dataset.id === id);
  }
}
