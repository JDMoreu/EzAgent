# Brief de tarea · E<n>-T<m> · <título corto>

> Copia esta plantilla para cada tarea en la que trocees una etapa. Todo lo
> que esté entre `<>` es obligatorio. Si una sección no aplica, escribe «N/A»
> y por qué.

## Objetivo
<Una frase con el resultado observable.>

## Por qué
<Hallazgo de la auditoría que resuelve (ID de `00-auditoria.md`) y su impacto.>

## Dentro del alcance
- <…>

## Fuera del alcance
- <Lo que el agente podría tener la tentación de hacer y NO debe hacer.>

## Archivos que se tocan
| Archivo | Acción | Notas |
|---|---|---|
| `<ruta>` | crear / modificar / borrar | <…> |

## Archivos prohibidos
- <rutas que no se deben tocar aunque parezca útil>

## Interfaces y contratos
```python
<firmas exactas, dataclasses, esquemas>
```

## Pasos
1. <Paso concreto; ≤ 300 líneas de diff.> → verificación: `<comando>`
2. <…>

## Criterios de aceptación
- [ ] <medible: comando + resultado esperado, número o prueba concreta>

## Pruebas
- Añadir: `<tests/test_x.py::test_y>`: <qué comprueba>
- Deben seguir pasando: `uv run pytest -q`

## Comandos de verificación
```powershell
uv run pytest -q
uv run ruff check src tests
<otros>
```

## Riesgos y rollback
- <riesgo> → <mitigación>; rollback: `git revert <commit>`

## Definición de hecho
Criterios cumplidos, pruebas y lint en verde, informe final según
`AGENTS.md` §6.

## Prompt para el agente
```
Lee AGENTS.md y este brief completo antes de empezar:
<ruta del brief>
Ejecuta solo la tarea E<n>-T<m>. Toca solo los archivos listados.
Al terminar, ejecuta los comandos de verificación y responde con el informe
final de AGENTS.md §6. Si algo no está especificado, usa # DECISION: y dilo.
```
