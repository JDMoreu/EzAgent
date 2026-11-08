# E6 · Spike de UI y decisión go/no-go

| Campo | Valor |
|---|---|
| Depende de | E5 |
| Agente sugerido | Prototipos: MiniMax/DeepSeek; medición y decisión: revisión humana o modelo fuerte |
| Esfuerzo | M, con límite estricto de 5 días laborables |
| Rama | `spike/E6-ui-a` y `spike/E6-ui-b` (no se fusionan; se fusiona solo el informe) |
| Hallazgos | U1, U2, U3, U5, U8 |

## Objetivo
Decidir con mediciones si el notch sigue en Python (optimizado) o pasa a
Tauri v2 / WebView2, usando el protocolo de E5 sin tocar el núcleo.

## Por qué
La fluidez de las animaciones es parte de la promesa («fluido y fresco»). El
render actual tiene límites estructurales (U1–U3), pero migrar tiene coste.
Un spike acotado evita decidir por intuición.

## Prototipo A — Python optimizado
- Proceso propio (ya lo da E5).
- Render en uint8 premultiplicado; nada de float32 por frame.
- Caché de silueta cuantizada (alto y ancho en pasos de 2 px) + interpolación.
- Sin pasar por Pillow por frame (show/hide con escala numpy o pre-renders).
- Firma de caché del menú sin `settings.snapshot()` por frame (versión
  incremental en `SettingsChanged`).
- Springs de `03-diseno-y-movimiento.md` §6 en lugar de bezier por bisección.

## Prototipo B — Tauri v2 + WebView2
- Ventana sin bordes, transparente, `alwaysOnTop`, `skipTaskbar`, sin
  activación (`WS_EX_NOACTIVATE` vía API nativa en Rust) y click-through en
  píxeles transparentes (región de hit-test actualizada desde JS).
- Front: TypeScript + CSS, sin framework (o Preact si hace falta estado),
  WAAPI y springs propios (mismos parámetros).
- Se conecta al WebSocket de E5 con `EZAGENT_UI_URL`/`EZAGENT_UI_TOKEN`.
- Alcance mínimo: isla (todos los estados de ojos), apertura/cierre del menú
  con 3 grupos de ejemplo y la fila de confirmación.

## Medición (igual para A y B)

| Criterio | Método | Umbral go |
|---|---|---|
| fps mientras habla el TTS | Contador de frames + PresentMon o log propio, 60 s | P5 ≥ 55 fps |
| Frame más largo | Mismo log | ≤ 50 ms |
| CPU en reposo (ojos idle) | `Get-Counter` del proceso UI, 120 s | < 1 % |
| CPU hablando | Ídem | < 6 % |
| RAM del proceso UI | Working set tras 10 min | ≤ 120 MB |
| No roba el foco | Escribir en otro programa mientras cambian estados y se abre el menú | 0 pérdidas de foco |
| Click-through | Clic en píxeles transparentes llega a la ventana de debajo | Sí |
| Arrastre y acople superior | Manual | Sin parpadeo |
| Arranque de la UI | Hasta el primer frame | < 1,5 s |
| Tamaño añadido al instalador | Medido | Informativo |
| Latencia del núcleo | Banco E1 con cada UI | Sin regresión > 5 % |

## Entregable
`docs/roadmap/decision-ui.md` con: tabla de mediciones A vs B, capturas o GIF,
riesgos de cada opción, coste estimado de terminar cada una (E7) y la
recomendación. **El usuario toma la decisión** y la anota en el documento.

Regla de decisión por defecto: si B cumple todos los umbrales y A falla en
alguno de fps/frame largo/CPU, elegir B. Si ambos cumplen, elegir A (menos
piezas, sin Rust), salvo que el usuario priorice la velocidad de iteración
de diseño.

## Fuera del alcance
- No terminar ninguna de las dos UI (eso es E7).
- No tocar el núcleo ni el protocolo (si falta un mensaje, se anota en la decisión).

## Prompt para el agente (por prototipo)
```
Lee AGENTS.md, docs/roadmap/etapas/E6-spike-ui.md,
docs/roadmap/03-diseno-y-movimiento.md §5-§6 y docs/roadmap/protocolo-ui.md.
Construye solo el prototipo <A|B> en la rama spike/E6-ui-<a|b>. Límite: 3 días
de trabajo. Al terminar, ejecuta la tabla de medición completa y entrega los
números brutos (no redondeados) en el informe de AGENTS.md §6.
```
