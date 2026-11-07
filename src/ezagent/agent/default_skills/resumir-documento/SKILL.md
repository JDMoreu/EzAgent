---
name: resumir-documento
description: Resume un documento del usuario (texto, Markdown, Word, PDF o Excel) en pocas frases y señala los puntos clave. Úsala cuando pidan un resumen de un archivo.
---

# Resumir un documento

## Cuándo usarla
Cuando el usuario pida resumir, explicar o sacar las ideas principales de un
archivo que ya está en su equipo.

## Pasos
1. Localiza el archivo y confirma la ruta si hay varias coincidencias.
2. Para texto o Markdown, usa `read_file` con un límite de caracteres.
3. Para `.docx`, `.pdf` o `.xlsx`, carga primero la skill correspondiente con
   `load_skill` y sigue sus pasos para extraer el texto.
4. Resume en una o dos frases por defecto. Si el usuario pide más detalle,
   añade hasta cinco puntos clave.
5. Indica el nombre del archivo resumido y, si hay dudas, qué parte no pudiste
   leer.

## Límites
- No leas en voz alta párrafos enteros ni listados largos: resume.
- No inventes datos que no estén en el documento.
- No resumas archivos enormes de una vez: pide permiso para dividirlos por
  secciones o páginas.
- No incluyas datos sensibles que aparezcan en el documento si el usuario no
  los necesita.
- Si el archivo no se puede leer, dilo y no supongas su contenido.
