// Columna derecha: tarjetas de peces, traza y aprobaciones (§10).

import { TEXTS, statusLabel } from "./texts.js";

/** Pinta los peces y llama a `openView` con el hueco de cada terminal. */
export function renderFish(pane, fish, openView, onExpand) {
  pane.replaceChildren();
  if (fish.length === 0) {
    const empty = document.createElement("p");
    empty.className = "empty";
    empty.textContent = TEXTS.emptyFish;
    pane.appendChild(empty);
    return;
  }
  const list = document.createElement("div");
  list.className = "fish-list";
  for (const item of fish) {
    const card = document.createElement("article");
    card.className = "fish-card";

    const header = document.createElement("header");
    header.className = "fish-header";
    const id = document.createElement("strong");
    id.textContent = item.id;
    const role = document.createElement("span");
    role.textContent = item.role;
    const model = document.createElement("small");
    model.textContent = `${item.agent} · ${item.model}`;
    const status = document.createElement("span");
    status.className = `chip status-${item.status}`;
    status.textContent = statusLabel(item.status);
    header.append(id, role, model, status);
    header.addEventListener("click", () => {
      card.classList.toggle("expanded");
      onExpand();
    });

    const host = document.createElement("div");
    host.className = "terminal-host fish-terminal";
    card.append(header, host);
    list.appendChild(card);
    openView(item, host, onExpand);
  }
  pane.appendChild(list);
}

/** Pinta los eventos y las tarjetas de aprobación. */
export function renderTrace(timelinePane, cardsPane, state, onAction) {
  timelinePane.replaceChildren();
  cardsPane.replaceChildren();

  if (state.timeline.length === 0) {
    timelinePane.appendChild(empty(TEXTS.emptyTimeline));
  } else {
    for (const event of state.timeline) {
      const row = document.createElement("div");
      row.className = "timeline-row";
      const time = document.createElement("time");
      time.textContent = event.time;
      const kind = document.createElement("span");
      kind.className = "timeline-kind";
      kind.textContent = event.kind;
      const text = document.createElement("span");
      text.textContent = event.text;
      row.append(time, kind, text);
      timelinePane.appendChild(row);
    }
  }

  if (state.cards.length === 0) {
    cardsPane.appendChild(empty(TEXTS.emptyCards));
  } else {
    for (const card of state.cards) {
      cardsPane.appendChild(renderCard(card, onAction));
    }
  }
}

function empty(text) {
  const node = document.createElement("p");
  node.className = "empty";
  node.textContent = text;
  return node;
}

function renderCard(card, onAction) {
  const article = document.createElement("article");
  article.className = "approval-card";
  const title = document.createElement("h3");
  title.textContent = card.title;
  const body = document.createElement("pre");
  body.textContent = card.body;
  article.append(title, body);

  const actions = document.createElement("div");
  actions.className = "card-actions";
  const approve = document.createElement("button");
  approve.type = "button";
  approve.className = "primary";
  approve.textContent = TEXTS.approve;
  approve.addEventListener("click", () => onAction(card.id, "approve", null));
  const reject = document.createElement("button");
  reject.type = "button";
  reject.textContent = TEXTS.reject;
  actions.append(approve, reject);
  article.appendChild(actions);

  reject.addEventListener("click", () => {
    if (actions.querySelector(".reject-box")) {
      return;
    }
    const box = document.createElement("div");
    box.className = "reject-box";
    const comment = document.createElement("textarea");
    comment.rows = 3;
    comment.placeholder = TEXTS.rejectHint;
    const error = document.createElement("p");
    error.className = "form-error";
    const send = document.createElement("button");
    send.type = "button";
    send.textContent = TEXTS.rejectSend;
    send.addEventListener("click", () => {
      const text = comment.value.trim();
      if (!text) {
        error.textContent = TEXTS.rejectHint;
        return;
      }
      onAction(card.id, "reject", text);
    });
    box.append(comment, error, send);
    article.appendChild(box);
    comment.focus();
  });

  return article;
}
