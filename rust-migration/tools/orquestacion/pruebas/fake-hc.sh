#!/bin/bash
# hc falso para C0-3: simula un reinicio del núcleo haciendo que las primeras N
# llamadas `get` salgan sin salida y con error; el resto se delega al hc real.
# Solo se usa en las pruebas de probar.sh (EZ_HC_FAIL_FILE / EZ_REAL_HC).
#
# HD-0e: EZ_FAKE_STATUS=idle|working|blocked — el estado que «reporta» el
# agente en `hc get --json` (así se prueba que un agente trabajando no está
# bloqueado y que uno parado sí).
if [ "$1" = "get" ] && [ -n "${EZ_HC_FAIL_FILE:-}" ] && [ -f "$EZ_HC_FAIL_FILE" ]; then
  n="$(cat "$EZ_HC_FAIL_FILE" 2>/dev/null)"
  case "$n" in
    ''|*[!0-9]*) n=0 ;;
  esac
  if [ "$n" -gt 0 ]; then
    echo "$((n-1))" > "$EZ_HC_FAIL_FILE"
    exit 1
  fi
fi
if [ "$1" = "get" ] && [ -n "${EZ_FAKE_STATUS:-}" ]; then
  py="${EZ_REAL_PYTHON:-}"
  if [ -z "$py" ]; then
    if command -v python3 >/dev/null 2>&1 && python3 -c 'pass' 2>/dev/null; then py=python3; else py=python; fi
  fi
  "$EZ_REAL_HC" "$@" | "$py" -c "
import json,sys
try:
    t = json.load(sys.stdin)
except Exception:
    sys.exit(0)
if isinstance(t, dict):
    t['status'] = {'state': '$EZ_FAKE_STATUS'}
print(json.dumps(t))
"
  exit 0
fi
exec "$EZ_REAL_HC" "$@"
