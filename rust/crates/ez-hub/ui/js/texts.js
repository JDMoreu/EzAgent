// Todos los textos de la vista y la traducción de la traza, en un solo lugar.
// Español neutro latinoamericano (§15.9).

export const TEXTS = {
  appTitle: "EzAgent",
  spacesTitle: "Espacios",
  newSpace: "+ Espacio",
  newMain: "+ Agente principal",
  name: "Nombre",
  folder: "Carpeta",
  agent: "Agente",
  model: "Modelo",
  effort: "Esfuerzo",
  effortHint: "Opcional",
  create: "Crear",
  cancel: "Cancelar",
  send: "Enviar",
  voiceNote: "Nota de voz",
  stop: "Detener",
  transcribing: "Transcribiendo…",
  call: "Llamar",
  soon: "Disponible pronto",
  placeholder: "Escribe o dicta…",
  split: "Dividir",
  join: "Unir",
  traceTab: "Traza",
  cardsTab: "Tarjetas",
  collapse: "Ocultar panel",
  expand: "Mostrar panel",
  emptyTree: "Todavía no hay agentes principales en este espacio.",
  emptyTrace: "Todavía no hay eventos.",
  emptyCards: "No hay tarjetas pendientes.",
  noSpace: "Elige o crea un espacio para empezar.",
  noMain: "Crea un agente principal en el espacio seleccionado.",
  needFolder: "Escribe una carpeta existente.",
  needName: "Escribe un nombre.",
  needModel: "Escribe un modelo.",
  sendFailed: "No pude enviar el texto al agente activo.",
  approve: "Aprobar",
  reject: "Rechazar",
  rejectHint: "Comentario para el agente principal (obligatorio)",
  rejectSend: "Enviar rechazo",
  resume: "Reanudar",
  moreTime: "Dar más margen",
  stopTask: "Detener",
  integrate: (branch) => `Integrar en ${branch}`,
  keepBranch: "Dejar en su rama",
  exited: (code) => `[proceso terminado: código ${code}]`,
};

export const AGENT_NAMES = {
  claude: "Claude Code",
  opencode: "OpenCode",
  codex: "Codex",
  powershell: "PowerShell",
};

const STATUSES = {
  idle: "inactivo",
  working: "trabajando",
  done: "listo",
  blocked: "bloqueado",
  error: "error",
};

export function agentLabel(id) {
  return AGENT_NAMES[id] ?? id ?? "";
}

export function statusLabel(status) {
  return STATUSES[status] ?? status ?? "";
}

// Traducción de un evento de la traza (el mismo mapa que texts.rs).
export function traceText(event) {
  if (!event) return "";
  if (event.texto) return event.texto;
  const text = (key) => event[key] ?? "";
  const number = (key) => event[key] ?? 0;
  switch (event.type) {
    case "task_proposed":
      return `Tarea propuesta: ${text("title")}`;
    case "task_approved":
      return "Tarea aprobada";
    case "team_proposed":
      return "Equipo propuesto";
    case "team_approved":
      return "Equipo aprobado";
    case "round_started":
      return `Ronda ${number("round")}`;
    case "fish_launched":
      return `${text("role")} ${text("fish")} en marcha (${text("agente")} · ${text("modelo")})`;
    case "fish_reported":
      return `${text("fish")}: ${text("verdict") === "pass" ? "pasa" : "no pasa"} (${number("blocking")} bloqueantes, ${number("major")} mayores)`;
    case "fish_blocked":
      return `${text("fish")} espera una respuesta (${text("motivo")})`;
    case "fish_failed":
      return `${text("fish")} falló: ${text("reason")}`;
    case "verification":
      return `Verificación: ${number("aprobados")}/${number("total")} criterios`;
    case "round_closed":
      return event.clean
        ? `Ronda ${number("round")} limpia`
        : `Ronda ${number("round")} no limpia: ${text("motivos")}`;
    case "paused":
      return `En pausa: ${text("motivo")}`;
    case "resumed":
      return "Reanudada";
    case "task_closed":
      return "Tarea terminada";
    case "task_stopped":
      return "Tarea detenida";
    case "integrated":
      return "Rama integrada";
    case "rejected":
      return `Rechazado (${text("rule")}): ${text("message")}`;
    default:
      return event.type ?? "";
  }
}
