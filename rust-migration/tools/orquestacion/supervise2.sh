#!/bin/bash
# Uso: supervise2.sh <ID> <directorio-worktree> <archivo-prompt> [resume]
# Lanza un worker de OpenCode y lo relanza (continuando SU sesión) si se corta
# por un error de red o si se cuelga al arrancar. Termina cuando aparece el
# informe del worker o tras MAX intentos.
#
# Las sesiones de OpenCode son globales (no por carpeta): `--continue` retoma la
# última de cualquier worker. Por eso los reintentos usan `--session <id>` con el
# id de la sesión titulada «<ID>-s1».
#
# Con «resume», la primera vuelta ya continúa la sesión existente «<ID>-s1»
# (tras un cierre de Claude Code, por ejemplo). Un informe previo de la ronda se
# aparta a «<ID>-r1.parcial.md» para que no cuente como terminado.
ID="$1"; DIR="$2"; PROMPT_FILE="$3"; MODE="${4:-new}"
# v2: el informe solo cuenta si dice «Estado: completado» (los workers escriben
# a veces un informe provisional), al terminar se cierran los procesos que el
# worker dejó en su carpeta y OpenCode corre con prioridad baja.
HERE="$(cd "$(dirname "$0")" && pwd)"
MAX=8
LOG_DIR=/c/Users/JDMoreu/AppData/Local/Temp/claude/40010d84cecbf54d/ffb0b8cb-44a7-4d4d-8f38-01a6793a649e/scratchpad
REPORT="$DIR/rust-migration/reports/$ID-r1.md"
PROMPT="$(cat "$PROMPT_FILE")"
CONT="Continúa exactamente donde lo dejaste: la ejecución se cortó. Revisa git status y git log de esta rama, ejecuta de nuevo lo que falte y termina la tarea de rust-migration/briefs/$ID.md (commit + informe rust-migration/reports/$ID-r1.md con la verificación real, sin «pendiente»)."
MODEL="opencode-go/deepseek-v4.1-flash#max"

session_id() {
  opencode session list 2>/dev/null | awk -v t="$ID-s1" '$2 == t { print $1; exit }'
}

cd "$DIR" || exit 1
done_report() {
  [ -f "$REPORT" ] && grep -qiE "Estado:[^a-z]*completado" "$REPORT"
}
finish() {
  powershell -NoProfile -ExecutionPolicy Bypass -File "$HERE/cleanup_worker.ps1" -Dir "$DIR" >/dev/null 2>&1
  exit "$1"
}
low_priority() {
  powershell -NoProfile -Command "Get-Process opencode,opencode-pty -ErrorAction SilentlyContinue | ForEach-Object { try { \$_.PriorityClass = 'BelowNormal' } catch {} }" >/dev/null 2>&1
}
if [ "$MODE" = "resume" ] && [ -f "$REPORT" ]; then
  mv "$REPORT" "${REPORT%.md}.parcial.md"
fi
for attempt in $(seq 1 $MAX); do
  LOG="$LOG_DIR/$ID-sup$attempt.log"
  [ "$MODE" = "resume" ] && LOG="$LOG_DIR/$ID-resume$attempt.log"
  : > "$LOG"
  SESSION="$(session_id)"
  if [ "$attempt" -eq 1 ] && [ "$MODE" != "resume" ]; then
    opencode run --model "$MODEL" --title "$ID-s1" --auto "$PROMPT" > "$LOG" 2>&1 &
  elif [ -n "$SESSION" ]; then
    opencode run --session "$SESSION" --model "$MODEL" --auto "$CONT" > "$LOG" 2>&1 &
  else
    echo "[$ID] no encuentro la sesión $ID-s1; empiezo una nueva"
    opencode run --model "$MODEL" --title "$ID-s1" --auto "$PROMPT" > "$LOG" 2>&1 &
  fi
  PID=$!
  START=$(date +%s)
  low_priority
  while kill -0 $PID 2>/dev/null; do
    sleep 15
    if done_report; then
      # El worker suele seguir ajustando tras escribir el informe: se le deja
      # terminar por su cuenta (hasta 15 min) en vez de cortarlo a medias.
      for _ in $(seq 1 60); do kill -0 $PID 2>/dev/null || break; sleep 15; done
      kill $PID 2>/dev/null
      echo "[$ID] informe completado en el intento $attempt"; finish 0
    fi
    NOW=$(date +%s)
    SIZE=$(stat -c %s "$LOG" 2>/dev/null || echo 0)
    if [ "$SIZE" -eq 0 ] && [ $((NOW-START)) -gt 180 ]; then
      echo "[$ID] intento $attempt colgado al arrancar; lo reinicio"
      kill $PID 2>/dev/null; break
    fi
  done
  done_report && { echo "[$ID] terminó con informe completado en el intento $attempt"; finish 0; }
  [ -f "$REPORT" ] && mv "$REPORT" "${REPORT%.md}.parcial.md"
  # El siguiente intento continúa la sesión del worker.
  MODE=resume
  echo "[$ID] intento $attempt terminó sin informe; reintento"
  sleep 10
done
echo "[$ID] sin informe completado tras $MAX intentos"
finish 2
