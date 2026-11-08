# 06 · Auditoría de la UI y prototipo v2

Fecha: 8 nov 2026. Basada en la lectura de `DESIGN.md`, `src/ezagent/voice/notch/**`
y la auditoría técnica (`00-auditoria.md`), **no en capturas de la app en ejecución**.
Si se aportan capturas, esta tabla se ajusta antes de E7.

**Prototipo vivo:** [`docs/design/notch-v2.html`](../design/notch-v2.html) (ábrelo en
el navegador). Implementa los once estados, los cuatro muelles, el menú
simplificado, la confirmación, los cinco earcons, ambos temas, el modo acoplado y
el movimiento reducido, con los valores de `03-diseno-y-movimiento.md`. Es la
referencia visual que E7 debe igualar en la UI real.

| # | Hoy (MVP) | Problema | Versión 2 | Etapa |
|---|---|---|---|---|
| 1 | Ojos mint sobre gris frío | Se lee como «herramienta de IA genérica»; el símbolo no dice nada de agentes | Tinta cálida, señal lima y violeta exclusivo de agentes | E7 |
| 2 | Estados centrados en el micrófono | No se ve si un agente trabaja, terminó o se atascó | `dispatch`, `working` (órbita), `done`, `blocked` | E7 |
| 3 | 36 ajustes, 9 grupos, menú de 596 px con scroll | ~20 filas técnicas visibles por defecto | VOZ · MODELO · AGENTES + «Avanzado» plegado, sin scroll | E4 |
| 4 | «Spring» como bezier con overshoot | Rebote fijo, resuelto por bisección en cada llamada | Muelles físicos `snappy/launch/panel/gentle`; sin rebote donde recorta | E7 |
| 5 | Sin sonido | Con el notch fuera de la vista nada confirma nada | Cinco earcons activables | E2 · E7 |
| 6 | Valores visuales en cinco sitios | `tokens.py`, `menu.py`, `eyes.py`, DESIGN.md y HTML se contradicen | `design/tokens.json` como única fuente | E7 |
| 7 | Render completo a 60 fps en el proceso de audio | GIL con Kokoro/PyAudio; caché de silueta falla al abrir el menú | UI en proceso aparte; Python optimizado vs WebView2 con mediciones | E5 · E6 |
| 8 | Primera ejecución sin guía | La isla no tiene texto | Onboarding de tres pasos con texto | E8 |
| 9 | Menú y estados con textos solo en español | Sin i18n | Catálogo es/en | E4 |

## Qué no cambia
- Los dos ojos como único símbolo, la isla sin texto fuera de la confirmación y el
  onboarding, el foco que nunca se roba, el movimiento reducido y la regla de
  «una señal viva a la vez».

## Criterios de aceptación visuales para E7
1. Cada estado de `notch-v2.html` existe en la UI real con la misma forma, color,
   movimiento y sonido.
2. Contrastes de la tabla del prototipo cumplen en ambos temas.
3. Con la prueba de captura de E6 (fps, frame largo, CPU), sin regresión.
4. `impeccable` `critique` y `audit` sin hallazgos de severidad alta.
