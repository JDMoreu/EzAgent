#!/bin/bash
# Uso: supervise3.sh <ID> <directorio-worktree> <archivo-prompt> [resume]
#
# v3 (2026-11-19): como supervise2.sh, pero cada intento del worker corre en una
# terminal de herdr-core lanzada desde la terminal de quien ejecuta este script
# (normalmente el agente principal en Herdr). Así el worker aparece en Herdr como
# SUBAGENTE debajo de su agente, con nombre y etiquetas, y se puede mirar en vivo
# (clic en el árbol) en vez de quedar como un «Shell» de fondo.
#
# v4 (2026-11-19, HD-0d): corrige tres fallos del v3 que se vieron en uso real en
# Windows (8 intentos en ~25 min; cada trabajador cortado a los 2-3 min):
#   - el \r que Python (y el resto de herramientas de Windows) ponen al final de
#     sus líneas ya no llega a los datos: todo lo que el script compara o usa
#     como dato pasa por un único ayudante (strip_cr). Antes, «live\r» no era
#     «live» y el supervisor tomaba una terminal sana por «desconocida» (C0-3),
#     esperaba 2 min y mataba al trabajador sano con free_name.
#   - una terminal viva NO se mata por un estado «desconocido»: a los 2 min el
#     supervisor sigue esperando (y lo dice cada 5 min). free_name solo retira
#     terminales terminadas: una terminal viva con el mismo nombre significa que
#     el trabajador sigue, así que no se lanza otro intento.
#   - C0-6: si la pantalla del worker muestra un aviso de límite de uso, el
#     intento se registra como «fallido» con ese motivo, se avisa al padre y no
#     se reintenta con el mismo modelo. Si EZ_MODEL cambió respecto de la sesión
#     guardada, se abre una sesión nueva (no se continúa la vieja).
#
# v5 (2026-11-19, HD-0e): C0-6 ya no da falsos positivos. El aviso de límite de
#   uso solo cuenta si el agente NO está «working», la pantalla no cambió entre
#   dos lecturas seguidas (≥ 15 s) y la frase COMPLETA está en sus últimas 15
#   líneas («Free limit reached», «5 hour usage limit reached», «You've hit your
#   usage limit», «Usage limit reached», «rate limit reached», «quota exceeded»).
#   Palabras sueltas («quota», «rate limit», «usage limit») ya no cuentan: un
#   agente que lee o escribe sobre límites de uso no está bloqueado
#   (§3 de docs/designs/novedades-subagentes.md).
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
# - EZ_PYTHON: intérprete de Python (por omisión python3 o python). Las pruebas
#   apuntan a un envoltorio que devuelve \r\n como en Windows (HD-0d).
# - EZ_UNKNOWN_GRACE / EZ_UNKNOWN_EVERY: segundos de estado «desconocido» antes
#   de avisar que se sigue esperando (por omisión 120) y cada cuánto se repite
#   el aviso (por omisión 300).
#
# Termina cuando aparece el informe del worker y el worktree queda confirmado, o
# tras MAX intentos. Los reintentos continúan la sesión del worker.
ID="$1"; DIR="$2"; PROMPT_FILE="$3"; MODE="${4:-new}"
HERE="$(cd "$(dirname "$0")" && pwd)"
# DECISION: EZ_PYTHON manda; si no, python3 o python, el primero que funcione.
if [ -n "${EZ_PYTHON:-}" ]; then
  PYTHON="$EZ_PYTHON"
elif command -v python3 >/dev/null 2>&1 && python3 -c 'pass' 2>/dev/null; then PYTHON=python3; else PYTHON=python; fi
# ÚNICO ayudante para el \r de Windows: todo lo que el script compara o usa
# como dato pasa por aquí. Sin él, «live\r» no es «live» (C0-3) y el supervisor
# confunde una terminal sana con una desconocida (HD-0d).
strip_cr() { tr -d '\r'; }
MAX="${EZ_MAX:-8}"
LOG_DIR="${EZ_LOG_DIR:-${TMPDIR:-/tmp}}"
REPORT="$DIR/rust-migration/reports/$ID-r1.md"
PROMPT="$(cat "$PROMPT_FILE" | strip_cr)"
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
UNKNOWN_GRACE="${EZ_UNKNOWN_GRACE:-120}"
UNKNOWN_EVERY="${EZ_UNKNOWN_EVERY:-300}"
# HD-0e (C0-6): cómo se decide que hay un límite de uso. §3 del diseño.
# DECISION: las 6 frases completas de §3 son el patrón; las palabras sueltas
# («quota», «rate limit», «usage limit») quedan fuera a propósito.
LIMIT_TAIL_LINES="${EZ_LIMIT_TAIL_LINES:-15}"
LIMIT_STABLE_SECS="${EZ_LIMIT_STABLE_SECS:-15}"
LIMIT_PATTERNS="free limit reached|5 hour usage limit reached|you've hit your usage limit|usage limit reached|rate limit reached|quota exceeded"
# Pantalla de la lectura anterior: para saber si lleva quieta (HD-0e).
PREV_ADDR=""
PREV_TAIL=""
PREV_TAIL_AT=0
# Frase de límite detectada en la última vuelta (HD-0e).
LIMIT_HIT=""
SUBAGENTS_ROOT="${EZ_SUBAGENTS_DIR:-${LOCALAPPDATA:-$USERPROFILE/AppData/Local}/EzAgent/subagents}"
# C0-5: OpenCode solo acepta ids de sesión que empiezan por «ses».
# DECISION: id estable por trabajador para que los reintentos continúen la sesión.
SID_PREFIX="ses_$(printf '%s' "$ID" | tr '[:upper:]' '[:lower:]' | tr -cd 'a-z0-9')"
SID_N=1
SID="${SID_PREFIX}s1"
# C0-6: la sesión guarda el modelo con que se creó; si EZ_MODEL cambia, se abre
# una sesión nueva en vez de continuar la vieja (que ignora el modelo nuevo).
SESSION_STATE="$SUBAGENTS_ROOT/sessions/$NAME.state"
if [ -f "$SESSION_STATE" ]; then
  PREV_SID="$(sed -n 's/^sid=//p' "$SESSION_STATE" 2>/dev/null | head -1)"
  PREV_MODEL="$(sed -n 's/^model=//p' "$SESSION_STATE" 2>/dev/null | head -1)"
  PREV_N="$(sed -n 's/^n=//p' "$SESSION_STATE" 2>/dev/null | head -1)"
  case "$PREV_N" in ''|*[!0-9]*) PREV_N=1 ;; esac
  if [ -n "$PREV_MODEL" ] && [ "$PREV_MODEL" != "$MODEL" ]; then
    SID_N=$((PREV_N + 1))
    SID="${SID_PREFIX}s${SID_N}"
    echo "[$ID] aviso: el modelo cambió ($PREV_MODEL -> $MODEL); abro una sesión nueva ($SID)"
  elif [ -n "$PREV_SID" ]; then
    SID="$PREV_SID"
    SID_N="$PREV_N"
  fi
fi
save_session() { # save_session <sid> <model> — recuerda la sesión y su modelo
  mkdir -p "$(dirname "$SESSION_STATE")" 2>/dev/null || return 0
  printf 'sid=%s\nmodel=%s\nn=%s\n' "$1" "$2" "$SID_N" > "$SESSION_STATE" 2>/dev/null || true
}
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
  shim="$(where.exe opencode.cmd 2>/dev/null | strip_cr | head -1)"
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
  "$BIN" session list 2>/dev/null | awk -v t="$SID" '$2 == t { print $1; exit }' | strip_cr
}
field() { # field <address> <python-expr sobre t>
  hc get "$1" --json 2>/dev/null | $PYTHON -c "import json,sys
try:
    t=json.load(sys.stdin); print($2)
except Exception:
    print('')" | strip_cr
}
report_summary() { # línea «Estado: …» o primera viñeta de «## Resultado» (máx. 300)
  [ -f "$REPORT" ] || return 0
  "$PYTHON" - "$REPORT" <<'PY' 2>/dev/null | strip_cr
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
  root="$SUBAGENTS_ROOT"
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
    | $PYTHON -c "import json,sys; print(json.load(sys.stdin).get('address',''))" | strip_cr
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
  [ -f "$REPORT" ] || return 1
  tr -d '\r' < "$REPORT" | grep -qiE "Estado:[^a-z]*completado" \
    && ! tr -d '\r' < "$REPORT" | grep -qiE "^[-*] .*: *(\*\*)?pendiente"
}
wait_for_commit() { # espera hasta 15 min a que el worktree quede confirmado
  for _ in $(seq 1 60); do
    [ -z "$(git -C "$DIR" status --porcelain 2>/dev/null)" ] && return 0
    sleep 15
  done
  echo "[$ID] aviso: el worktree sigue con cambios tras 15 min; cierro el worker"
}
save_log() { hc read "$1" --lines 4000 2>&1 | strip_cr > "$LOG"; }
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
named_terminals() { # líneas «estado dirección» de las terminales con este nombre
  hc ls --all --json 2>/dev/null | $PYTHON -c "
import json,sys
try: ts=json.load(sys.stdin).get('terminals',[])
except Exception: ts=[]
for t in ts:
    if t.get('name')=='$NAME': print((t.get('state') or ''), t.get('address',''))" 2>/dev/null | strip_cr
}
free_name() { # HD-0d: solo retira terminales TERMINADAS; una viva no se toca
  local st a
  while read -r st a; do
    [ -n "$a" ] || continue
    [ "$st" = "live" ] && continue
    hc rm "$a" >/dev/null 2>&1
  done <<< "$(named_terminals)"
}
live_addr() { # dirección de una terminal VIVA con este nombre (vacío si no hay)
  local st a
  while read -r st a; do
    [ -n "$a" ] || continue
    if [ "$st" = "live" ]; then printf '%s\n' "$a"; return 0; fi
  done <<< "$(named_terminals)"
  return 1
}
agent_state() { # agent_state <address> — estado que reporta el agente (idle|working|…)
  field "$1" "str((t.get('status') or {}).get('state','') if isinstance(t.get('status'),dict) else (t.get('status') or ''))"
}
screen_tail() { # screen_tail <address> — las últimas 15 líneas con texto de la pantalla
  hc read "$1" --lines 200 2>/dev/null | strip_cr \
    | awk -v max="$LIMIT_TAIL_LINES" 'NF { b[n % max] = $0; n++ }
        END { s = (n > max ? n - max : 0); for (i = s; i < n; i++) print b[i % max] }'
}
# HD-0e (C0-6): un límite de uso solo cuenta si el agente NO está «working», la
# pantalla no cambió entre dos lecturas seguidas (≥ LIMIT_STABLE_SECS) y la frase
# COMPLETA está en las últimas LIMIT_TAIL_LINES líneas. Palabras sueltas
# («quota», «rate limit», «usage limit») no cuentan: quien escribe o lee sobre
# límites de uso no está bloqueado (§3 del diseño).
limit_hit() { # limit_hit <address> — deja la frase en LIMIT_HIT (vacía si no hay)
  # DECISION (HD-0e): la función NO se llama en `$(...)`: en un subshell se
  # perdería el estado de «pantalla quieta» (PREV_*), y la detección nunca
  # llegaría a evaluarse.
  local addr="$1" cur now
  LIMIT_HIT=""
  if [ "$(agent_state "$addr")" = "working" ]; then
    # El agente está trabajando: no está bloqueado y no se busca nada.
    PREV_ADDR="$addr"; PREV_TAIL=""; PREV_TAIL_AT=0
    return 0
  fi
  cur="$(screen_tail "$addr")"
  now="$(date +%s)"
  if [ "$PREV_ADDR" != "$addr" ] || [ "$PREV_TAIL" != "$cur" ]; then
    # La pantalla cambió (o es otra terminal): la ventana de quieta empieza aquí.
    PREV_ADDR="$addr"; PREV_TAIL="$cur"; PREV_TAIL_AT="$now"
    return 0
  fi
  [ $((now - PREV_TAIL_AT)) -ge "$LIMIT_STABLE_SECS" ] || return 0
  LIMIT_HIT="$(printf '%s\n' "$cur" | grep -iE -m 1 "$LIMIT_PATTERNS" || true)"
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
  # HD-0d: si ya hay una terminal VIVA con nuestro nombre, el trabajador sigue:
  # la seguimos. No se lanza otro intento y no se mata ninguna terminal.
  ADDR="$(live_addr || true)"
  LAUNCHED=0
  if [ -n "$ADDR" ]; then
    echo "[$ID] la terminal $ADDR sigue viva con el nombre $NAME; la sigo (no lanzo otro intento)"
  else
    free_name
    if [ -n "$EZ_WORKER_CMD" ]; then
      # shellcheck disable=SC2086
      ADDR="$(launch $EZ_WORKER_CMD)"
    elif [ "$AGENT" = "opencode" ] && [ "$OPENCODE_MODE" = "interactive" ]; then
      # Sin --model: el modelo va en OPENCODE_CONFIG_CONTENT. env.exe fija la
      # variable y ejecuta el programa con argv en lista, sin shell.
      TEXT="$PROMPT"
      [ "$attempt" -gt 1 ] && TEXT="$CONT"
      save_session "$SID" "$MODEL"
      ADDR="$(launch "$ENV_EXE" "OPENCODE_CONFIG_CONTENT={\"model\":\"$MODEL\"}" \
        "$BIN" --standalone --auto --session "$SID" --prompt "$TEXT")"
    elif [ "$AGENT" = "opencode" ]; then
      # EZ_OPENCODE_MODE=run: modo anterior de un solo disparo.
      if [ "$attempt" -eq 1 ] && [ "$MODE" != "resume" ]; then
        save_session "$SID" "$MODEL"
        ADDR="$(launch "$BIN" run --model "$MODEL" --title "$SID" --auto "$PROMPT")"
      elif [ -n "$SESSION" ]; then
        ADDR="$(launch "$BIN" run --session "$SESSION" --model "$MODEL" --auto "$CONT")"
      else
        echo "[$ID] no encuentro la sesión $SID; empiezo una nueva"
        save_session "$SID" "$MODEL"
        ADDR="$(launch "$BIN" run --model "$MODEL" --title "$SID" --auto "$PROMPT")"
      fi
    elif [ "$attempt" -eq 1 ] && [ "$MODE" != "resume" ]; then
      ADDR="$(launch "$BIN" -m "$MODEL" "$PROMPT")"
    else
      ADDR="$(launch "$BIN" -c "$CONT")"
    fi
    LAUNCHED=1
  fi
  if [ -z "$ADDR" ]; then
    echo "[$ID] hc no lanzó el intento $attempt; reintento en 30 s"; sleep 30; MODE=resume; continue
  fi
  echo "[$ID] intento $attempt en $ADDR (padre ${HC_TERMINAL_ID:-ninguno}, agente $AGENT)"
  [ "$LAUNCHED" = "1" ] && [ "$attempt" -eq 1 ] && record en_proceso "" "" "" 1
  show "$ADDR"
  START=$(date +%s)
  UNKNOWN_SINCE=""
  UNKNOWN_MSG_AT=0
  [ -z "$EZ_AGENT_BIN" ] && [ -z "$EZ_WORKER_CMD" ] && low_priority
  while :; do
    st="$(terminal_state "$ADDR")"
    # El informe completo gana: si está, se cierra el intento con éxito.
    done_report && succeed "$ADDR" "$attempt"
    # C0-6 (HD-0e): la pantalla puede avisar de un límite de uso. El intento se
    # da por fallido con ese motivo, se avisa al padre y no se reintenta (mismo
    # modelo). Solo cuenta con el agente parado y la pantalla quieta (§3).
    lim=""
    limit_hit "$ADDR"
    lim="$LIMIT_HIT"
    if [ -n "$lim" ]; then
      lim="$(printf '%s' "$lim" | tr -s '[:space:]' ' ' | sed -e 's/^ //' -e 's/ $//')"
      echo "[$ID] límite de uso en $ADDR: $lim"
      save_log "$ADDR"
      record fallido "límite de uso: $lim" "" "" 1
      # DECISION (HD-0e): la terminal del trabajador NO se cierra. El padre
      # decide («esperar» o «ez retry»); el aviso le dice que lea la pantalla.
      finish 2
    fi
    case "$st" in
      live)
        UNKNOWN_SINCE=""; UNKNOWN_MSG_AT=0
        sleep 15
        NOW=$(date +%s)
        # Solo un worker recién lanzado puede estar «colgado al arrancar»; una
        # terminal a la que nos enganchamos (viva con nuestro nombre) no se mata.
        if [ "$LAUNCHED" = "1" ] && [ $((NOW-START)) -gt 180 ] && [ -z "$(hc read "$ADDR" --lines 5 2>/dev/null | strip_cr | tr -d '[:space:]')" ]; then
          echo "[$ID] intento $attempt colgado al arrancar; lo reinicio"
          hc kill "$ADDR" >/dev/null 2>&1; break
        fi
        ;;
      finished) break ;;
      *)
        # C0-3 + HD-0d: «desconocido» NO es «terminado». Nunca se mata una
        # terminal por esto: a los 2 min se sigue esperando (avisando cada 5 min).
        if [ -z "$UNKNOWN_SINCE" ]; then
          UNKNOWN_SINCE=$(date +%s)
          UNKNOWN_MSG_AT="$UNKNOWN_SINCE"
          echo "[$ID] aviso: estado desconocido de $ADDR; sigo esperando (no mato la terminal)"
        fi
        NOW=$(date +%s)
        if [ $((NOW-UNKNOWN_SINCE)) -gt "$UNKNOWN_GRACE" ] && [ $((NOW-UNKNOWN_MSG_AT)) -ge "$UNKNOWN_EVERY" ]; then
          UNKNOWN_MSG_AT="$NOW"
          echo "[$ID] aviso: $ADDR lleva $(( (NOW-UNKNOWN_SINCE)/60 )) min en estado desconocido; sigo esperando (no mato una terminal viva)"
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
