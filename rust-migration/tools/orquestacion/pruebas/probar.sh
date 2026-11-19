#!/bin/bash
# Pruebas de supervise3.sh sin agentes reales ni cuota. Se ejecuta dentro de una
# terminal de hc para que HC_TERMINAL_ID exista y los workers cuelguen de ella.
# No usa EZ_SHOW: no toca la ventana de Herdr de la persona.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
if command -v python3 >/dev/null 2>&1 && python3 -c 'pass' 2>/dev/null; then PYTHON=python3; else PYTHON=python; fi
REAL_HC="$(command -v hc)"
ENV_EXE="${EZ_ENV_EXE:-C:/Program Files/Git/usr/bin/env.exe}"
# hc no inyecta HC_TERMINAL_ID en comandos directos: si falta, se deriva del
# nombre de esta terminal (primer argumento) con hc get.
if [ -z "${HC_TERMINAL_ID:-}" ] && [ -n "${1:-}" ]; then
  HC_TERMINAL_ID="$(hc get "$1" --json 2>/dev/null | $PYTHON -c "import json,sys; print(json.load(sys.stdin).get('address',''))")"
fi
[ -n "${HC_TERMINAL_ID:-}" ] || echo "[pruebas] sin HC_TERMINAL_ID: la prueba corre fuera de hc (Git Bash); el padre falso se crea sin padre"
WORK="${TEMP:-/tmp}/hd0-probar-$$"
TOOLS="$WORK/tools"
ADDRS=""
LAST_OUT=""
FAIL=0

cleanup() {
  for a in $ADDRS; do
    hc kill "$a" >/dev/null 2>&1
    hc rm "$a" >/dev/null 2>&1
  done
  [ -d "$WORK" ] && rm -rf "$WORK"
}
trap cleanup EXIT

# Copia el supervisor con LF: este bash (MSYS) no tiene la opción igncr.
mkdir -p "$TOOLS" "$WORK/bin"
tr -d '\r' < "$HERE/../supervise3.sh" > "$TOOLS/supervise3.sh"
cp "$HERE/../cleanup_worker.ps1" "$TOOLS/cleanup_worker.ps1"
cp "$HERE/fake-hc.sh" "$WORK/bin/hc"
chmod +x "$WORK/bin/hc"

# C0-2: padre falso propio. Todos los avisos van a él, nunca a la terminal de
# la persona ni a la del agente principal.
# HD-0d: la suite también se ejecuta fuera de hc (Git Bash), donde no hay
# HC_TERMINAL_ID: entonces el padre falso se crea con --no-parent y no se toca
# ninguna terminal de la persona.
PARENT_CAP="$WORK/parent-notes.txt"
: > "$PARENT_CAP"
if [ -n "${HC_TERMINAL_ID:-}" ]; then
  PARENT_PARENT="--parent $HC_TERMINAL_ID"
else
  PARENT_PARENT="--no-parent"
fi
PARENT_ADDR="$(hc run --bg --json --name "hd0bpadre$$" --cwd "$WORK" --no-notify \
  $PARENT_PARENT -- "C:/Program Files/Git/bin/bash.exe" \
  "$HERE/fake-parent.sh" "$PARENT_CAP" \
  | $PYTHON -c "import json,sys; print(json.load(sys.stdin).get('address',''))")"
[ -n "$PARENT_ADDR" ] || { echo "FALLO: no pude crear el padre falso"; exit 3; }
ADDRS="$PARENT_ADDR"
export HC_TERMINAL_ID="$PARENT_ADDR"
export EZ_SUBAGENTS_DIR="$WORK/subagents"
sleep 2

mkrepo() { # mkrepo <carpeta> <id> <modo: exit|tui|fail|retry>
  local dir="$1" id="$2" mode="$3"
  mkdir -p "$dir/rust-migration/reports" "$WORK/log"
  printf '%s\n' "$id" > "$dir/.fake-id"
  printf '%s\n' "$mode" > "$dir/.fake-mode"
  printf 'Encargo de prueba %s. Escribe el informe y haz commit.\n' "$id" > "$WORK/prompt.txt"
  git -C "$dir" init -q
  git -C "$dir" add -A
  git -C "$dir" -c user.email=hd0@example.com -c user.name=HD0Test commit -q -m "inicio"
}

addrs_for() { # direcciones de terminales con la etiqueta ez.brief=<id>
  hc ls --all --json --label "ez.brief=$1" 2>/dev/null \
    | $PYTHON -c "import json,sys; print(' '.join(t['address'] for t in json.load(sys.stdin).get('terminals',[])))"
}

field() { hc get "$1" --json 2>/dev/null | $PYTHON -c "import json,sys
try:
    d=json.load(sys.stdin); print($2)
except Exception:
    print('')"; }

regfile() { # regfile <id> — ruta del registro §13 de ese trabajador
  local p; p="$(printf '%s' "$HC_TERMINAL_ID" | tr '/' '_')"
  printf '%s/%s/%s.json' "$EZ_SUBAGENTS_DIR" "$p" "$(echo "$1" | tr '[:upper:]' '[:lower:]')"
}
reg_field() { # reg_field <id> <python-expr sobre d>
  local f; f="$(regfile "$1")"
  [ -f "$f" ] || { echo ""; return; }
  $PYTHON -c "import json,sys; d=json.load(open(sys.argv[1],encoding='utf-8')); print($2)" "$f" 2>/dev/null
}

check() { # check <nombre> <valor> <esperado>
  if [ "$2" = "$3" ]; then
    echo "OK $1: $2"
  else
    echo "FALLO $1: esperado «$3», hay «$2»"
    FAIL=1
  fi
}
notice() { # notice <nombre> <texto> — comprueba lo que recibió el padre falso
  if grep -Fq "$2" "$PARENT_CAP"; then
    echo "OK aviso $1"
  else
    echo "FALLO aviso $1: no aparece «$2»"
    FAIL=1
  fi
}

changes() { git -C "$1" status --porcelain 2>/dev/null | wc -l | tr -d ' '; }
name_count() { # cuántas terminales (vivas o no) llevan ese nombre
  hc ls --all --json 2>/dev/null | $PYTHON -c "
import json,sys
print(sum(1 for t in json.load(sys.stdin).get('terminals',[]) if t.get('name')=='$1'))"
}

humo_real() { # C0-HUMO: OpenCode real sin encargo, sin escribir en la interfaz
  echo "[pruebas] humo real: OpenCode sin encargo (EZ_HUMO_REAL=1)"
  local resolved bin sid addr st waited cfg state_dir
  resolved="$(EZ_RESOLVE_OPENCODE=1 bash "$TOOLS/supervise3.sh" HD0CHUMO "$WORK" "$HERE/../prompt-AUTO-A3.txt" 2>&1)"
  printf '%s\n' "$resolved"
  bin="$(printf '%s\n' "$resolved" | sed -n 's/^BIN=//p')"
  sid="$(printf '%s\n' "$resolved" | sed -n 's/^SID=//p')"
  check "resolución exe nativo" "$(basename "$bin")" "opencode.exe"
  check "resolución modo interactivo" "$(printf '%s\n' "$resolved" | sed -n 's/^MODE=//p')" "interactive"
  check "sesión del humo" "$sid" "ses_hd0chumos1"
  mkdir -p "$WORK/humo"
  cfg="$(cygpath -m "$WORK/humo/config")"
  state_dir="$(cygpath -m "$WORK/humo/state")"
  addr="$(hc run --bg --json --name "hd0chumo$$" --cwd "$WORK/humo" --no-notify \
    --parent "$HC_TERMINAL_ID" --label ez.brief=HD0CHUMO -- \
    "$ENV_EXE" "OPENCODE_CONFIG_CONTENT={\"model\":\"opencode/hy4-flash-free\"}" \
    "OPENCODE_CONFIG_DIR=$cfg" "XDG_STATE_HOME=$state_dir" \
    "$bin" --standalone --auto --session "$sid" \
    | $PYTHON -c "import json,sys; print(json.load(sys.stdin).get('address',''))")"
  ADDRS="$ADDRS $addr"
  [ -n "$addr" ] || { echo "FALLO humo: hc no lanzó la terminal"; FAIL=1; return; }
  echo "[humo] terminal=$addr bin=$bin sesión=$sid"
  waited=0
  st=""
  while [ "$waited" -lt 15 ]; do
    st="$(field "$addr" "str(d.get('status','') or '')")"
    case "$st" in *idle*) break ;; esac
    sleep 1
    waited=$((waited+1))
  done
  echo "[humo] tras ${waited}s: status=$st"
  case "$st" in
    *idle*) echo "OK humo idle: $st" ;;
    *) echo "FALLO humo idle: esperaba idle, hay «$st»"; FAIL=1 ;;
  esac
  check "humo sin error" "$(field "$addr" "str(d.get('state',''))")" "live"
  hc kill "$addr" >/dev/null 2>&1
  hc rm "$addr" >/dev/null 2>&1
  echo "[humo] cerrada $addr"
}

run_case() { # run_case <id> <carpeta> <modo> <variables-de-entorno...>
  local id="$1" dir="$2" mode="$3"; shift 3
  mkrepo "$dir" "$id" "$mode"
  LAST_OUT="$(env EZ_LOG_DIR="$WORK/log" EZ_MAX=2 "$@" bash "$TOOLS/supervise3.sh" \
    "$id" "$dir" "$WORK/prompt.txt" 2>&1)"
  local code=$?
  printf '%s\n' "$LAST_OUT"
  echo "[$id] código de supervise3.sh: $code"
  ADDRS="$ADDRS $(addrs_for "$id")"
  return $code
}

echo "[pruebas] terminal padre falso: $HC_TERMINAL_ID"

# EZ_SOLO_HUMO=1: solo el caso opt-in, para repetir el humo sin la suite completa.
if [ "${EZ_SOLO_HUMO:-0}" = "1" ]; then
  [ "${EZ_HUMO_REAL:-0}" = "1" ] && humo_real
  echo "[pruebas] resultado: $([ "$FAIL" -eq 0 ] && echo VERDE || echo ROJO)"
  exit "$FAIL"
fi

echo "[pruebas] caso 1: opencode interactivo falso, argv/env, informe, registro y aviso"
run_case HD0T1 "$WORK/opencode" exit EZ_AGENT=opencode EZ_AGENT_BIN="$HERE/fake-worker.cmd"
code=$?
[ "$code" -eq 0 ] || { echo "FALLO: caso 1 devolvió $code (esperado 0)"; FAIL=1; }
addr="$(addrs_for HD0T1 | awk '{print $1}')"
[ -n "$addr" ] || { echo "FALLO: caso 1 no dejó terminal"; FAIL=1; }
if [ -n "$addr" ]; then
  check "padre" "$(field "$addr" "d.get('parent','')")" "$HC_TERMINAL_ID"
  check "nombre" "$(field "$addr" "d.get('name','')")" "hd0t1"
  check "ez.agent" "$(field "$addr" "d.get('labels',{}).get('ez.agent','')")" "opencode"
  check "ez.model" "$(field "$addr" "d.get('labels',{}).get('ez.model','')")" "opencode/hy4-flash-free"
  check "cambios sin confirmar" "$(changes "$WORK/opencode")" "0"
fi
check "argv env.exe opencode" "$(grep -c -- '--standalone --auto --session ses_hd0t1s1 --prompt' "$WORK/opencode/.fake-argv.log")" "1"
check "id de sesión ses_" "$(grep -o -- '--session ses_[a-z0-9]*s1' "$WORK/opencode/.fake-argv.log" | head -1)" "--session ses_hd0t1s1"
check "OPENCODE_CONFIG_CONTENT" "$(grep -Fc 'ENV=[{"model":"opencode/hy4-flash-free"}]' "$WORK/opencode/.fake-argv.log")" "1"
grep -Fq -- '--prompt' "$WORK/opencode/.fake-argv.log" \
  && grep -Fq 'Encargo de prueba HD0T1' "$WORK/opencode/.fake-argv.log" \
  || { echo "FALLO: caso 1 no pasó el encargo en --prompt"; FAIL=1; }
check "registro estado" "$(reg_field HD0T1 "d['state']")" "terminado"
check "registro historial" "$(reg_field HD0T1 "','.join(h['state'] for h in d['history'])")" "en_proceso,terminado"
check "registro by" "$(reg_field HD0T1 "d['history'][0]['by']")" "supervisor"
check "registro notified" "$(reg_field HD0T1 "d.get('notified','')")" "terminado"
check "registro summary" "$(reg_field HD0T1 "d.get('summary','')")" "Estado: completado"
# La ruta puede llegar en forma POSIX o Windows (MSYS convierte los argv al
# pasar por env.exe): basta con que el registro apunte a un informe existente.
HD0B_REPORT="$(reg_field HD0T1 "d.get('report','')")"
if [ -f "$HD0B_REPORT" ] && printf '%s' "$HD0B_REPORT" | grep -Fq 'HD0T1-r1.md'; then
  echo "OK registro report: $HD0B_REPORT"
else
  echo "FALLO registro report: «$HD0B_REPORT» no es un informe existente"
  FAIL=1
fi
notice "terminado opencode" "hd0t1 (OpenCode · opencode/hy4-flash-free) terminó"
notice "resumen del subagente" "Resumen del subagente: «Estado: completado»"
notice "informe en aviso" "Informe:"
notice "ruta del informe en aviso" "HD0T1-r1.md"
notice "validación en aviso" "Para validarlo: ez validate hd0t1 --pass|--fail --file <evidencia>"
echo "[pruebas] línea «Informe» recibida: $(grep -F 'Informe:' "$PARENT_CAP" | head -1)"

echo "[pruebas] caso 2: mcode falso en modo tui, informe, commit y cierre"
run_case HD0T2 "$WORK/mcode" tui EZ_AGENT=mcode EZ_AGENT_BIN="$HERE/fake-worker.cmd"
code=$?
[ "$code" -eq 0 ] || { echo "FALLO: caso 2 devolvió $code (esperado 0)"; FAIL=1; }
addr="$(addrs_for HD0T2 | awk '{print $1}')"
[ -n "$addr" ] || { echo "FALLO: caso 2 no dejó terminal"; FAIL=1; }
if [ -n "$addr" ]; then
  check "padre" "$(field "$addr" "d.get('parent','')")" "$HC_TERMINAL_ID"
  check "nombre" "$(field "$addr" "d.get('name','')")" "hd0t2"
  check "ez.agent" "$(field "$addr" "d.get('labels',{}).get('ez.agent','')")" "mcode"
  check "ez.model" "$(field "$addr" "d.get('labels',{}).get('ez.model','')")" "minimax/MiniMax-M3.1-Flash"
  check "estado cerrado" "$(field "$addr" "d.get('state','')")" "finished"
  check "cambios sin confirmar" "$(changes "$WORK/mcode")" "0"
fi
notice "terminado mcode" "hd0t2 (mcode · minimax/MiniMax-M3.1-Flash) terminó"

echo "[pruebas] caso 3: sin informe (EZ_WORKER_CMD) reintenta, falla y avisa"
run_case HD0T3 "$WORK/fail" fail EZ_WORKER_CMD="$HERE/fake-worker.cmd"
code=$?
[ "$code" -eq 2 ] || { echo "FALLO: caso 3 devolvió $code (esperado 2)"; FAIL=1; }
printf '%s\n' "$LAST_OUT" | grep -q "sin informe completado tras 2 intentos" \
  || { echo "FALLO: caso 3 no agotó los 2 intentos"; FAIL=1; }
check "registro fallido estado" "$(reg_field HD0T3 "d['state']")" "fallido"
check "registro fallido detalle" "$(reg_field HD0T3 "d['history'][-1]['detail']")" "sin informe completado tras 2 intentos"
check "registro fallido notified" "$(reg_field HD0T3 "d.get('notified','')")" "fallido"
notice "fallido" "hd0t3 (OpenCode · opencode/hy4-flash-free) falló: sin informe completado tras 2 intentos. Revisa su pantalla: hc read"

echo "[pruebas] caso 4: opencode interactivo reintenta con la misma sesión"
run_case HD0T4 "$WORK/retry" retry EZ_AGENT=opencode EZ_AGENT_BIN="$HERE/fake-worker.cmd"
code=$?
[ "$code" -eq 0 ] || { echo "FALLO: caso 4 devolvió $code (esperado 0)"; FAIL=1; }
check "reintento misma sesión" "$(grep -c -- '--session ses_hd0t4s1' "$WORK/retry/.fake-argv.log")" "2"
check "reintento conserva entorno" "$(grep -Fc 'ENV=[{"model":"opencode/hy4-flash-free"}]' "$WORK/retry/.fake-argv.log")" "2"

echo "[pruebas] caso 5: opencode interactivo con TUI se cierra tras informe y commit"
run_case HD0T5 "$WORK/octui" tui EZ_AGENT=opencode EZ_AGENT_BIN="$HERE/fake-worker.cmd"
code=$?
[ "$code" -eq 0 ] || { echo "FALLO: caso 5 devolvió $code (esperado 0)"; FAIL=1; }
addr="$(addrs_for HD0T5 | awk '{print $1}')"
if [ -n "$addr" ]; then
  check "opencode tui cerrado" "$(field "$addr" "d.get('state','')")" "finished"
  check "opencode tui sin cambios" "$(changes "$WORK/octui")" "0"
else
  echo "FALLO: caso 5 no dejó terminal"; FAIL=1
fi

echo "[pruebas] caso 6: EZ_OPENCODE_MODE=run conserva el modo anterior"
run_case HD0T6 "$WORK/legacy" exit EZ_AGENT=opencode EZ_OPENCODE_MODE=run EZ_AGENT_BIN="$HERE/fake-worker.cmd"
code=$?
[ "$code" -eq 0 ] || { echo "FALLO: caso 6 devolvió $code (esperado 0)"; FAIL=1; }
check "modo run sin envoltura" "$(grep -c -- 'ARGS=run --model opencode/hy4-flash-free --title ses_hd0t6s1 --auto' "$WORK/legacy/.fake-argv.log")" "1"
check "modo run sin OPENCODE_CONFIG_CONTENT" "$(grep -Fc 'ENV=[]' "$WORK/legacy/.fake-argv.log")" "1"

echo "[pruebas] caso 7: C0-3 un hc get fallido/vacío no cuenta como terminado"
echo 3 > "$WORK/hc-fails"
run_case HD0T7 "$WORK/unknown" exit EZ_AGENT=opencode EZ_AGENT_BIN="$HERE/fake-worker.cmd" \
  PATH="$WORK/bin:$PATH" EZ_HC_FAIL_FILE="$WORK/hc-fails" EZ_REAL_HC="$REAL_HC"
code=$?
[ "$code" -eq 0 ] || { echo "FALLO: caso 7 devolvió $code (esperado 0)"; FAIL=1; }
printf '%s\n' "$LAST_OUT" | grep -q "estado desconocido" \
  || { echo "FALLO: caso 7 no reintentó ante un hc get fallido"; FAIL=1; }
printf '%s\n' "$LAST_OUT" | grep -q "informe completado" \
  || { echo "FALLO: caso 7 no acabó tras recuperarse hc"; FAIL=1; }

echo "[pruebas] caso 8: HD-0d una terminal viva con el mismo nombre se sigue; no se mata ni se duplica"
LIVE_ADDR="$(hc run --bg --json --name hd0t8 --cwd "$WORK" --no-notify \
  --parent "$HC_TERMINAL_ID" -- "C:/Program Files/Git/bin/bash.exe" -c 'sleep 25' \
  | $PYTHON -c "import json,sys; print(json.load(sys.stdin).get('address',''))")"
[ -n "$LIVE_ADDR" ] || { echo "FALLO: caso 8 no pude crear la terminal previa"; FAIL=1; }
ADDRS="$ADDRS $LIVE_ADDR"
sleep 2
run_case HD0T8 "$WORK/live" exit EZ_AGENT=opencode EZ_AGENT_BIN="$HERE/fake-worker.cmd"
code=$?
[ "$code" -eq 0 ] || { echo "FALLO: caso 8 devolvió $code (esperado 0)"; FAIL=1; }
printf '%s\n' "$LAST_OUT" | grep -q "name_taken" \
  && { echo "FALLO: caso 8 chocó con name_taken"; FAIL=1; }
printf '%s\n' "$LAST_OUT" | grep -q "sigue viva con el nombre" \
  && echo "OK caso 8: siguió la terminal viva en vez de matarla" \
  || { echo "FALLO: caso 8 no siguió la terminal viva"; FAIL=1; }
check "caso 8 una sola terminal con el nombre" "$(name_count hd0t8)" "1"

echo "[pruebas] caso 9: HD-0d un python que devuelve \r\n no debe parecer «desconocido»"
# Envoltorio que devuelve cada línea con \r\n, como el python de Windows.
printf '%s\n' '#!/bin/bash' '"$EZ_REAL_PYTHON" "$@" | sed -e "s/\r$//" -e "s/$/\r/"' > "$WORK/bin/python-cr"
chmod +x "$WORK/bin/python-cr"
run_case HD0T9 "$WORK/cr" slow EZ_AGENT=opencode EZ_AGENT_BIN="$HERE/fake-worker.cmd" \
  EZ_PYTHON="$WORK/bin/python-cr" EZ_REAL_PYTHON="$PYTHON" \
  EZ_UNKNOWN_GRACE=10 EZ_UNKNOWN_EVERY=10
code=$?
[ "$code" -eq 0 ] || { echo "FALLO: caso 9 devolvió $code (esperado 0)"; FAIL=1; }
if [ -f "$WORK/cr/rust-migration/reports/HD0T9-r1.md" ]; then
  echo "OK caso 9: el trabajador vivo terminó su informe"
else
  echo "FALLO caso 9: el trabajador vivo no dejó informe (lo mataron)"
  FAIL=1
fi
check "caso 9 una sola terminal" "$(name_count hd0t9)" "1"
printf '%s\n' "$LAST_OUT" | grep -q "estado desconocido" \
  && { echo "FALLO: caso 9 vio «estado desconocido» (el \r no se quitó)"; FAIL=1; }
printf '%s\n' "$LAST_OUT" | grep -q "informe completado en el intento 1" \
  && echo "OK caso 9: terminó en el primer intento" \
  || { echo "FALLO: caso 9 no terminó en el primer intento"; FAIL=1; }

echo "[pruebas] caso 10: HD-0d/HD-0e C0-6 un aviso de límite de uso deja el intento «fallido» sin reintento"
echo "[pruebas]   (HD-0e: solo cuenta con el agente parado y la pantalla quieta)"
run_case HD0T10 "$WORK/limit" limit EZ_AGENT=opencode EZ_AGENT_BIN="$HERE/fake-worker.cmd" \
  PATH="$WORK/bin:$PATH" EZ_FAKE_STATUS=idle EZ_REAL_HC="$REAL_HC"
code=$?
[ "$code" -eq 2 ] || { echo "FALLO: caso 10 devolvió $code (esperado 2)"; FAIL=1; }
printf '%s\n' "$LAST_OUT" | grep -q "límite de uso" \
  || { echo "FALLO: caso 10 no detectó el límite de uso"; FAIL=1; }
check "registro fallido por límite" "$(reg_field HD0T10 "d['state']")" "fallido"
detalle="$(reg_field HD0T10 "d['history'][-1]['detail']")"
case "$detalle" in
  *"límite de uso"*) echo "OK registro detalle del límite: $detalle" ;;
  *) echo "FALLO registro detalle del límite: «$detalle»"; FAIL=1 ;;
esac
notice "aviso de límite de uso" "hd0t10 (OpenCode · opencode/hy4-flash-free) falló: límite de uso:"
check "caso 10 sin reintento" "$(name_count hd0t10)" "1"

echo "[pruebas] caso 11: HD-0d C0-6 si EZ_MODEL cambia, se abre una sesión nueva"
run_case HD0T11 "$WORK/model" exit EZ_AGENT=opencode EZ_AGENT_BIN="$HERE/fake-worker.cmd" EZ_MODEL=opencode/m1
code=$?
[ "$code" -eq 0 ] || { echo "FALLO: caso 11 (primer modelo) devolvió $code (esperado 0)"; FAIL=1; }
rm -f "$WORK/model/rust-migration/reports/HD0T11-r1.md"
run_case HD0T11 "$WORK/model" exit EZ_AGENT=opencode EZ_AGENT_BIN="$HERE/fake-worker.cmd" EZ_MODEL=opencode/m2
code=$?
[ "$code" -eq 0 ] || { echo "FALLO: caso 11 (modelo nuevo) devolvió $code (esperado 0)"; FAIL=1; }
check "sesión nueva con el modelo nuevo" "$(grep -c -- '--session ses_hd0t11s2' "$WORK/model/.fake-argv.log")" "1"
check "la sesión vieja no se repite" "$(grep -c -- '--session ses_hd0t11s1' "$WORK/model/.fake-argv.log")" "1"

# HD-0e: la detección de límite de uso solo cuenta con el agente parado, la
# pantalla quieta y una frase COMPLETA en las últimas 15 líneas.
no_tocado() { # no_tocado <id> <carpeta> <log> — comprueba que no hubo límite
  local id="$1" dir="$2" log="$3"
  printf '%s\n' "$LAST_OUT" | grep -q "límite de uso" \
    && { echo "FALLO caso $id: el supervisor tomó la pantalla por un límite de uso"; FAIL=1; return; }
  if [ -f "$dir/rust-migration/reports/$id-r1.md" ]; then
    echo "OK caso $id: el trabajador terminó su informe (no se lo tocó)"
  else
    echo "FALLO caso $id: el trabajador no dejó informe"; FAIL=1
  fi
  [ -f "$log" ] || { echo "FALLO caso $id: no hay log de pantalla"; FAIL=1; return; }
}
echo "[pruebas] caso 12: HD-0e palabras sueltas («quota», «rate limit») con el agente parado no son un límite"
run_case HD0T12 "$WORK/chatter" chatter EZ_AGENT=opencode EZ_AGENT_BIN="$HERE/fake-worker.cmd" \
  PATH="$WORK/bin:$PATH" EZ_FAKE_STATUS=idle EZ_REAL_HC="$REAL_HC"
code=$?
[ "$code" -eq 0 ] || { echo "FALLO: caso 12 devolvió $code (esperado 0)"; FAIL=1; }
no_tocado HD0T12 "$WORK/chatter" "$WORK/log/HD0T12-hc1.log"
grep -Fq "quota" "$WORK/log/HD0T12-hc1.log" \
  && echo "OK caso 12: las palabras sueltas estaban en la pantalla" \
  || { echo "FALLO caso 12: la pantalla no tenía las palabras sueltas"; FAIL=1; }
check "registro caso 12" "$(reg_field HD0T12 "d['state']")" "terminado"

echo "[pruebas] caso 13: HD-0e palabras sueltas con el agente «working» tampoco son un límite"
run_case HD0T13 "$WORK/chatterwork" chatter EZ_AGENT=opencode EZ_AGENT_BIN="$HERE/fake-worker.cmd" \
  PATH="$WORK/bin:$PATH" EZ_FAKE_STATUS=working EZ_REAL_HC="$REAL_HC"
code=$?
[ "$code" -eq 0 ] || { echo "FALLO: caso 13 devolvió $code (esperado 0)"; FAIL=1; }
no_tocado HD0T13 "$WORK/chatterwork" "$WORK/log/HD0T13-hc1.log"
check "registro caso 13" "$(reg_field HD0T13 "d['state']")" "terminado"

echo "[pruebas] caso 14: HD-0e la frase de límite arriba de una pantalla larga no cuenta"
run_case HD0T14 "$WORK/limittop" limit-top EZ_AGENT=opencode EZ_AGENT_BIN="$HERE/fake-worker.cmd" \
  PATH="$WORK/bin:$PATH" EZ_FAKE_STATUS=idle EZ_REAL_HC="$REAL_HC"
code=$?
[ "$code" -eq 0 ] || { echo "FALLO: caso 14 devolvió $code (esperado 0)"; FAIL=1; }
no_tocado HD0T14 "$WORK/limittop" "$WORK/log/HD0T14-hc1.log"
grep -Fq "Free limit reached" "$WORK/log/HD0T14-hc1.log" \
  && echo "OK caso 14: la frase estaba en la pantalla (pero no en las últimas 15 líneas)" \
  || { echo "FALLO caso 14: la frase no llegó a la pantalla"; FAIL=1; }
check "registro caso 14" "$(reg_field HD0T14 "d['state']")" "terminado"

echo "[pruebas] caso 15: HD-0e la frase completa con el agente «working» no cuenta"
run_case HD0T15 "$WORK/limitwork" limit-work EZ_AGENT=opencode EZ_AGENT_BIN="$HERE/fake-worker.cmd" \
  PATH="$WORK/bin:$PATH" EZ_FAKE_STATUS=working EZ_REAL_HC="$REAL_HC"
code=$?
[ "$code" -eq 0 ] || { echo "FALLO: caso 15 devolvió $code (esperado 0)"; FAIL=1; }
no_tocado HD0T15 "$WORK/limitwork" "$WORK/log/HD0T15-hc1.log"
grep -Fq "Free limit reached" "$WORK/log/HD0T15-hc1.log" \
  && echo "OK caso 15: la frase estaba en la pantalla (pero el agente estaba trabajando)" \
  || { echo "FALLO caso 15: la frase no llegó a la pantalla"; FAIL=1; }
check "registro caso 15" "$(reg_field HD0T15 "d['state']")" "terminado"

[ "${EZ_HUMO_REAL:-0}" = "1" ] && humo_real

echo "[pruebas] resultado: $([ "$FAIL" -eq 0 ] && echo VERDE || echo ROJO)"
exit "$FAIL"
