---
name: hojas-excel
description: Lee, resume y crea hojas de cálculo .xlsx con openpyxl. Úsala cuando el usuario pida revisar, completar o generar un archivo Excel.
---

# Hojas de cálculo Excel

## Cuándo usarla
Cuando pidan leer un `.xlsx`, resumir sus hojas, añadir filas o crear una hoja
de cálculo nueva con datos.

## Pasos
1. Si solo quieren saber qué contiene, usa `scripts/xlsx_tool.py summary`.
2. Para leer datos concretos, usa `read` con `--sheet` y `--max-rows`; no
   vuelques miles de filas en la voz.
3. Para crear un archivo nuevo, prepara un JSON de filas y usa `create`; el
   script se niega a sobrescribir un archivo existente.
4. Para añadir datos a un libro existente, usa `append` con `--sheet`; esto
   modifica el archivo y el sistema pedirá confirmación.
5. Al terminar, di la ruta y una frase con el número de filas o columnas.

## Límites
- No se ejecutan macros ni archivos `.xlsm`.
- No se evalúan fórmulas: openpyxl lee el valor guardado o la fórmula, no la
  recalcula. Si hace falta recalcular, ábrelo en Excel.
- Máximo 20.000 filas por operación y 50 columnas.
- No borres hojas ni sobrescribas el libro original sin confirmación.
- Si falta `openpyxl`, el script devuelve un error claro; no instales nada sin
  permiso explícito.
