#!/bin/bash
# Uso: supervise.sh <ID> <directorio-worktree> <archivo-prompt>
# Lanza un worker de OpenCode y lo relanza (continuando su sesión) si se corta
# por un error de red o si se cuelga al arrancar. Termina cuando aparece el
# informe del worker o tras MAX intentos.
ID="$1"; DIR="$2"; PROMPT_FILE="$3"
MAX=8
LOG_DIR=/c/Users/JDMoreu/AppData/Local/Temp/claude/40010d84cecbf54d/ffb0b8cb-44a7-4d4d-8f38-01a6793a649e/scratchpad
REPORT="$DIR/rust-migration/reports/$ID-r1.md"
PROMPT="$(cat "$PROMPT_FILE")"
CONT="Continúa exactamente donde lo dejaste: la conexión se cortó. Revisa git status y git log de esta rama, ejecuta de nuevo lo que falte y termina la tarea de rust-migration/briefs/$ID.md (commit + informe rust-migration/reports/$ID-r1.md)."

cd "$DIR" || exit 1
for attempt in $(seq 1 $MAX); do
  LOG="$LOG_DIR/$ID-sup$attempt.log"
  : > "$LOG"
  if [ "$attempt" -eq 1 ]; then
    opencode run --model "opencode-go/deepseek-v4.1-flash#max" --title "$ID-s$attempt" --auto "$PROMPT" > "$LOG" 2>&1 &
  else
    opencode run --continue --model "opencode-go/deepseek-v4.1-flash#max" --auto "$CONT" > "$LOG" 2>&1 &
  fi
  PID=$!
  START=$(date +%s)
  while kill -0 $PID 2>/dev/null; do
    sleep 15
    [ -f "$REPORT" ] && { sleep 60; kill $PID 2>/dev/null; echo "[$ID] informe presente en el intento $attempt"; exit 0; }
    NOW=$(date +%s)
    SIZE=$(stat -c %s "$LOG" 2>/dev/null || echo 0)
    if [ "$SIZE" -eq 0 ] && [ $((NOW-START)) -gt 180 ]; then
      echo "[$ID] intento $attempt colgado al arrancar; lo reinicio"
      kill $PID 2>/dev/null; break
    fi
  done
  [ -f "$REPORT" ] && { echo "[$ID] terminó con informe en el intento $attempt"; exit 0; }
  echo "[$ID] intento $attempt terminó sin informe; reintento"
  sleep 10
done
echo "[$ID] sin informe tras $MAX intentos"
exit 2
