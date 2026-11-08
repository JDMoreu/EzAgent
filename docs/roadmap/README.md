# Hoja de ruta de EzAgent

Fecha: 8 nov 2026 · Estado: aprobada · Responsable: JDMoreu

Este directorio contiene la auditoría completa del proyecto, las decisiones de
producto, la arquitectura objetivo, el sistema de diseño propuesto y un brief
autocontenido por etapa, pensado para delegarlo en agentes de código baratos
(DeepSeek V4.1 Flash, MiniMax M3.1 Flash) con revisión humana entre etapas.

## Decisiones fijadas

| Decisión | Valor |
|---|---|
| Posicionamiento | **Capa de voz para agentes de código.** No es un asistente general. |
| Audiencia (6 meses) | Open source primero: desarrolladores en Windows 11 |
| LLM por defecto | DeepSeek V4.1 Flash vía OpenCode Go; cualquier endpoint compatible con OpenAI |
| Local | VAD, STT (whisper.cpp) y TTS (Kokoro) siempre locales |
| UI | Notch flotante; tecnología final decidida en E6 con mediciones |
| Idioma | Español neutro latinoamericano por defecto + inglés (i18n) |
| Licencia | Apache-2.0 |

## Documentos

| Documento | Para qué sirve |
|---|---|
| [`00-auditoria.md`](00-auditoria.md) | Hallazgos con severidad y referencias `archivo:línea` |
| [`01-producto-y-marca.md`](01-producto-y-marca.md) | Posicionamiento, qué se queda y qué se va, persona, voz y naming |
| [`02-arquitectura.md`](02-arquitectura.md) | Arquitectura objetivo, módulos, hilos, backends de agentes, IPC |
| [`03-diseno-y-movimiento.md`](03-diseno-y-movimiento.md) | Sistema visual, estados, coreografía, springs, earcons |
| [`04-estrategia-llm.md`](04-estrategia-llm.md) | Herramientas, prompt, router local, narración, evals, modelos |
| [`05-skills-y-flujo.md`](05-skills-y-flujo.md) | Cómo delegar, qué skills usar y cómo revisar cada etapa |
| [`06-auditoria-ui.md`](06-auditoria-ui.md) | Auditoría de la UI y enlace al prototipo vivo `docs/design/notch-v2.html` |
| [`07-marca-hilo.md`](07-marca-hilo.md) | Marca y dirección de arte: **la gota** elegida (prototipos `gota-windows.html`; el hilo quedó descartado) |
| [`metricas.md`](metricas.md) | Línea base y resultados por etapa (se rellena desde E1) |
| [`plantillas/brief-tarea.md`](plantillas/brief-tarea.md) | Plantilla para trocear una etapa en tareas |

## Etapas

| Etapa | Brief | Depende de | Agente sugerido | Esfuerzo |
|---|---|---|---|---|
| E0 | [Higiene y base open source](etapas/E0-higiene-oss.md) | — | DeepSeek V4.1 Flash | S |
| E1 | [Medición: latencia y set dorado](etapas/E1-medicion.md) | E0 | DeepSeek V4.1 Flash | M |
| E2 | [Pipeline de voz fluido](etapas/E2-pipeline-voz.md) | E1 | MiniMax M3.1 Flash | L |
| E3 | [Núcleo de orquestación y backends](etapas/E3-orquestacion.md) | E1 | MiniMax M3.1 Flash | L |
| E4 | [Recorte de alcance y ajustes simples](etapas/E4-recorte.md) | E3 | DeepSeek V4.1 Flash | M |
| E5 | [Separación núcleo/UI](etapas/E5-nucleo-ui.md) | E2, E3 | MiniMax M3.1 Flash | L |
| E6 | [Spike de UI y decisión](etapas/E6-spike-ui.md) | E5 | Revisión humana o modelo fuerte | M |
| E7 | [Diseño, marca y animación](etapas/E7-diseno.md) | E6 | DeepSeek V4.1 Flash + skill `impeccable` | L |
| E8 | [Instalación y onboarding](etapas/E8-onboarding.md) | E4, E7 | DeepSeek V4.1 Flash | M |
| E9 | [Lanzamiento open source](etapas/E9-lanzamiento.md) | E8 | DeepSeek V4.1 Flash | S |

Esfuerzo: S ≈ 1–2 días, M ≈ 3–5 días, L ≈ 1–2 semanas de trabajo delegado
con revisión.

```
E0 ─► E1 ─┬─► E2 ─┐
          └─► E3 ─┼─► E5 ─► E6 ─► E7 ─┐
                  └─► E4 ─────────────┴─► E8 ─► E9
```

E2 y E3 pueden ejecutarse en paralelo en ramas o worktrees distintos; solo se
tocan en `voice/app.py` (zonas distintas, ver cada brief).

## Presupuestos objetivo (puertas de calidad)

Se miden con el banco de E1 en la máquina de referencia (Windows 11, GPU AMD con
Vulkan, auriculares) y DeepSeek V4.1 Flash.

| Métrica | Objetivo | Se exige desde |
|---|---|---|
| Fin de voz → primer sonido, P50 | ≤ 900 ms | E2 |
| Fin de voz → primer sonido, P95 | ≤ 1 500 ms | E2 |
| Comando local (mute, repetir…) → efecto | ≤ 300 ms | E2 |
| Fin de voz → acuse cuando hay herramienta | ≤ 700 ms | E3 |
| Llamadas al LLM por turno con herramienta | 1 | E3 |
| Precisión herramienta + argumentos (set dorado, es) | ≥ 95 % | E3 |
| Turnos cortados por error del VAD (20 frases) | 0 | E2 |
| CPU del notch en reposo | < 1 % | E6/E7 |
| Frames del notch mientras habla el TTS | 60 fps sin caídas > 50 ms | E6/E7 |
| Instalación limpia → primera orden por voz | < 5 min, sin consola | E8 |

## Cómo se ejecuta una etapa

1. Crea una rama `stage/E<n>-<slug>` (o un worktree).
2. Abre el brief y, si es largo, trocea con
   [`plantillas/brief-tarea.md`](plantillas/brief-tarea.md).
3. Pega al agente el bloque «Prompt para el agente» del brief (incluye la
   referencia a `AGENTS.md`).
4. Revisa el informe del agente, ejecuta los comandos de verificación del
   brief y pasa la revisión de [`05-skills-y-flujo.md`](05-skills-y-flujo.md).
5. Actualiza [`metricas.md`](metricas.md) y marca la etapa como hecha aquí.

## Estado

| Etapa | Estado | Fecha | Notas |
|---|---|---|---|
| E0 | pendiente | | |
| E1 | pendiente | | |
| E2 | pendiente | | |
| E3 | pendiente | | |
| E4 | pendiente | | |
| E5 | pendiente | | |
| E6 | pendiente | | |
| E7 | pendiente | | |
| E8 | pendiente | | |
| E9 | pendiente | | |
