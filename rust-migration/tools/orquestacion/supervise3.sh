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
# - EZ_OPENCODE_MODE=interactive|run: opencode interactivo (por omisión) con su
#   interfaz y OPENCODE_CONFIG_CONTENT; `run` conserva el modo de un solo disparo.
# - EZ_AGENT_BIN: ruta del programa en lugar de opencode/mcode (solo para pruebas).
# - EZ_OPENCODE_BIN: ejecutable nativo de OpenCode; si no está se resuelve desde
#   `where.exe opencode.cmd` (C0-4: env.exe resolvería el shim bash de npm).
# - EZ_WORKER_CMD: comando de prueba en lugar del agente (palabras separadas por
#   espacios). Sin informe completado cuenta como intento fallido.
# - EZ_MAX: número máximo de intentos (por omisión 8).
# - EZ_SHOW=tab|right|down: además del árbol, le da pestaña o panel en Herdr.
# - EZ_SUBAGENTS_DIR: raíz del registro §13 (por omisión
#   %LOCALAPPDATA%/EzAgent/subagentes). Con HC_TERMINAL_ID se registra el estado
#   del trabajador y se avisa al padre con hc send; sin él, no hay registro ni avisos.
#
# Termina cuando aparece el informe del worker y el worktree queda confirmado, o
# tras MAX intentos. Los reintentos continúan la sesión del worker.
ID="$1"; DIR="$2"; PROMPT_FILE="$3"; MODE="${4:-new}"
HERE="$(cd "$(dirname "$0")" && pwd)"
if command -v python3 >/dev/null 2>&1 && python3 -c 'pass' 2>/dev/null; then PYTHON=python3; else PYTHON=python; fi
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
ENV_EXE="${EZ_ENV_EXE:-C:/Program Files/Git/usr/bin/env.exe}"
OPENCODE_MODE="${EZ_OPENCODE_MODE:-interactive}"
# C0-5: OpenCode solo acepta ids de sesión que empiezan por «ses».
# DECISION: id estable por trabajador para que los reintentos continúen la sesión.
SID="ses_$(printf '%s' "$ID" | tr '[:upper:]' '[:lower:]' | tr -cd 'a-z0-9')s1"
case "$AGENT" in
  opencode) LABEL=OpenCode ;;
  mcode) LABEL=mcode ;;
  *) LABEL="$AGENT" ;;
esac

resolve_opencode_bin() { # imprime el ejecutable real de OpenCode (C0-4)
  local shim dir bin
  if [ -n "${EZ_OPENCODE_BIN:-}" ]; then
    printf '%s\n' "$EZ_OPENCODE_BIN"; return
  fi
  shim="$(where.exe opencode.cmd 2>/dev/null | tr -d '\r' | head -1)"
  shim="${shim//\\//}"
  dir="$(dirname "$shim")"
  bin=opencode.cmd
  if [ -n "$shim" ] && [ -f "$dir/node_modules/@opencode/cli/bin/opencode.exe" ]; then
    bin="$dir/node_modules/@opencode/cli/bin/opencode.exe"
  fi
  printf '%s\n' "$bin"
}
# C0-4: env.exe resuelve `opencode` al shim bash de npm (falla: sed no encontrado).
# Resuelve el ejecutable nativo antes de lanzar; en pruebas manda EZ_AGENT_BIN.
if [ "$AGENT" = "opencode" ] && [ -z "${EZ_AGENT_BIN:-}" ] && [ -z "${EZ_WORKER_CMD:-}" ]; then
  BIN="$(resolve_opencode_bin)"
  if ! "$BIN" --version >/dev/null 2>&1; then
    echo "[$ID] aviso: «$BIN» no responde a --version; uso el modo run"
    OPENCODE_MODE=run
  fi
fi
# DECISION: EZ_RESOLVE_OPENCODE=1 solo informa de la resolución y sale, para que
# las pruebas verifiquen C0-4 sin lanzar un agente real.
if [ "${EZ_RESOLVE_OPENCODE:-0}" = "1" ]; then
  printf 'BIN=%s\nMODE=%s\nSID=%s\n' "$BIN" "$OPENCODE_MODE" "$SID"
  exit 0
fi

[ -z "$HC_TERMINAL_ID" ] && echo "[$ID] aviso: sin HC_TERMINAL_ID; el worker no tendrá padre en Herdr ni registro o avisos"

session_id() {
  "$BIN" session list 2>/dev/null | awk -v t="$SID" '$2 == t { print $1; exit }'
}
field() { # field <address> <python-expr sobre t>
  hc get "$1" --json 2>/dev/null | $PYTHON -c "import json,sys
try:
    t=json.load(sys.stdin); print($2)
except Exception:
    print('')"
}
report_summary() { # línea «Estado: …» o primera viñeta de «## Resultado» (máx. 300)
  [ -f "$REPORT" ] || return 0
  "$PYTHON" - "$REPORT" <<'PY' 2>/dev/null
import sys, re
lines = open(sys.argv[1], encoding='utf-8', errors='replace').read().splitlines()
s = ''
for ln in lines:
    if re.search(r'estado\s*:', ln, re.I):
        s = re.sub(r'^\s*[-*]\s+', '', ln).strip(); break
if not s:
    inres = False
    for ln in lines:
        if re.match(r'\s*##\s*resultado', ln, re.I): inres = True; continue
        if inres and re.match(r'\s*[-*]\s+\S', ln):
            s = re.sub(r'^\s*[-*]\s+', '', ln).strip(); break
print(' '.join(s.split())[:300])
PY
}
record() { # record <state> <detail> <summary> <report> <send:1|0> — registro §13 + aviso al padre
  [ -z "${HC_TERMINAL_ID:-}" ] && return 0
  local state="$1" detail="$2" summary="$3" report="$4" do_send="$5"
  local root pdir reg tmp notice sent=0 notify
  root="${EZ_SUBAGENTS_DIR:-${LOCALAPPDATA:-$USERPROFILE/AppData/Local}/EzAgent/subagentes}"
  pdir="$(printf '%s' "$HC_TERMINAL_ID" | tr '/' '_')"
  reg="$root/$pdir/$NAME.json"
  tmp="$reg.tmp.$$"
  case "$state" in
    en_proceso) notice="[EzAgent · aviso] $NAME ($LABEL · $MODEL) en proceso." ;;
    terminado) notice="[EzAgent · aviso] $NAME ($LABEL · $MODEL) terminó. Resumen del subagente: «$summary». Informe: $report. Para validarlo: ez validate $NAME --pass|--fail --file <evidencia>" ;;
    fallido) notice="[EzAgent · aviso] $NAME ($LABEL · $MODEL) falló: $detail. Revisa su pantalla: hc read ${ADDR:-$NAME}" ;;
  esac
  if [ "$do_send" -eq 1 ]; then
    if hc send "$HC_TERMINAL_ID" "$notice" >/dev/null 2>&1; then sent=1; else echo "[$ID] aviso: no pude avisar al padre ($HC_TERMINAL_ID)"; fi
  fi
  if [ "$sent" -eq 1 ]; then notify="$state"; else notify=__KEEP__; fi
  # DECISION: un solo escritor en este flujo; temp+rename atómico basta (el
  # candado §13 es para el CLI `ez`, con varios escritores concurrentes).
  STATE="$state" DETAIL="$detail" SUMMARY="$summary" RPT="$report" NOTIFY="$notify" \
  REG="$reg" TMP="$tmp" NAME="$NAME" TERMINAL="${ADDR:-}" PARENT="$HC_TERMINAL_ID" \
  AGENT_KIND="$AGENT" LABEL_TXT="$LABEL" MODEL_TXT="$MODEL" CWD_TXT="$DIR" \
  BRIEF_HEAD="$(printf '%s' "$PROMPT" | cut -c1-200)" \
  "$PYTHON" - <<'PY' 2>/dev/null || echo "[$ID] aviso: no pude escribir el registro $reg"
import os, json, time
reg, tmp = os.environ['REG'], os.environ['TMP']
try:
    d = json.load(open(reg, encoding='utf-8'))
except Exception:
    d = {}
now = int(time.time() * 1000)
state = os.environ['STATE']
d.update(name=os.environ['NAME'], terminal=os.environ.get('TERMINAL', ''),
    parent=os.environ['PARENT'], agent=os.environ['AGENT_KIND'],
    label=os.environ['LABEL_TXT'], model=os.environ['MODEL_TXT'],
    effort='', role='developer', cwd=os.environ['CWD_TXT'],
    brief_head=os.environ['BRIEF_HEAD'], state=state, since_ms=now,
    summary=os.environ['SUMMARY'], report=os.environ['RPT'],
    validation=d.get('validation') or {'by':None,'verdict':None,'summary':None,'file':None,'at_ms':None})
if os.environ['NOTIFY'] != '__KEEP__': d['notified'] = os.environ['NOTIFY']
d.setdefault('history', []).append({'state':state,'at_ms':now,'by':'supervisor','detail':os.environ['DETAIL']})
os.makedirs(os.path.dirname(reg), exist_ok=True)
with open(tmp, 'w', encoding='utf-8') as f: json.dump(d, f, ensure_ascii=False, indent=2)
os.replace(tmp, reg)
PY
}
launch() { # launch <argv...>  → imprime la dirección de la terminal
  hc run --bg --json --name "$NAME" --cwd "$DIR" --no-notify \
    --label "ez.agent=$AGENT" --label "ez.model=$MODEL" --label ez.role=developer \
    --label "ez.brief=$ID" --label "ez.parent=${HC_TERMINAL_ID:-none}" -- "$@" \
    | $PYTHON -c "import json,sys; print(json.load(sys.stdin).get('address',''))"
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
terminal_state() { # live|finished|unknown — C0-3: un get vacío/fallido no es «finished»
  local s
  s="$(field "$1" "str(t.get('state','') or '')")"
  case "$s" in
    live|finished) printf '%s\n' "$s" ;;
    *) printf '%s\n' unknown ;;
  esac
}
free_name() { # C0-3: ninguna terminal con este nombre queda viva antes de lanzar
  local st a rows
  rows="$(hc ls --all --json 2>/dev/null | $PYTHON -c "
import json,sys
try: ts=json.load(sys.stdin).get('terminals',[])
except Exception: ts=[]
for t in ts:
    if t.get('name')=='$NAME': print((t.get('state') or ''), t.get('address',''))" 2>/dev/null)"
  while read -r st a; do
    [ -n "$a" ] || continue
    if [ "$st" = "live" ]; then hc kill "$a" >/dev/null 2>&1; fi
    hc rm "$a" >/dev/null 2>&1
  done <<< "$rows"
}
succeed() { # <address> <attempt> — informe completo: confirma, registra, guarda y cierra
  wait_for_commit
  record terminado "" "$(report_summary)" "$REPORT" 1
  save_log "$1"
  hc kill "$1" >/dev/null 2>&1
  echo "[$ID] informe completado en el intento $2"
  finish 0
}

cd "$DIR" || exit 1
if [ "$MODE" = "resume" ] && [ -f "$REPORT" ]; then
  mv "$REPORT" "${REPORT%.md}.parcial.md"
fi
for attempt in $(seq 1 $MAX); do
  LOG="$LOG_DIR/$ID-hc$attempt.log"
  SESSION=""
  if [ "$AGENT" = "opencode" ] && [ "$OPENCODE_MODE" = "run" ] && [ -z "${EZ_AGENT_BIN:-}" ] && [ -z "$EZ_WORKER_CMD" ]; then
    SESSION="$(session_id)"
  fi
  free_name
  if [ -n "$EZ_WORKER_CMD" ]; then
    # shellcheck disable=SC2086
    ADDR="$(launch $EZ_WORKER_CMD)"
  elif [ "$AGENT" = "opencode" ] && [ "$OPENCODE_MODE" = "interactive" ]; then
    # Sin --model: el modelo va en OPENCODE_CONFIG_CONTENT. env.exe fija la
    # variable y ejecuta el programa con argv en lista, sin shell.
    TEXT="$PROMPT"
    [ "$attempt" -gt 1 ] && TEXT="$CONT"
    ADDR="$(launch "$ENV_EXE" "OPENCODE_CONFIG_CONTENT={\"model\":\"$MODEL\"}" \
      "$BIN" --standalone --auto --session "$SID" --prompt "$TEXT")"
  elif [ "$AGENT" = "opencode" ]; then
    # EZ_OPENCODE_MODE=run: modo anterior de un solo disparo.
    if [ "$attempt" -eq 1 ] && [ "$MODE" != "resume" ]; then
      ADDR="$(launch "$BIN" run --model "$MODEL" --title "$SID" --auto "$PROMPT")"
    elif [ -n "$SESSION" ]; then
      ADDR="$(launch "$BIN" run --session "$SESSION" --model "$MODEL" --auto "$CONT")"
    else
      echo "[$ID] no encuentro la sesión $SID; empiezo una nueva"
      ADDR="$(launch "$BIN" run --model "$MODEL" --title "$SID" --auto "$PROMPT")"
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
  [ "$attempt" -eq 1 ] && record en_proceso "" "" "" 1
  show "$ADDR"
  START=$(date +%s)
  UNKNOWN_SINCE=""
  [ -z "$EZ_AGENT_BIN" ] && [ -z "$EZ_WORKER_CMD" ] && low_priority
  while :; do
    st="$(terminal_state "$ADDR")"
    case "$st" in
      live)
        UNKNOWN_SINCE=""
        sleep 15
        done_report && succeed "$ADDR" "$attempt"
        NOW=$(date +%s)
        if [ $((NOW-START)) -gt 180 ] && [ -z "$(hc read "$ADDR" --lines 5 2>/dev/null | tr -d '[:space:]')" ]; then
          echo "[$ID] intento $attempt colgado al arrancar; lo reinicio"
          hc kill "$ADDR" >/dev/null 2>&1; break
        fi
        ;;
      finished) break ;;
      *)
        # C0-3: un hc get fallido o vacío es «desconocido», no «terminado».
        if [ -z "$UNKNOWN_SINCE" ]; then
          UNKNOWN_SINCE=$(date +%s)
          echo "[$ID] aviso: estado desconocido de $ADDR; reintento leyendo hasta 2 min"
        fi
        done_report && succeed "$ADDR" "$attempt"
        NOW=$(date +%s)
        if [ $((NOW-UNKNOWN_SINCE)) -gt 120 ]; then
          echo "[$ID] estado desconocido de $ADDR tras 2 min; sigo con el siguiente intento"
          break
        fi
        sleep 5
        ;;
    esac
  done
  save_log "$ADDR"
  done_report && succeed "$ADDR" "$attempt"
  [ -f "$REPORT" ] && mv "$REPORT" "${REPORT%.md}.parcial.md"
  MODE=resume
  echo "[$ID] intento $attempt terminó sin informe; reintento"
  sleep 10
done
echo "[$ID] sin informe completado tras $MAX intentos"
record fallido "sin informe completado tras $MAX intentos" "" "" 1
finish 2
