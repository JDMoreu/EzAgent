---
name: organizar-carpetas
description: Clasifica y organiza archivos por tipo y fecha con las herramientas nativas. Úsala cuando el usuario pida ordenar una carpeta, agrupar documentos, separar fotos o limpiar Descargas.
---

# Organizar carpetas

## Cuándo usarla
Cuando el usuario pida ordenar, clasificar o agrupar archivos de una carpeta
(por ejemplo, Descargas, Documentos o una carpeta del workspace).

## Pasos
1. Si no está claro, pregunta una sola vez qué carpeta organizar.
2. Lista el contenido con `list_dir` y cuenta los archivos por extensión.
3. Propón un plan breve: categorías (documentos, imágenes, hojas de cálculo,
   vídeos, audio, comprimidos y otros) y, si hay muchos archivos, subcarpetas
   por año o por año-mes.
4. Pide confirmación antes de mover nada. Si son más de 5 archivos, el sistema
   pedirá una confirmación fuerte.
5. Crea las carpetas necesarias con `create_dir` y mueve los archivos con
   `move_path`, sin sobrescribir nunca un archivo existente.
6. Al terminar, di en una frase cuántos archivos moviste y a qué carpetas.

## Límites
- No borres archivos: para eso existe `delete_path`, que exige confirmación y
  siempre manda a la Papelera.
- No muevas archivos fuera de las carpetas permitidas por la política.
- No toques `.env`, claves, credenciales ni carpetas protegidas.
- No abras ni leas el contenido de los archivos para clasificarlos: basta con
  la extensión y el nombre.
- Si un archivo no encaja en ninguna categoría, déjalo donde está y dilo.
