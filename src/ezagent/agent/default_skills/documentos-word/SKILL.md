---
name: documentos-word
description: Crea y edita documentos Word (.docx) con python-docx. Úsala cuando el usuario pida un informe, una carta, un acta o cualquier archivo .docx.
---

# Documentos Word

## Cuándo usarla
Cuando pidan crear o editar un documento `.docx`: informes, cartas, actas,
propuestas, currículos o plantillas sencillas.

## Pasos
1. Pregunta el título y el contenido si faltan; haz una sola pregunta.
2. Escribe el contenido en Markdown en un archivo temporal dentro de
   `~/EzAgentWorkspace` con `write_file` (nivel 1).
3. Ejecuta `scripts/md_to_docx.py` con la ruta del `.md` y la ruta de salida
   mediante `run_skill_script` (nivel 2: el sistema pedirá confirmación).
   Si `allow_scripts` está desactivado, entrega el Markdown y explica cómo
   activar los scripts en `permissions.toml`.
4. Nunca sobrescribas un `.docx` existente: añade un sufijo con la fecha o un
   número al nombre.
5. Al terminar, di la ruta exacta del archivo creado y un resumen de una frase.

## Límites
- Máximo 20 páginas por llamada; para documentos mayores, divide por secciones.
- No incluyas datos personales que el usuario no haya dado.
- No ejecutes macros ni archivos `.docm`.
- No inventes cifras, fechas ni firmas.
- El script solo admite Markdown sencillo: títulos, párrafos, viñetas y
  negritas; el formato complejo se revisa después en Word.
