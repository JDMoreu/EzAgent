---
name: notas-rapidas
description: Guarda notas rápidas en Markdown dentro de ~/EzAgentWorkspace/Notas. Úsala cuando el usuario dicte algo que quiera conservar o pida apuntar una idea.
---

# Notas rápidas

## Cuándo usarla
Cuando el usuario diga «apunta», «guarda una nota», «recuerda esto» o dicte
una idea, una tarea o un dato que quiera conservar.

## Pasos
1. Si no hay título, propón uno corto y pregunta una sola vez si le vale.
2. Crea el nombre del archivo con la fecha: `AAAA-MM-DD-titulo-corto.md`.
3. Escribe la nota con `write_file` dentro de
   `~/EzAgentWorkspace/Notas/` (nivel 1). Crea la carpeta si no existe.
4. Si el archivo ya existe, añade un sufijo numérico; nunca sobrescribas.
5. Confirma en una frase la ruta donde quedó la nota.

## Límites
- La nota es texto Markdown sencillo: título, fecha y cuerpo.
- No guardes contraseñas, tokens, claves ni datos personales sensibles.
- No escribas fuera de las carpetas permitidas por la política.
- Máximo 10.000 caracteres por nota; si es más larga, divide en varias.
- No borres notas existentes: para eso está `delete_path`, con confirmación.
