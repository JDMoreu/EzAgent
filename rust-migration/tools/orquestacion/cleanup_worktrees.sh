#!/bin/bash
# Clasifica (y con --apply elimina) los árboles de trabajo de workers ya integrados.
# Solo elimina: rama work/*, ya contenida en rust/main, sin cambios sin commit y
# que no sea de un worker activo. Las ramas de git se conservan.
cd /c/Users/JDMoreu/Documents/EzAgent || exit 1
ACTIVE=" P-IMP-BARGEIN P-IMP-NAMES EGM-REV-A EGM-REV-B "
BASE="${BASE:-rust/main}"   # rama de integración (eco/main para el espacio de trabajo)
apply=0; [ "$1" = "--apply" ] && apply=1
remove=0; keep=0
# Una rama recién creada (su punta está en la historia principal de BASE) no tiene
# commits propios: es de un worker que acaba de empezar, no de uno integrado.
# (2026-11-18: sin esta guarda se borraron los worktrees de ECO-2b y ECO-7 en marcha.)
FIRST_PARENT="$(git rev-list --first-parent "$BASE")"
# Procesos vivos (supervisor, opencode, cargo…) que nombran una carpeta de worker.
RUNNING="$(powershell -NoProfile -Command 'Get-CimInstance Win32_Process | ForEach-Object { $_.CommandLine }' 2>/dev/null | tr '\\' '/')"
git worktree list --porcelain | awk '/^worktree /{p=$2} /^branch /{print p" "$2}' | while read -r path ref; do
  branch=${ref#refs/heads/}
  name=$(basename "$path")
  [ "$path" = "C:/Users/JDMoreu/Documents/EzAgent" ] && continue
  reason=""
  case "$branch" in work/*) ;; *) reason="no es de un worker (rama $branch)";; esac
  [ -z "$reason" ] && [[ "$ACTIVE" == *" $name "* ]] && reason="worker activo"
  if [ -z "$reason" ] && echo "$RUNNING" | grep -qE "EzAgent-wt/$name([/\" ]|$)"; then
    reason="hay procesos trabajando en esa carpeta"
  fi
  if [ -z "$reason" ] && echo "$FIRST_PARENT" | grep -qx "$(git rev-parse "$branch")"; then
    reason="rama sin commits propios (recién creada)"
  fi
  if [ -z "$reason" ]; then
    dirty=$(git -C "$path" status --porcelain 2>/dev/null | head -3)
    [ -n "$dirty" ] && reason="cambios sin commit: $(echo $dirty | tr '\n' ' ')"
  fi
  if [ -z "$reason" ]; then
    git merge-base --is-ancestor "$branch" "$BASE" || reason="rama no integrada en $BASE"
  fi
  if [ -n "$reason" ]; then
    echo "CONSERVO  $name  ($reason)"
  else
    echo "ELIMINO   $name"
    if [ $apply -eq 1 ]; then
      git worktree remove --force "$path" 2>&1 | sed "s/^/          /"
    fi
  fi
done
git worktree prune
