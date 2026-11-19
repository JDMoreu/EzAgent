// Árbol plegable de espacios, agentes principales y subagentes (§15.6).

import { TEXTS, agentLabel } from "./texts.js";

const collapsed = new Set();
let selected = null;

/** Marca la fila seleccionada sin volver a pintar. */
export function markSelected(container, key) {
  selected = key;
  for (const row of container.querySelectorAll(".tree-row")) {
    row.classList.toggle("selected", row.dataset.key === key);
  }
}

/** Abre o cierra los hijos de un nodo; el estado sobrevive a los refrescos. */
export function toggleCollapse(container, key) {
  if (collapsed.has(key)) collapsed.delete(key);
  else collapsed.add(key);
  const rows = Array.from(container.querySelectorAll(".tree-row"));
  const byKey = new Map(rows.map((item) => [item.dataset.key, item]));
  for (const row of rows) {
    if (row.dataset.key === key) {
      const twisty = row.querySelector(".twisty");
      if (twisty) twisty.textContent = collapsed.has(key) ? "▸" : "▾";
      continue;
    }
    let parent = row.dataset.parent;
    let hide = false;
    while (parent) {
      if (collapsed.has(parent)) {
        hide = true;
        break;
      }
      parent = byKey.get(parent)?.dataset.parent;
    }
    row.hidden = hide;
  }
}

function row(container, { key, parent, depth, label, sub, status, twisty, space }) {
  const button = document.createElement("button");
  button.type = "button";
  button.className = `tree-row${space ? " space-row" : ""}`;
  button.dataset.key = key;
  if (parent) button.dataset.parent = parent;
  button.style.paddingLeft = `${10 + depth * 14}px`;
  if (selected === key) button.classList.add("selected");

  if (twisty) {
    const mark = document.createElement("span");
    mark.className = "twisty";
    mark.textContent = collapsed.has(key) ? "▸" : "▾";
    mark.addEventListener("click", (event) => {
      event.stopPropagation();
      toggleCollapse(container, key);
    });
    button.appendChild(mark);
  } else {
    const spacer = document.createElement("span");
    spacer.className = "twisty";
    button.appendChild(spacer);
  }
  if (status !== undefined) {
    const dot = document.createElement("span");
    dot.className = `dot ${status ?? "idle"}`;
    dot.title = status ?? "";
    button.appendChild(dot);
  }
  const name = document.createElement("span");
  name.className = "label";
  name.textContent = label;
  button.appendChild(name);
  if (sub) {
    const detail = document.createElement("span");
    detail.className = "sub-label";
    detail.textContent = sub;
    button.appendChild(detail);
  }
  container.appendChild(button);
  return button;
}

/** Pinta el árbol: un espacio con sus agentes principales y subagentes. */
export function renderTree(container, spaces, activeSpace, nodes, onSpace, onNode) {
  container.replaceChildren();
  if (spaces.length === 0) {
    const empty = document.createElement("p");
    empty.className = "tree-empty";
    empty.textContent = TEXTS.emptyTree;
    container.appendChild(empty);
    return;
  }
  for (const space of spaces) {
    const key = `space:${space.id}`;
    const button = row(container, {
      key,
      depth: 0,
      label: space.nombre || space.id,
      sub: space.id,
      space: true,
    });
    button.addEventListener("click", () => onSpace(space));
    if (space.id !== activeSpace?.id) continue;
    if (nodes.length === 0) {
      const empty = document.createElement("p");
      empty.className = "tree-empty";
      empty.textContent = TEXTS.noMain;
      container.appendChild(empty);
    }
    for (const node of nodes) addNode(container, node, 1, key, space, onNode);
  }
}

function addNode(container, node, depth, parent, space, onNode) {
  const sub = node.role
    ? `${node.role}${node.agent ? ` · ${agentLabel(node.agent)}` : ""}`
    : `${agentLabel(node.agent)} · ${node.model ?? ""}`;
  const button = row(container, {
    key: node.id,
    parent,
    depth,
    label: node.name || node.id,
    sub,
    status: node.status,
    twisty: node.children.length > 0,
  });
  button.addEventListener("click", () => onNode(space, node));
  const hidden = collapsed.has(parent);
  button.hidden = hidden;
  for (const child of node.children) addNode(container, child, depth + 1, node.id, space, onNode);
}
