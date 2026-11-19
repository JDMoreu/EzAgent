#!/bin/bash
# Uso: supervise3.sh <ID> <directorio-worktree> <archivo-prompt> [resume]
#
# v3 (2026-11-19): como supervise2.sh, pero cada intento del worker corre en una
# terminal de herdr-core lanzada desde la terminal de quien ejecuta este script
# (normalmente el agente principal en Herdr). Así el worker aparece en Herdr como
# SUBAGENTE debajo de su agente, con nombre y etiquetas, y se puede mirar en vivo
# (clic en el árbol) en vez de quedar como un «Shell» de fondo.
#
# - Padre: la terminal que hereda este script (HC_TERMINAL_ID). Sin ella el
#   worker queda sin padre (y oculto): se avisa y se sigue.
# - Sin notas al padre (--no-notify): el seguimiento lo hace este script.
# - El worker muere con su padre (por omisión de hc).
# - EZ_AGENT=opencode|mcode: tipo de trabajador (por omisión opencode). mcode es
#   interactivo y visible; sus reintentos continúan la última sesión de la carpeta.
# - EZ_MODEL: modelo, con valor por omisión según el tipo de trabajador.
# - EZ_AGENT_BIN: ruta del programa en lugar de opencode/mcode (solo para pruebas).
# - EZ_WORKER_CMD: comando de prueba en lugar del agente (palabras separadas por
#   espacios). Sin informe completado cuenta como intento fallido.
# - EZ_MAX: número máximo de intentos (por omisión 8).
# - EZ_SHOW=tab|right|down: además del árbol, le da pestaña o panel en Herdr.
#
# Termina cuando aparece el informe del worker y el worktree queda confirmado, o
# tras MAX intentos. Los reintentos continúan la sesión del worker.
ID="$1"; DIR="$2"; PROMPT_FILE="$3"; MODE="${4:-new}"
HERE="$(cd "$(dirname "$0")" && pwd)"
MAX="${EZ_MAX:-8}"
LOG_DIR="${EZ_LOG_DIR:-${TMPDIR:-/tmp}}"
REPORT="$DIR/rust-migration/reports/$ID-r1.md"
PROMPT="$(cat "$PROMPT_FILE")"
CONT="Continúa exactamente donde lo dejaste: la ejecución se cortó. Revisa git status y git log de esta rama, ejecuta de nuevo lo que falte y termina la tarea de rust-migration/briefs/$ID.md (commit + informe rust-migration/reports/$ID-r1.md con la verificación real, sin «pendiente»)."
AGENT="${EZ_AGENT:-opencode}"
case "$AGENT" in
  opencode) DEFAULT_MODEL="opencode/hy4-flash-free" ;;
  mcode) DEFAULT_MODEL="minimax/MiniMax-M3.1-Flash" ;;
  *) echo "[$ID] aviso: agente «$AGENT» desconocido; uso opencode"; AGENT=opencode; DEFAULT_MODEL="opencode/hy4-flash-free" ;;
esac
# DECISION: EZ_MODEL explícito manda; si no, se usa el modelo por omisión del tipo.
MODEL="${EZ_MODEL:-$DEFAULT_MODEL}"
BIN="${EZ_AGENT_BIN:-$AGENT}"
NAME="$(echo "$ID" | tr '[:upper:]' '[:lower:]' | tr -c 'a-z0-9_.\n-' '-' | cut -c1-32)"

[ -z "$HC_TERMINAL_ID" ] && echo "[$ID] aviso: sin HC_TERMINAL_ID; el worker no tendrá padre en Herdr"

session_id() {
  opencode session list 2>/dev/null | awk -v t="$ID-s1" '$2 == t { print $1; exit }'
}
field() { # field <address> <python-expr sobre t>
  hc get "$1" --json 2>/dev/null | python -c "import json,sys
try:
    t=json.load(sys.stdin); print($2)
except Exception:
    print('')"
}
launch() { # launch <argv...>  → imprime la dirección de la terminal
  hc run --bg --json --name "$NAME" --cwd "$DIR" --no-notify \
    --label "ez.agent=$AGENT" --label "ez.model=$MODEL" --label ez.role=developer \
    --label "ez.brief=$ID" --label "ez.parent=${HC_TERMINAL_ID:-none}" -- "$@" \
    | python -c "import json,sys; print(json.load(sys.stdin).get('address',''))"
}
show() { # da pestaña o panel si se pidió (el padre no cambia)
  # herdr-app tarda en ver una terminal recién creada («no running terminal»):
  # se reintenta hasta ~15 s.
  case "$EZ_SHOW" in tab|right|down) ;; *) return ;; esac
  for _ in $(seq 1 30); do
    if [ "$EZ_SHOW" != "tab" ]; then
      herdr-app do pane.focus --terminal "$HC_TERMINAL_ID" >/dev/null 2>&1
    fi
    if herdr-app do terminal.keep --terminal "$1" --place "$EZ_SHOW" >/dev/null 2>&1; then
      return
    fi
    sleep 0.5
  done
  echo "[$ID] aviso: Herdr no le dio $EZ_SHOW a $1; queda solo en el árbol"
}
done_report() {
  [ -f "$REPORT" ] && grep -qiE "Estado:[^a-z]*completado" "$REPORT" \
    && ! grep -qiE "^[-*] .*: *(\*\*)?pendiente" "$REPORT"
}
wait_for_commit() { # espera hasta 15 min a que el worktree quede confirmado
  for _ in $(seq 1 60); do
    [ -z "$(git -C "$DIR" status --porcelain 2>/dev/null)" ] && return 0
    sleep 15
  done
  echo "[$ID] aviso: el worktree sigue con cambios tras 15 min; cierro el worker"
}
save_log() { hc read "$1" --lines 4000 > "$LOG" 2>&1; }
finish() {
  powershell -NoProfile -ExecutionPolicy Bypass -File "$HERE/cleanup_worker.ps1" -Dir "$DIR" >/dev/null 2>&1
  exit "$1"
}
low_priority() {
  powershell -NoProfile -Command "Get-Process opencode,opencode-pty -ErrorAction SilentlyContinue | ForEach-Object { try { \$_.PriorityClass = 'BelowNormal' } catch {} }" >/dev/null 2>&1
}

cd "$DIR" || exit 1
if [ "$MODE" = "resume" ] && [ -f "$REPORT" ]; then
  mv "$REPORT" "${REPORT%.md}.parcial.md"
fi
for attempt in $(seq 1 $MAX); do
  LOG="$LOG_DIR/$ID-hc$attempt.log"
  SESSION=""
  if [ "$AGENT" = "opencode" ] && [ "$BIN" = "opencode" ] && [ -z "$EZ_WORKER_CMD" ]; then
    SESSION="$(session_id)"
  fi
  # Una terminal terminada con el mismo nombre se retira para reutilizarlo.
  OLD="$(hc ls --all --json 2>/dev/null | python -c "import json,sys
for t in json.load(sys.stdin).get('terminals',[]):
    if t.get('name')=='$NAME' and t.get('state')=='finished': print(t['address'])")"
  for a in $OLD; do hc rm "$a" >/dev/null 2>&1; done
  if [ -n "$EZ_WORKER_CMD" ]; then
    # shellcheck disable=SC2086
    ADDR="$(launch $EZ_WORKER_CMD)"
  elif [ "$AGENT" = "opencode" ]; then
    if [ "$attempt" -eq 1 ] && [ "$MODE" != "resume" ]; then
      ADDR="$(launch "$BIN" run --model "$MODEL" --title "$ID-s1" --auto "$PROMPT")"
    elif [ -n "$SESSION" ]; then
      ADDR="$(launch "$BIN" run --session "$SESSION" --model "$MODEL" --auto "$CONT")"
    else
      echo "[$ID] no encuentro la sesión $ID-s1; empiezo una nueva"
      ADDR="$(launch "$BIN" run --model "$MODEL" --title "$ID-s1" --auto "$PROMPT")"
    fi
  elif [ "$attempt" -eq 1 ] && [ "$MODE" != "resume" ]; then
    ADDR="$(launch "$BIN" -m "$MODEL" "$PROMPT")"
  else
    ADDR="$(launch "$BIN" -c "$CONT")"
  fi
  if [ -z "$ADDR" ]; then
    echo "[$ID] hc no lanzó el intento $attempt; reintento en 30 s"; sleep 30; MODE=resume; continue
  fi
  echo "[$ID] intento $attempt en $ADDR (padre ${HC_TERMINAL_ID:-ninguno}, agente $AGENT)"
  show "$ADDR"
  START=$(date +%s)
  [ -z "$EZ_AGENT_BIN" ] && [ -z "$EZ_WORKER_CMD" ] && low_priority
  while [ "$(field "$ADDR" "t['state']")" = "live" ]; do
    sleep 15
    if done_report; then
      # Los TUI (mcode) no salen solos: se espera a que confirmen el worktree
      # (hasta 15 min) y luego se guarda la pantalla y se cierra la terminal.
      wait_for_commit
      save_log "$ADDR"; hc kill "$ADDR" >/dev/null 2>&1
      echo "[$ID] informe completado en el intento $attempt"; finish 0
    fi
    NOW=$(date +%s)
    if [ $((NOW-START)) -gt 180 ] && [ -z "$(hc read "$ADDR" --lines 5 2>/dev/null | tr -d '[:space:]')" ]; then
      echo "[$ID] intento $attempt colgado al arrancar; lo reinicio"
      hc kill "$ADDR" >/dev/null 2>&1; break
    fi
  done
  save_log "$ADDR"
  if done_report; then
    wait_for_commit
    echo "[$ID] terminó con informe completado en el intento $attempt"; finish 0
  fi
  [ -f "$REPORT" ] && mv "$REPORT" "${REPORT%.md}.parcial.md"
  MODE=resume
  echo "[$ID] intento $attempt terminó sin informe; reintento"
  sleep 10
done
echo "[$ID] sin informe completado tras $MAX intentos"
finish 2
