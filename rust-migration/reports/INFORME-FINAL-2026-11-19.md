# EzAgent — Informe final de la jornada del 19-nov-2026

Rama de integración: `eco/main` (nada se subió a GitHub ni se fusionó en `main`).
Registro completo, paso a paso, de cada revisión, incidente y decisión:
`rust-migration/tools/orquestacion/revision-HD.md`. Encargos en `rust-migration/briefs/`,
informes de cada subagente en `rust-migration/reports/<ID>-r1.md`.

**Reparto.** El agente principal (Claude Code) escribió los encargos, revisó, validó e
integró; el código y los diagnósticos los hicieron subagentes (OpenCode con DeepSeek V4.1
Flash desde las 14:10; antes también mcode, hasta que se agotaron sus créditos). Cada
validación repitió las pruebas del subagente y metió mutaciones para comprobar que las
pruebas detectan el fallo que dicen cubrir.

## 1. Lo que pediste y en qué quedó

| Pedido | Estado |
|---|---|
| Subagentes de cualquier tipo bajo el agente que los delega en Herdr, con tipo, modelo y esfuerzo del catálogo; `--show tree\|tab\|right` | **Hecho** (HD-1 … HD-9; prueba de punta a punta 131/131) |
| Estado nativo de cada subagente (en proceso, terminado, fallido, validado) y avisos al padre sin pedirlos, sin depender de Herdr | **Hecho** (HD-7, HD-8, HD-10; supervisor HD-0…HD-0f) |
| Lidiar con novedades: límites de uso, bloqueos, avisos, cambio de modelo | **Hecho** (NV-1 + NV-1b + NV-1c: estado `bloqueado`; NV-2: `ez retry`) |
| Recursos del PC, inventario de agentes, recomendación de modelo/esfuerzo según límites, `ez init` | **Hecho** (IR-0 … IR-5; prueba de punta a punta 86/86) |
| La app se congela al abrir el chat | **Arreglado** (DX-1b) y **virtualizado** (DX-1c: 100 mensajes 6 s → 0,9 s) |
| Respuestas breves a «ok / listo / bueno» | **Hecho e instalado** (DX-2, DX-2b, DX-2c) |
| La app se cayó al abrir el chat (13:52) | **Blindado** (DX-1d): un pánico ya no cierra la app y queda registrado; el disparador exacto no se reprodujo |
| Al cerrar la app, que se cierre whisper | **Hecho** (DX-3: Job Object; humo y mutación) — **no instalado aún en tu app** |
| La app de voz como padre: avisa novedades y responde «¿cómo va X?» | **Hecho** (NV-3 + NV-3b + NV-3c: vigila a sus hijos, anuncia bloqueado/fallido, reintenta; humo con `hc` real) |
| La animación de la barra al chat no se ve | **Hecho** (DX-1f: la apertura pasa de 0 a 11 cuadros intermedios) |
| La ventana del chat tiene varias sombras apiladas | **Hecho** (DX-1g: una sola sombra suave); su prueba falla en depuración → DX-1h |
| Máximo 3 compilando y vigilar recursos | Cumplido; tres incidentes de memoria atendidos (§4) |

## 2. Validado e integrado en `eco/main`

| Ola | Qué quedó | Evidencia del agente principal |
|---|---|---|
| HD-1…HD-9 | `ez delegate` con padre, tipo/modelo/esfuerzo validados, colocación, registro, `ez report/validate/tasks/watch`, enjambre bajo su primer miembro | `e2e_delegate` 131/131 con Herdr real |
| HD-0d/0e/0f | Supervisor de la orquestación: sin `\r`, no mata terminales vivas, límite solo con el agente parado y pantalla quieta, registro donde lo lee el producto, UTF-8 | `probar.sh` 18 casos; 4 mutaciones detectadas |
| HD-10/10b | Un registro dañado ya no deja ciego al vigilante ni a `ez tasks`; `--json` con forma fija | Mutación; `ez tasks` real |
| NV-1/1b/1c | Estado `bloqueado`, detector de novedades, frases completas, últimas 15 líneas con texto, dos lecturas iguales | 304 pruebas; 4 mutaciones; captura real de OpenCode |
| NV-2 | `ez retry` (sesión nueva, `-rN`, registro enlazado, `--auto`, reintento automático) | 3 mutaciones; `e2e_retry` 17/17 con Herdr real |
| IR-1/1b | `ez-scan`, `ez init`, `ez resources`, guarda de recursos; commit libre real de Windows | Diferencia < 2 % con Windows |
| IR-2/2b | `ez usage` por agente y por modelo, `planes.toml`; OpenCode leído de su esquema actual | `ez usage` real: OpenCode Go con tus tokens de hoy, no agotado |
| IR-3/3b/3c | `ez recommend`, `ez delegate` sin `--agent`, guarda con código 4, avisos de memoria | 3 mutaciones; recomendación real |
| IR-4/IR-5 | Prueba de punta a punta del inventario y corrección de los 3 fallos que encontró | `e2e_inventory` 86/86 |
| DX-1b/1d | Chat sin congelar; blindaje de pánicos en callbacks de Windows | Humo real; 3 mutaciones |
| DX-1c | Solo se maqueta lo visible | Humo con 320 mensajes; mutación |
| DX-2/2b/2c | Acuses cortos naturales | Instalado en tu app |
| DX-3/3b | `whisper-server` atado a la app; mismo servidor que el guion | Humo 4 pasos; mutación sin `KILL_ON_JOB_CLOSE` detectada |
| NV-3/3b/3c | La app de voz vigila a sus subagentes (`voz`), anuncia novedades, «¿cómo va X?», reintento sin `--parent voz` | 2 mutaciones; humo `--real-hc` |
| DX-1f/DX-1g | Apertura del chat animada; una sola sombra | Mutación del morph; humo `anim` 11/8 cuadros; capturas antes/después |

## 3. Lo que falló y cómo se resolvió

- **Validación sin humo real (DX-1b, 13:52):** la app se cayó al abrir el chat tras
  instalarlo. Se revirtió, se blindó (DX-1d) y quedó la regla: ningún cambio de interfaz
  se valida sin prueba de humo de la app real.
- **Supervisor con `\r`** (mató trabajadores sanos) y **falsos positivos de límite**
  (palabras sueltas): corregidos en HD-0d…HD-0f.
- **Devoluciones con correcciones concretas:** NV-1b (líneas en blanco de la pantalla
  real), IR-1 (memoria virtual inflada 13 GB), IR-2 (OpenCode leía datos viejos),
  IR-3b/3c (agotamientos por modelo y que venzan), HD-10b (JSON con dos formas), NV-3b
  (nadie vigilaba a los hijos de la app; `--parent voz` no existe), DX-3b (humo no
  repetible).
- **Proveedores:** se agotó el plan gratuito de OpenCode y luego los créditos de mcode;
  todo pasó a OpenCode + DeepSeek V4.1 Flash con notas de relevo.
- **Regla rota por un subagente:** DX-3 hizo `grep` sobre `.env` (no existía en su
  worktree; no leyó nada). Anotado.

## 4. Incidentes de recursos

1. Benchmark de DX-1 a 11 GB (mañana): parado; límite de 3 compilaciones y guardián.
2. `whisper-server` huérfanos (12,5 GB): cerrados; causa → DX-3.
3. Noche: un juego ocupó hasta 11 GB; NV-3 en pausa hasta que la memoria volvió; con tu
   permiso se cerraron todos los `whisper-server` (uno con elevación).

## 5. Lo que falta

- **Recompilar tu app** (pendiente de tu permiso). Hoy corre la versión de las 17:25
  (DX-1b + DX-1d + DX-2). Al recompilar desde `eco/main` llegan DX-1c (chat rápido), DX-3
  (whisper se cierra con la app), NV-3 (la app avisa de las novedades de sus subagentes),
  DX-1f (animación de apertura) y DX-1g (una sola sombra). Hasta entonces, si cierras la
  app, su `whisper-server` queda vivo.
- **DX-1e** (abrir el chat en tiempo constante con cualquier historial): propuesto, no
  lanzado. Con 100 mensajes ya abre en ~0,9 s.
- **Pruebas inestables con el equipo saturado** (pasan aisladas y por crate):
  `ez-app` `a_dirty_round_creates_one_fix_task_per_blocking_finding` (anterior a hoy),
  `ez-ctl` `stop_closes_the_port_and_url_is_loopback` (anterior a hoy) y
  `ez-app` `el_vigilante_de_la_app_bloquea_y_avisa_sin_escribir_el_registro` (NV-3; con
  `ez-app` solo, 378/378). Propuesta: endurecer sus esperas (NV-3d).
- **Riesgos conocidos**: el disparador exacto de la caída de las 13:52 no se reprodujo (si
  vuelve, el registro dirá archivo y línea); los subagentes lanzados a mano con `hc run`
  no entran en el registro (usar `ez delegate`); nada se subió a GitHub ni se fusionó en
  `main`.

## 6. Cierre

- `eco/main` final con todo lo de esta jornada fusionado (DX-1h, 5d795c6, es la última
  fusión de código). Batería completa de 8 crates (`ez-herdr`, `ez-ctl`, `ez-scan`,
  `ez-app`, `ez-stt`, `ez-hud`, `ez-core`, `ez-fakes`): **1 722 pruebas en verde**; las
  únicas fallas fueron las inestables de §5 bajo carga, y pasan aisladas. clippy sin avisos
  propios (los 14 son del `freetype-sys` incluido).
- Todos los subagentes cerrados y sus worktrees retirados; ningún `whisper-server` vivo.
- El PC se apaga al terminar, como pediste (apagado programado a las 00:00, aplazado
  automáticamente a la 01:00 y a las 02:00 mientras seguía el trabajo).
