---
name: documentos-pdf
description: Lee, une, divide, extrae texto y crea archivos PDF con pypdf y reportlab. Úsala cuando el usuario pida trabajar con documentos .pdf.
---

# Documentos PDF

## Cuándo usarla
Cuando pidan consultar un PDF, extraer su texto, unir varios PDF, dividirlos
por páginas o generar un PDF nuevo a partir de un texto.

## Pasos
1. Identifica la operación: `info`, `extract`, `merge`, `split` o `create`.
2. Usa `scripts/pdf_tool.py` con `run_skill_script` (nivel 2: se pedirá
   confirmación). Ejemplos:
   - `pdf_tool.py info informe.pdf`
   - `pdf_tool.py extract informe.pdf --pages 1-3 --output resumen.txt`
   - `pdf_tool.py merge unido.pdf parte1.pdf parte2.pdf`
   - `pdf_tool.py split informe.pdf carpeta_salida`
   - `pdf_tool.py create nuevo.pdf texto.txt --title "Informe"`
3. Comprueba que el archivo de salida no exista. El script se niega a
   sobrescribir: usa un nombre nuevo.
4. Al terminar, resume en una frase qué generaste y dónde está.

## Límites
- Máximo 50 MB por PDF y 300 páginas por operación.
- No se hace OCR: si el PDF es una imagen escaneada, avisa de que no hay texto.
- No se pueden abrir PDF protegidos con contraseña; pide una copia sin cifrar.
- No borres ni sustituyas el PDF original.
- Si falta `pypdf` o `reportlab`, el script devuelve un error claro; no instales
  nada sin permiso explícito del usuario.
