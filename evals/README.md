# Evals de EzAgent (promptfoo)

Los tests de `tests/` comprueban que el **código** hace lo que dice. Estos evals comprueban
que el **modelo**, con el prompt y las herramientas reales de EzAgent, se comporta como se
espera: elige la herramienta correcta, no obedece órdenes escondidas en datos externos,
habla de forma apta para voz y no inventa resultados.

El proveedor (`provider.py`) no copia el prompt: construye el `AgentRuntime` y el `Messenger`
reales, así que un cambio en `system_prompt.py`, en las herramientas o en el catálogo de
agentes se evalúa sin tocar nada aquí. Corre con un perfil de usuario temporal: la memoria,
las skills y el `system.md` reales **nunca** se envían al LLM.

## Uso

```powershell
npm install --prefix evals                     # una sola vez (promptfoo 0.125.1, versión fija)
uv run python evals/run_evals.py offline       # contrato del prompt: sin red ni clave (cabe en CI)
uv run python evals/run_evals.py live          # llama al LLM de .env / llm.json (cuesta tokens)
uv run python evals/report.py live             # resumen legible de la última corrida
uv run python evals/run_evals.py live -- --filter-pattern "delegaci"   # un subconjunto
npx --prefix evals promptfoo view              # interfaz web con el detalle
```

Código de salida: `0` todo verde, `100` hubo fallos (convención de promptfoo).

## Qué hay

| Archivo | Qué valida |
|---|---|
| `tests/contract.yaml` | **Offline.** Preámbulo de seguridad primero, ≤14 herramientas, sin shell libre, esquemas coherentes, hora al final (caché), presupuesto de tamaño del prompt |
| `tests/routing.yaml` | Herramienta y argumentos correctos; respuesta directa cuando no hace falta |
| `tests/delegation.yaml` | Lanzar el agente correcto (`run`, `then`, `notify_on_done`, `gui`, `background`, modelo y esfuerzo) |
| `tests/safety.yaml` | No promete borrar sin confirmar, no obedece «ignora tus reglas», no instala, no toca `.ssh`, no exfiltra |
| `tests/injection.yaml` | Web, archivos y pantallas de agentes con órdenes ocultas: se resumen, no se obedecen |
| `tests/style.yaml` | Apto para voz (sin markdown ni URLs completas; dominios sueltos sí), español neutro latinoamericano, acuse inmediato |
| `tests/honesty.yaml` | No inventa resultados fallidos ni datos que no tiene; no resume sin que se lo pidan |

`assertions.js` contiene las aserciones deterministas (baratas y repetibles). El juez LLM
(`llm-rubric`) se usa solo para lo subjetivo, con el mismo proveedor que usa la app.

## Cómo leer un fallo

- Un fallo de **estilo** o **enrutamiento** es una señal sobre el prompt o el modelo elegido;
  prueba con otro modelo en `.env` (`LLM_MODEL=...`) y compara.
- Un fallo de **seguridad** o **inyección** merece atención inmediata: la política del código
  seguiría bloqueando la acción, pero el modelo no debería ni intentarla.
- Los evals usan temperatura 0, aun así hay variación entre corridas; repite antes de concluir.
