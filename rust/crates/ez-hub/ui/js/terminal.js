// Una vista xterm.js por terminal PTY. Los bytes crudos llegan por un Channel.

import { Terminal } from "../vendor/xterm/lib/xterm.mjs";
import { FitAddon } from "../vendor/xterm-addon-fit/lib/addon-fit.mjs";
import { TEXTS } from "./texts.js";

const { invoke, Channel } = window.__TAURI__.core;

// Terminales que recibieron al menos un bloque de bytes del PTY.
const withData = new Set();

/** Cuántas terminales montadas recibieron datos (para la autocomprobación). */
export function terminalsWithData() {
  return withData.size;
}

const THEME = {
  background: "#0f1115",
  foreground: "#e6e9ef",
  cursor: "#7aa2f7",
  black: "#0f1115",
  red: "#f7768e",
  green: "#9ece6a",
  yellow: "#e0af68",
  blue: "#7aa2f7",
  magenta: "#bb9af7",
  cyan: "#7dcfff",
  white: "#e6e9ef",
  brightBlack: "#8b93a7",
  brightRed: "#f7768e",
  brightGreen: "#9ece6a",
  brightYellow: "#e0af68",
  brightBlue: "#7aa2f7",
  brightMagenta: "#bb9af7",
  brightCyan: "#7dcfff",
  brightWhite: "#ffffff",
};

/** Monta una terminal y devuelve su vista, o `null` si el programa no existe. */
export async function mountTerminal(host, { argv, cwd, space }) {
  const term = new Terminal({
    fontFamily: '"Cascadia Mono", Consolas, monospace',
    fontSize: 12,
    lineHeight: 1.2,
    cursorBlink: true,
    theme: THEME,
    scrollback: 4000,
  });
  const fit = new FitAddon();
  term.loadAddon(fit);
  term.open(host);
  try {
    fit.fit();
  } catch {
    // El panel todavía no tiene tamaño; se reajusta al mostrarlo.
  }

  let viewId;
  let gotData = false;
  const channel = new Channel();
  channel.onmessage = (message) => {
    if (message instanceof ArrayBuffer) {
      gotData = true;
      term.write(new Uint8Array(message));
    } else if (message instanceof Uint8Array) {
      gotData = true;
      term.write(message);
    } else if (message && message.event === "exit") {
      term.write(`\r\n${TEXTS.exited(message.code)}\r\n`);
    }
    if (gotData && viewId) withData.add(viewId);
  };

  try {
    viewId = await invoke("view_open", {
      argv,
      cwd,
      cols: term.cols,
      rows: term.rows,
      channel,
      space,
    });
    if (gotData) withData.add(viewId);
  } catch (error) {
    term.dispose();
    host.replaceChildren();
    const message = document.createElement("div");
    message.className = "terminal-error";
    message.textContent = String(error);
    host.appendChild(message);
    return null;
  }

  term.onData((data) => {
    invoke("view_write", { id: viewId, data }).catch((error) =>
      term.write(`\r\n[${String(error)}]\r\n`),
    );
  });
  term.onResize(({ cols, rows }) => {
    invoke("view_resize", { id: viewId, cols, rows }).catch(() => {});
  });

  return {
    term,
    id: viewId,
    fit: () => {
      try {
        fit.fit();
      } catch {
        // El host está oculto; se reintentará al mostrarlo.
      }
    },
    close: async () => {
      withData.delete(viewId);
      term.dispose();
      await invoke("view_close", { id: viewId });
    },
  };
}
