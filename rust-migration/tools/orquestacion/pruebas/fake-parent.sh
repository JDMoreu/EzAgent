#!/bin/bash
# Padre falso para C0-2: guarda en un archivo cada línea que hc send escribe
# en su terminal. No imprime nada: solo registra lo que recibe.
out="$1"
while IFS= read -r line; do
  printf '%s\n' "$line" >> "$out"
done
