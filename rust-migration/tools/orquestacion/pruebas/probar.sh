#!/bin/bash
# Pruebas de supervise3.sh sin agentes reales ni cuota. Se ejecuta dentro de una
# terminal de hc para que HC_TERMINAL_ID exista y los workers cuelguen de ella.
# No usa EZ_SHOW: no toca la ventana de Herdr de la persona.
set -u
HERE="$(cd "$(dirname "$0")" && pwd)"
if command -v python3 >/dev/null 2>&1 && python3 -c 'pass' 2>/dev/null; then PYTHON=python3; else PYTHON=python; fi
# hc no inyecta HC_TERMINAL_ID en comandos directos: si falta, se deriva del
# nombre de esta terminal (primer argumento) con hc get.
if [ -z "${HC_TERMINAL_ID:-}" ] && [ -n "${1:-}" ]; then
  HC_TERMINAL_ID="$(hc get "$1" --json 2>/dev/null | $PYTHON -c "import json,sys; print(json.load(sys.stdin).get('address',''))")"
fi
[ -n "${HC_TERMINAL_ID:-}" ] || { echo "FALLO: ejecuta probar.sh desde una terminal de hc"; exit 3; }
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
mkdir -p "$TOOLS"
tr -d '\r' < "$HERE/../supervise3.sh" > "$TOOLS/supervise3.sh"
cp "$HERE/../cleanup_worker.ps1" "$TOOLS/cleanup_worker.ps1"

# C0-2: padre falso propio. Todos los avisos van a él, nunca a la terminal de
# la persona ni a la del agente principal.
PARENT_CAP="$WORK/parent-notes.txt"
: > "$PARENT_CAP"
PARENT_ADDR="$(hc run --bg --json --name "hd0bpadre$$" --cwd "$WORK" --no-notify \
  --parent "$HC_TERMINAL_ID" -- "C:/Program Files/Git/bin/bash.exe" \
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

field() { hc get "$1" --json 2>/dev/null | $PYTHON -c "import json,sys; d=json.load(sys.stdin); print($2)"; }

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
check "argv env.exe opencode" "$(grep -c -- '--standalone --auto --session HD0T1-s1 --prompt' "$WORK/opencode/.fake-argv.log")" "1"
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
check "reintento misma sesión" "$(grep -c -- '--session HD0T4-s1' "$WORK/retry/.fake-argv.log")" "2"
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
check "modo run sin envoltura" "$(grep -c -- 'ARGS=run --model opencode/hy4-flash-free --title HD0T6-s1 --auto' "$WORK/legacy/.fake-argv.log")" "1"
check "modo run sin OPENCODE_CONFIG_CONTENT" "$(grep -Fc 'ENV=[]' "$WORK/legacy/.fake-argv.log")" "1"

echo "[pruebas] resultado: $([ "$FAIL" -eq 0 ] && echo VERDE || echo ROJO)"
exit "$FAIL"
