// Pestañas de terminal: abrir, enfocar, cerrar, dividir y escribir (§15.6).

import { mountTerminal, terminalsWithData } from "./terminal.js";

const { invoke } = window.__TAURI__.core;
const byId = (id) => document.getElementById(id);

const tabs = [];
let activeKey = null;
let split = false;

export function tabCount() {
  return tabs.length;
}

export function activeTerminalsWithData() {
  return terminalsWithData();
}

/** Reajusta todas las terminales montadas al cambiar el tamaño de la ventana. */
export function fitAll() {
  for (const tab of tabs) tab.view?.fit();
}

function activeTab() {
  return tabs.find((tab) => tab.key === activeKey) ?? null;
}

function renderBar() {
  const list = byId("tab-list");
  list.replaceChildren();
  for (const tab of tabs) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = `tab${tab.key === activeKey ? " active" : ""}`;
    const label = document.createElement("span");
    label.textContent = tab.label;
    const close = document.createElement("span");
    close.className = "tab-close";
    close.textContent = "×";
    close.addEventListener("click", (event) => {
      event.stopPropagation();
      closeTab(tab.key).catch(() => {});
    });
    button.append(label, close);
    button.addEventListener("click", () => {
      activeKey = tab.key;
      renderBar();
      layout();
    });
    list.appendChild(button);
  }
}

function layout() {
  const first = byId("terminals").querySelector('[data-slot="0"] .terminal-host');
  const second = byId("terminals").querySelector('[data-slot="1"] .terminal-host');
  const active = activeTab();
  const other = tabs.find((tab) => tab.key !== activeKey) ?? null;
  byId("terminals").querySelector('[data-slot="1"]').hidden = !split || !other;
  if (active) first.appendChild(active.host);
  if (split && other) second.appendChild(other.host);
  fitAll();
}

/** Abre la pestaña de un nodo o enfoca la que ya esté abierta. */
export async function openTab(space, node, demo) {
  const existing = tabs.find((tab) => tab.key === node.id);
  if (existing) {
    activeKey = existing.key;
    renderBar();
    layout();
    return;
  }
  let argv;
  let cwd;
  if (demo) {
    argv = space.elefante?.argv?.length ? space.elefante.argv : ["powershell.exe", "-NoLogo"];
    cwd = space.carpeta;
  } else {
    let address = node.address;
    if (!address) {
      address = await invoke("node_address", { space: space.id, node: node.id });
    }
    argv = ["hc", "attach", address];
  }
  const host = document.createElement("div");
  host.className = "terminal-host";
  const view = await mountTerminal(host, { argv, cwd });
  const tab = {
    key: node.id,
    label: node.name || node.id,
    node,
    space,
    host,
    view,
  };
  tabs.push(tab);
  activeKey = tab.key;
  renderBar();
  layout();
}

/** Cierra solo la vista; el agente sigue vivo en herdr-core. */
export async function closeTab(key) {
  const index = tabs.findIndex((tab) => tab.key === key);
  if (index < 0) return;
  const [tab] = tabs.splice(index, 1);
  if (tab.view) await tab.view.close();
  tab.host.remove();
  if (activeKey === key) activeKey = tabs[0]?.key ?? null;
  renderBar();
  layout();
}

/** Alterna una única división lado a lado. */
export function toggleSplit() {
  split = !split;
  byId("split-tab").textContent = split ? "Unir" : "Dividir";
  layout();
}

/** Envía el texto de la barra al agente de la pestaña activa. */
export async function sendToActive(text, demo) {
  const tab = activeTab();
  if (!tab) throw new Error("No hay una pestaña activa.");
  if (demo) {
    if (!tab.view) throw new Error("La terminal activa no está abierta.");
    await invoke("view_send", { id: tab.view.id, text });
    return;
  }
  let address = tab.node.address;
  if (!address) {
    address = await invoke("node_address", { space: tab.space.id, node: tab.node.id });
  }
  await invoke("agent_send", { address, text });
}
