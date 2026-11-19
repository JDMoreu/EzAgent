#!/bin/bash
# Clasifica (y con --apply elimina) los árboles de trabajo de workers ya integrados.
# Solo elimina: rama work/*, ya contenida en rust/main, sin cambios sin commit y
# que no sea de un worker activo. Las ramas de git se conservan.
cd /c/Users/JDMoreu/Documents/EzAgent || exit 1
ACTIVE=" P-IMP-BARGEIN P-IMP-NAMES EGM-REV-A EGM-REV-B "
BASE="${BASE:-rust/main}"   # rama de integración (eco/main para el espacio de trabajo)
apply=0; [ "$1" = "--apply" ] && apply=1
remove=0; keep=0
git worktree list --porcelain | awk '/^worktree /{p=$2} /^branch /{print p" "$2}' | while read -r path ref; do
  branch=${ref#refs/heads/}
  name=$(basename "$path")
  [ "$path" = "C:/Users/JDMoreu/Documents/EzAgent" ] && continue
  reason=""
  case "$branch" in work/*) ;; *) reason="no es de un worker (rama $branch)";; esac
  [ -z "$reason" ] && [[ "$ACTIVE" == *" $name "* ]] && reason="worker activo"
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
