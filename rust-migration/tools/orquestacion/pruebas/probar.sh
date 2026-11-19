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

mkrepo() { # mkrepo <carpeta> <id> <modo: exit|tui|fail>
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

check() { # check <nombre> <valor> <esperado>
  if [ "$2" = "$3" ]; then
    echo "OK $1: $2"
  else
    echo "FALLO $1: esperado «$3», hay «$2»"
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

echo "[pruebas] terminal padre: $HC_TERMINAL_ID"

echo "[pruebas] caso 1: opencode con binario falso, informe y commit"
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

echo "[pruebas] caso 3: sin informe (EZ_WORKER_CMD) reintenta y falla"
run_case HD0T3 "$WORK/fail" fail EZ_WORKER_CMD="$HERE/fake-worker.cmd"
code=$?
[ "$code" -eq 2 ] || { echo "FALLO: caso 3 devolvió $code (esperado 2)"; FAIL=1; }
printf '%s\n' "$LAST_OUT" | grep -q "sin informe completado tras 2 intentos" \
  || { echo "FALLO: caso 3 no agotó los 2 intentos"; FAIL=1; }

echo "[pruebas] resultado: $([ "$FAIL" -eq 0 ] && echo VERDE || echo ROJO)"
exit "$FAIL"
