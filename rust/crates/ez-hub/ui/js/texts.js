// Todos los textos de la vista, en español neutro latinoamericano.

export const TEXTS = {
  appTitle: "EzAgent",
  spacesTitle: "Espacios",
  newSpace: "+ Nuevo espacio",
  name: "Nombre",
  folder: "Carpeta",
  elephant: "Elefante",
  create: "Crear",
  cancel: "Cancelar",
  send: "Enviar",
  voiceNote: "Nota de voz",
  stop: "Detener",
  transcribing: "Transcribiendo…",
  call: "Llamar",
  soon: "Disponible pronto",
  placeholder: "Escribe o dicta…",
  fishTab: "Peces",
  traceTab: "Traza",
  timeline: "Traza",
  cards: "Aprobaciones",
  emptySpaces: "Todavía no hay espacios. Crea uno para empezar.",
  emptyFish: "Todavía no hay peces en este espacio.",
  emptyTimeline: "Todavía no hay eventos.",
  emptyCards: "No hay tarjetas pendientes.",
  noSpace: "Elige o crea un espacio para empezar.",
  elephantOf: (agent) => `Elefante · ${agent}`,
  agentNames: { claude: "Claude Code", opencode: "OpenCode", codex: "Codex", powershell: "PowerShell" },
  statuses: {
    idle: "inactivo",
    working: "trabajando",
    done: "listo",
    blocked: "bloqueado",
    error: "error",
  },
  approve: "Aprobar",
  reject: "Rechazar",
  rejectHint: "Comentario para el elefante (obligatorio)",
  rejectSend: "Enviar rechazo",
  exited: (code) => `[proceso terminado: código ${code}]`,
  notice: {
    emptyInput: "Escribe algo antes de enviar.",
    needFolder: "Escribe una carpeta existente.",
    inputFailed: "No pude enviar el texto al elefante.",
  },
};

export function agentLabel(id) {
  return TEXTS.agentNames[id] ?? id;
}

export function statusLabel(status) {
  return TEXTS.statuses[status] ?? status;
}
