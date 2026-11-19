// Arranque de la ventana: árbol, pestañas, barra, traza y tarjetas (§15.6).

import { TEXTS, AGENT_NAMES } from "./texts.js";
import { renderTree, markSelected, ensureSelection } from "./tree.js";
import { openTab, toggleSplit, sendToActive, tabCount, activeTerminalsWithData, activeTerminalMetrics, fitAll } from "./tabs.js";
import { renderPanel, reportCardError } from "./panel.js";
import { wireVoice } from "./voice.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;
const byId = (id) => document.getElementById(id);

const loadSpaces = () => invoke("spaces_list");
const createSpace = ({ nombre, carpeta }) => invoke("space_create", { nombre, carpeta });

let spaces = [];
let activeSpace = null;
let currentTree = { nodes: [] };
let activeMain = null;
let demo = false;
let inventory = [];

function notice(text) {
  byId("input-notice").textContent = text ?? "";
}

function applyTexts() {
  for (const node of document.querySelectorAll("[data-text]")) {
    node.textContent = TEXTS[node.dataset.text] ?? node.textContent;
  }
  for (const node of document.querySelectorAll("[data-placeholder]")) {
    node.placeholder = TEXTS[node.dataset.placeholder] ?? "";
  }
}

function findMain(nodes, id) {
  for (const node of nodes) {
    if (node.id === id) return node;
    const inner = findMain(node.children ?? [], id);
    if (inner) return node;
  }
  return null;
}

async function loadTree() {
  if (!activeSpace) return;
  currentTree = await invoke("space_tree", { space: activeSpace.id });
  renderTree(byId("tree"), spaces, activeSpace, currentTree.nodes, selectSpace, onNode);
  ensureSelection(byId("tree"), `space:${activeSpace.id}`);
}

async function loadTrace() {
  if (!activeSpace) return;
  const elephant = activeMain?.id ?? null;
  const state = await invoke("hub_state", { space: activeSpace.id, elephant });
  renderPanel(
    byId("timeline-list"),
    byId("card-list"),
    state,
    (card, action, comment) => invoke("card_action", { card, action, comment }),
    reportCardError,
  );
}

async function refresh() {
  await loadTree();
  await loadTrace();
}

async function selectSpace(space) {
  activeSpace = space;
  activeMain = null;
  markSelected(byId("tree"), `space:${space.id}`);
  await loadTree();
  await loadTrace();
}

async function onNode(space, node) {
  if (space.id !== activeSpace?.id) await selectSpace(space);
  activeMain = findMain(currentTree.nodes, node.id) ?? node;
  markSelected(byId("tree"), node.id);
  await openTab(space, node, demo);
  await loadTrace();
}

async function openDefaultTabs() {
  const first = currentTree.nodes[0];
  if (!first) return;
  activeMain = first;
  await openTab(activeSpace, first, demo);
  const child = first.children?.[0];
  if (child) await openTab(activeSpace, child, demo);
  await loadTrace();
}

async function sendInput() {
  const field = byId("input-text");
  const text = field.value;
  if (!activeSpace) {
    notice(TEXTS.noSpace);
    return;
  }
  if (!text.trim()) {
    notice(TEXTS.emptyInput ?? "Escribe algo antes de enviar.");
    return;
  }
  try {
    await sendToActive(text, demo);
    field.value = "";
    notice("");
  } catch (error) {
    notice(`${TEXTS.sendFailed}: ${String(error)}`);
  }
}

function showTab(name) {
  const trace = name === "trace";
  byId("trace-pane").hidden = !trace;
  byId("cards-pane").hidden = trace;
  byId("tab-trace").classList.toggle("active", trace);
  byId("tab-cards").classList.toggle("active", !trace);
}

function toggleForm(id, other) {
  byId(other).hidden = true;
  const form = byId(id);
  form.hidden = !form.hidden;
  if (!form.hidden) form.querySelector("input")?.focus();
}

function agentsForForm() {
  const data = inventory?.data ?? [];
  if (data.length === 0) {
    return Object.entries(AGENT_NAMES)
      .filter(([id]) => id !== "powershell")
      .map(([id, name]) => ({ id, nombre: name, modelos: [] }));
  }
  return data;
}

function fillAgents() {
  const select = byId("main-agent");
  select.replaceChildren();
  for (const agent of agentsForForm()) {
    const option = document.createElement("option");
    option.value = agent.id ?? agent.agente ?? "";
    option.textContent = agent.nombre ?? option.value;
    option.dataset.models = JSON.stringify(agent.modelos ?? agent.models ?? []);
    select.appendChild(option);
  }
  fillModels();
}

function fillModels() {
  const option = byId("main-agent").selectedOptions[0];
  const models = option ? JSON.parse(option.dataset.models ?? "[]") : [];
  const select = byId("main-model");
  const text = byId("main-model-text");
  if (models.length > 0) {
    select.hidden = false;
    text.hidden = true;
    select.replaceChildren();
    for (const model of models) {
      const item = document.createElement("option");
      item.value = model;
      item.textContent = model;
      select.appendChild(item);
    }
  } else {
    select.hidden = true;
    text.hidden = false;
  }
}

async function createNewSpace(event) {
  event.preventDefault();
  const error = byId("space-error");
  const carpeta = byId("space-folder").value.trim();
  if (!carpeta) {
    error.textContent = TEXTS.needFolder;
    return;
  }
  try {
    const space = await createSpace({
      nombre: byId("space-name").value.trim(),
      carpeta,
    });
    byId("space-form").hidden = true;
    spaces = await loadSpaces();
    await selectSpace(space);
  } catch (failure) {
    error.textContent = String(failure);
  }
}

async function createNewMain(event) {
  event.preventDefault();
  const error = byId("main-error");
  if (!activeSpace) {
    error.textContent = TEXTS.noSpace;
    return;
  }
  const modelo = byId("main-model").hidden
    ? byId("main-model-text").value.trim()
    : byId("main-model").value;
  if (!byId("main-name").value.trim() || !modelo) {
    error.textContent = TEXTS.needModel;
    return;
  }
  try {
    const info = await invoke("main_agent_create", {
      space: activeSpace.id,
      nombre: byId("main-name").value.trim(),
      agente: byId("main-agent").value,
      modelo,
      esfuerzo: byId("main-effort").value.trim() || null,
    });
    byId("main-form").hidden = true;
    await refresh();
    activeMain = { id: info.id };
    markSelected(byId("tree"), info.id);
    await loadTrace();
  } catch (failure) {
    error.textContent = String(failure);
  }
}

async function showStatus() {
  const status = await invoke("runtime_status");
  demo = status.mode === "demo";
  const strip = byId("status-strip");
  if (status.mode === "degradado") {
    strip.textContent = status.reason;
    strip.hidden = false;
  } else {
    strip.hidden = true;
  }
}

function wire() {
  byId("tab-trace").addEventListener("click", () => showTab("trace"));
  byId("tab-cards").addEventListener("click", () => showTab("cards"));
  byId("toggle-right").addEventListener("click", () => {
    const app = byId("app");
    app.classList.toggle("right-collapsed");
    byId("toggle-right").textContent = app.classList.contains("right-collapsed")
      ? TEXTS.expand
      : TEXTS.collapse;
  });
  byId("split-tab").addEventListener("click", toggleSplit);
  byId("btn-send").addEventListener("click", sendInput);
  byId("input-text").addEventListener("keydown", (event) => {
    if (event.key === "Enter" && !event.shiftKey) {
      event.preventDefault();
      sendInput();
    }
  });
  byId("new-space").addEventListener("click", () => toggleForm("space-form", "main-form"));
  byId("new-main").addEventListener("click", () => {
    toggleForm("main-form", "space-form");
    if (!byId("main-form").hidden) fillAgents();
  });
  byId("space-cancel").addEventListener("click", () => { byId("space-form").hidden = true; });
  byId("main-cancel").addEventListener("click", () => { byId("main-form").hidden = true; });
  byId("space-form").addEventListener("submit", createNewSpace);
  byId("main-form").addEventListener("submit", createNewMain);
  byId("main-agent").addEventListener("change", fillModels);
  window.addEventListener("resize", fitAll);
  listen("hub://changed", () => refresh().catch((error) => notice(String(error))));
  wireVoice(notice);
}

function forbiddenCount() {
  const visible = (document.body.innerText ?? "").toLowerCase();
  return ["elefante", "pez", "peces"].reduce(
    (total, word) => total + visible.split(word).length - 1,
    0,
  );
}

async function maybeSelfcheck() {
  if (!(await invoke("selfcheck_enabled").catch(() => false))) return;
  setTimeout(() => reportSelfcheck(), 6000);
}

async function reportSelfcheck() {
  const traceTexts = Array.from(byId("timeline-list").querySelectorAll(".event-text"));
  const metrics = activeTerminalMetrics();
  const report = {
    errores: window.__ezErrors ?? [],
    nodos: byId("tree").querySelectorAll(".tree-row").length,
    pestañas: tabCount(),
    terminales_con_datos: activeTerminalsWithData(),
    eventos: byId("timeline-list").querySelectorAll(".timeline-row").length,
    tarjetas: byId("card-list").querySelectorAll(".approval-card").length,
    textos_prohibidos: forbiddenCount(),
    traza_ancho_min: traceTexts.length
      ? Math.min(...traceTexts.map((node) => node.clientWidth))
      : 0,
    panel_ancho: byId("timeline-list").clientWidth,
    etiquetas_pestañas: Array.from(byId("tab-list").querySelectorAll(".tab-label")).map(
      (node) => node.textContent,
    ),
    banda_terminal_px: metrics.banda,
    alto_celda_px: metrics.celda,
  };
  try {
    await invoke("selfcheck_report", { report });
  } catch (error) {
    window.__ezErrors?.push(String(error));
  }
}

async function boot() {
  applyTexts();
  wire();
  await showStatus();
  inventory = await invoke("inventory").catch(() => ({ data: [] }));
  spaces = await loadSpaces();
  renderTree(byId("tree"), spaces, null, [], selectSpace, onNode);
  if (spaces.length > 0) {
    await selectSpace(spaces[0]);
    if (demo) await openDefaultTabs();
  }
}

boot()
  .catch((error) => {
    notice(String(error));
    window.__ezErrors?.push(String(error));
  })
  .finally(() => {
    maybeSelfcheck().catch((error) => console.warn(String(error)));
  });
