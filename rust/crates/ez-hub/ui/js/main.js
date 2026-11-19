// Arranque de la ventana: espacios, terminal del elefante, peces, traza y tarjetas.

import { TEXTS, agentLabel } from "./texts.js";
import { loadSpaces, createSpace, renderSpaces, markSelected } from "./spaces.js";
import { mountTerminal } from "./terminal.js";
import { renderFish, renderTrace } from "./panel.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const byId = (id) => document.getElementById(id);
let activeSpace = null;
let elephantView = null;
let fishViews = [];

function notice(text) {
  byId("input-notice").textContent = text ?? "";
}

function fitAll() {
  if (elephantView) elephantView.fit();
  for (const view of fishViews) view.fit();
}

async function closeViews() {
  for (const view of [elephantView, ...fishViews]) {
    if (view) {
      try {
        await view.close();
      } catch (error) {
        console.warn(String(error));
      }
    }
  }
  elephantView = null;
  fishViews = [];
}

async function selectSpace(space) {
  await closeViews();
  activeSpace = space;
  markSelected(byId("space-list"), space.id);
  byId("elephant-header").textContent = TEXTS.elephantOf(agentLabel(space.elephant.agent));
  elephantView = await mountTerminal(byId("elephant-view"), {
    argv: space.elephant.argv,
    cwd: space.folder,
    space: space.id,
  });
  fitAll();
  await refreshFish();
  await refreshTrace();
}

async function refreshFish() {
  if (!activeSpace) return;
  const state = await invoke("hub_state", { space: activeSpace.id });
  fishViews = [];
  renderFish(
    byId("fish-pane"),
    state.fish,
    async (fish, host) => {
      const view = await mountTerminal(host, { argv: fish.argv, cwd: fish.cwd });
      if (view) fishViews.push(view);
    },
    fitAll,
  );
  fitAll();
}

async function refreshTrace() {
  if (!activeSpace) return;
  const state = await invoke("hub_state", { space: activeSpace.id });
  renderTrace(byId("timeline-list"), byId("card-list"), state, async (card, action, comment) => {
    try {
      await invoke("card_action", { card, action, comment });
      await refreshTrace();
    } catch (error) {
      notice(String(error));
    }
  });
}

async function sendInput() {
  const field = byId("input-text");
  const text = field.value;
  if (!activeSpace) {
    notice(TEXTS.noSpace);
    return;
  }
  if (!text.trim()) {
    notice(TEXTS.notice.emptyInput);
    return;
  }
  try {
    await invoke("elephant_input", { space: activeSpace.id, text });
    field.value = "";
    notice("");
  } catch (error) {
    notice(TEXTS.notice.inputFailed);
  }
}

function showTab(name) {
  const fish = name === "fish";
  byId("fish-pane").hidden = !fish;
  byId("trace-pane").hidden = fish;
  byId("tab-fish").classList.toggle("active", fish);
  byId("tab-trace").classList.toggle("active", !fish);
  fitAll();
}

function applyTexts() {
  for (const node of document.querySelectorAll("[data-text]")) {
    node.textContent = TEXTS[node.dataset.text] ?? node.textContent;
  }
  byId("new-space").textContent = TEXTS.newSpace;
  byId("elephant-header").textContent = TEXTS.elephant;
  byId("input-text").placeholder = TEXTS.placeholder;
  byId("btn-voice").textContent = TEXTS.voiceNote;
  byId("btn-call").textContent = TEXTS.call;
  byId("btn-send").textContent = TEXTS.send;
  for (const id of ["btn-voice", "btn-call"]) {
    byId(id).title = TEXTS.soon;
  }
}

function wire() {
  byId("tab-fish").addEventListener("click", () => showTab("fish"));
  byId("tab-trace").addEventListener("click", () => showTab("trace"));
  byId("btn-send").addEventListener("click", sendInput);
  byId("input-text").addEventListener("keydown", (event) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      sendInput();
    }
  });
  byId("new-space").addEventListener("click", () => {
    byId("space-form").hidden = !byId("space-form").hidden;
    byId("space-error").textContent = "";
    if (!byId("space-form").hidden) byId("space-name").focus();
  });
  byId("space-cancel").addEventListener("click", () => {
    byId("space-form").hidden = true;
  });
  byId("space-form").addEventListener("submit", async (event) => {
    event.preventDefault();
    const error = byId("space-error");
    const carpeta = byId("space-folder").value.trim();
    if (!carpeta) {
      error.textContent = TEXTS.notice.needFolder;
      return;
    }
    try {
      const space = await createSpace({
        nombre: byId("space-name").value,
        carpeta,
        agente: byId("space-agent").value,
      });
      byId("space-form").hidden = true;
      const spaces = await loadSpaces();
      renderSpaces(spaces, byId("space-list"), selectSpace);
      await selectSpace(space);
    } catch (failure) {
      error.textContent = String(failure);
    }
  });
  window.addEventListener("resize", fitAll);
  listen("hub://changed", () => {
    refreshTrace().catch((error) => notice(String(error)));
  });
}

async function boot() {
  applyTexts();
  wire();
  const spaces = await loadSpaces();
  renderSpaces(spaces, byId("space-list"), selectSpace);
  if (spaces.length > 0) {
    await selectSpace(spaces[0]);
  }
}

boot().catch((error) => notice(String(error)));
