// Nota de voz de la barra: grabar, transcribir y dejar el texto sin enviarlo.

import { TEXTS } from "./texts.js";

const { invoke } = window.__TAURI__.core;
const { listen } = window.__TAURI__.event;

const byId = (id) => document.getElementById(id);

let recording = false;
let notice = () => {};

function idle() {
  recording = false;
  const button = byId("btn-voice");
  button.textContent = TEXTS.voiceNote;
  button.disabled = false;
  button.classList.remove("recording");
  byId("voice-meter").hidden = true;
  byId("voice-level-fill").style.width = "0%";
}

function active() {
  recording = true;
  const button = byId("btn-voice");
  button.textContent = TEXTS.stop;
  button.classList.add("recording");
  byId("voice-meter").hidden = false;
}

function working() {
  recording = false;
  const button = byId("btn-voice");
  button.textContent = TEXTS.transcribing;
  button.disabled = true;
  byId("voice-meter").hidden = true;
}

async function insertText(texto) {
  const field = byId("input-text");
  const cursor = field.selectionStart ?? field.value.length;
  const result = await invoke("voice_insert", {
    current: field.value,
    cursor,
    added: texto,
  });
  field.value = result.text;
  field.focus();
  field.setSelectionRange(result.cursor, result.cursor);
}

async function toggle() {
  if (recording) {
    working();
    try {
      const answer = await invoke("voice_note_stop");
      await insertText(answer.texto);
      notice("");
    } catch (error) {
      notice(String(error));
    } finally {
      idle();
    }
    return;
  }
  try {
    await invoke("voice_note_start");
    active();
    notice("");
  } catch (error) {
    idle();
    notice(String(error));
  }
}

async function finished(event) {
  const payload = event.payload ?? {};
  working();
  try {
    if (payload.error) {
      notice(String(payload.error));
      return;
    }
    await insertText(payload.texto ?? "");
    notice("");
  } catch (error) {
    notice(String(error));
  } finally {
    idle();
  }
}

function onLevel(event) {
  if (!recording) return;
  const level = Number(event.payload?.level ?? 0);
  const width = Math.max(0, Math.min(100, level * 1000));
  byId("voice-level-fill").style.width = `${width}%`;
}

export function wireVoice(report) {
  notice = report;
  byId("btn-voice").addEventListener("click", toggle);
  listen("hub://voice-level", onLevel).catch(() => {});
  listen("hub://voice-note-done", finished).catch(() => {});
}
