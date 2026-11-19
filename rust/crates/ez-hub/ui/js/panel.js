// Panel derecho: traza traducida y tarjetas con sus acciones (§15.6).

import { TEXTS, traceText } from "./texts.js";

function empty(text) {
  const node = document.createElement("p");
  node.className = "empty";
  node.textContent = text;
  return node;
}

function actionButton(label, primary, run) {
  const button = document.createElement("button");
  button.type = "button";
  if (primary) button.className = "primary";
  button.textContent = label;
  button.addEventListener("click", () => run(button));
  return button;
}

function rejectBox(card, article, onAction, report) {
  if (article.querySelector(".reject-box")) return;
  const box = document.createElement("div");
  box.className = "reject-box";
  const comment = document.createElement("textarea");
  comment.rows = 3;
  comment.placeholder = TEXTS.rejectHint;
  const error = document.createElement("p");
  error.className = "form-error";
  const send = actionButton(TEXTS.rejectSend, false, () => {
    const text = comment.value.trim();
    if (!text) {
      error.textContent = TEXTS.rejectHint;
      return;
    }
    onAction(card.id, "reject", text, article).catch(report);
  });
  box.append(comment, error, send);
  article.appendChild(box);
  comment.focus();
}

function cardActions(card, article, onAction, report) {
  const actions = document.createElement("div");
  actions.className = "card-actions";
  const run = (action, comment = null) =>
    onAction(card.id, action, comment, article).catch((error) => report(article, error));
  const kind = card.kind ?? "task";
  if (kind === "task" || kind === "team") {
    actions.append(
      actionButton(TEXTS.approve, true, () => run("approve")),
      actionButton(TEXTS.reject, false, () => rejectBox(card, article, onAction, report)),
    );
  } else if (kind === "pause") {
    actions.append(
      actionButton(TEXTS.resume, true, () => run("resume")),
      actionButton(TEXTS.moreTime, false, () => run("more_time")),
      actionButton(TEXTS.stopTask, false, () => run("stop")),
    );
  } else if (kind === "integrate") {
    actions.append(
      actionButton(TEXTS.integrate(card.branch ?? ""), true, () => run("merge")),
      actionButton(TEXTS.keepBranch, false, () => run("keep")),
    );
  }
  article.appendChild(actions);
}

function renderCard(card, onAction, report) {
  const article = document.createElement("article");
  article.className = "approval-card";
  const title = document.createElement("h3");
  title.textContent = card.title ?? "";
  const body = document.createElement("pre");
  body.textContent = card.body ?? "";
  article.append(title, body);
  cardActions(card, article, onAction, report);
  return article;
}

/** Pinta la traza y las tarjetas del agente principal seleccionado. */
export function renderPanel(timelineList, cardList, state, onAction, report) {
  timelineList.replaceChildren();
  cardList.replaceChildren();
  const events = state?.eventos ?? state?.timeline ?? [];
  if (events.length === 0) {
    timelineList.appendChild(empty(TEXTS.emptyTrace));
  } else {
    for (const event of events) {
      const row = document.createElement("div");
      row.className = "timeline-row";
      if (event.time) {
        const time = document.createElement("time");
        time.textContent = event.time;
        row.appendChild(time);
      }
      const text = document.createElement("span");
      text.className = "event-text";
      text.textContent = traceText(event);
      row.appendChild(text);
      timelineList.appendChild(row);
    }
  }
  const cards = state?.tarjetas ?? state?.cards ?? [];
  if (cards.length === 0) {
    cardList.appendChild(empty(TEXTS.emptyCards));
  } else {
    for (const card of cards) cardList.appendChild(renderCard(card, onAction, report));
  }
}

/** Muestra en la tarjeta el mensaje con el que el árbitro rechazó la acción. */
export function reportCardError(article, error) {
  let node = article.querySelector(".card-error");
  if (!node) {
    node = document.createElement("p");
    node.className = "card-error";
    article.appendChild(node);
  }
  node.textContent = String(error?.message ?? error);
}
