#!/bin/bash
# hc falso para C0-3: simula un reinicio del núcleo haciendo que las primeras N
# llamadas `get` salgan sin salida y con error; el resto se delega al hc real.
# Solo se usa en las pruebas de probar.sh (EZ_HC_FAIL_FILE / EZ_REAL_HC).
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
exec "$EZ_REAL_HC" "$@"
