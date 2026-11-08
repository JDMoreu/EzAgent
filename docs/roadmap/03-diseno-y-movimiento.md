# 03 · Diseño, movimiento y sonido

Dirección propuesta para E7. Sustituye a `DESIGN.md` cuando E7 termine. Hasta
entonces `DESIGN.md` sigue vigente para el notch actual. Los valores de color
son **propuesta**: E7 los valida con la skill `impeccable` (critique + audit) y
con la función de contraste del proyecto antes de fijarlos.

## 1. Idea rectora: «la mirada que delega»

Los dos ojos siguen siendo el único símbolo. Lo nuevo es que **expresan la
relación con los agentes**, no solo el estado del micrófono:

- te miran cuando hablas (escucha),
- miran hacia un lado y «lanzan» cuando despachan,
- esperan con un punto que orbita mientras un agente trabaja,
- dan un saltito y suenan cuando el agente termina,
- se entornan en ámbar cuando un agente se bloquea.

El notch deja de ser un instrumento técnico y pasa a ser un compañero
discreto. Sigue sin texto en la isla (salvo confirmación y onboarding).

## 2. Paleta «Tinta y señal» (propuesta)

Se abandona grafito + mint, que se lee como «herramienta de IA genérica».
Base de tinta cálida, una señal lima fosforescente para lo vivo y un violeta
suave, exclusivo del mundo de los agentes.

### Tema oscuro (por defecto)

| Token | Valor | Uso |
|---|---|---|
| `ink` | `#141210` | Cuerpo de la isla y del menú |
| `ink-deep` | `#0C0B0A` | Pozos de campos |
| `surface` | `#1E1B18` | Grupos |
| `surface-hover` | `#282420` | Hover de fila |
| `track` | `#34302B` | Pistas, toggles apagados |
| `text` | `#F4EFE8` | Texto principal |
| `text-2` | `#C2BAAE` | Valores, etiquetas |
| `text-3` | `#9C9488` | Ayudas, micro-etiquetas |
| `line` | `rgb(244 239 232 / 9%)` | Hairline |
| `signal` | `#D4F25A` | Escucha, habla, selección, acción principal |
| `signal-ink` | `#141210` | Texto sobre `signal` |
| `signal-dim` | `#7E8F3A` | Ojos en reposo (señal al ~55 % sobre `ink`) |
| `agent` | `#A8A1FF` | Despachando, agente trabajando, punto en órbita |
| `attention` | `#FFB23E` | Pensando, agente bloqueado, cambios sin guardar |
| `fault` | `#FF5D52` | Error, micrófono silenciado |

### Tema claro

| Token | Valor |
|---|---|
| `ink` | `#FAF8F5` |
| `surface` | `#F1EDE7` |
| `surface-hover` | `#E8E3DB` |
| `track` | `#D6D0C6` |
| `text` / `text-2` / `text-3` | `#1A1714` / `#4E4840` / `#6F675C` |
| `signal` | `#4D6B00` (texto/iconos) · relleno de botón `#5C7F00` con texto blanco |
| `agent` | `#5B4FD6` |
| `attention` | `#A35F00` |
| `fault` | `#C8241A` |

### Reglas
- **Una señal viva a la vez.** `signal` en una superficie solo marca lo que está
  vivo o la siguiente acción. Si aparece en dos funciones, una sobra.
- **`agent` solo para agentes.** Nunca para selección ni para botones.
- **Ámbar = espera o atención; rojo = fallo.** Nunca para énfasis.
- Contraste: texto ≥ 4,5:1 sobre su fondo; iconos y ojos ≥ 3:1. E7 adjunta la
  tabla de contrastes calculada.
- Sin glow decorativo, degradados de texto, glassmorphism ni sombras en reposo
  (se heredan las prohibiciones de `DESIGN.md`).

## 3. Tipografía

Se mantiene **Geist / Geist Mono** (ya empaquetadas en
`src/ezagent/voice/assets/fonts/`, licencia OFL). La isla no tiene texto; en el
menú solo se usan:

| Estilo | Especificación | Uso |
|---|---|---|
| Row | Geist 14/400 | Nombre de fila |
| Body | Geist 13/400, interlineado 1,45 | Ayudas, resumen de confirmación |
| Control | Geist 12/600 | Botones |
| Micro | Geist Mono 11/500, +0,08 em, MAYÚSCULAS | Etiquetas de grupo, medidas |
| Value | Geist Mono 11/500, tabular | Valores numéricos con unidades |

## 4. Geometría

| Elemento | Valor |
|---|---|
| Isla flotante (píldora) | 152×36, radio 18 |
| Isla acoplada (muesca) | 168×36, hombros cóncavos de 7 px |
| Menú abierto | 320 × hasta 520 (el menú básico cabe sin scroll) |
| Fila de confirmación | 300×88 |
| Radios | 6 (pequeño) · 10 (campos) · 12 (grupos) · 18 (superficie) · cápsula |
| Espaciado | 4 · 8 · 12 · 16 · 20 · 24 |

## 5. Estados de los ojos

| Estado | Disparador | Forma | Color | Movimiento | Sonido |
|---|---|---|---|---|---|
| `idle` | En espera | Cápsula 7×11, gap 14 | `signal-dim` | Parpadeo 4,5–6,5 s + deriva ±1,5 px | — |
| `listen` | Voz detectada | Cápsula 8×(14+nivel·8), gap 16 | `signal` | Sigue la voz (ataque 45 / caída 200 ms) | — |
| `heard` | Fin de turno | Cápsulas comprimen a 8×8 en 90 ms | `signal` | Un «asentimiento» de 3 px hacia abajo | `heard` (opcional) |
| `think` | Esperando STT/LLM | Cápsula 8×8 | `attention` | Barrido horizontal 1,2 s | — |
| `speak` | TTS sonando | Cápsula 8×(9+nivel·5), inclinación 4–8° y **la voz sale de los ojos**: 3 barras por lado (2,6 px, alto 2+nivel·(13−2i)) | `signal` | Cada barra repite el nivel con 38 ms más de retraso que la anterior (onda que viaja hacia afuera). Los ojos no se desplazan. El nivel es el de la salida real del TTS; en el prototipo, una envolvente silábica de ~3,5 Hz con pausas entre palabras | — |
| `dispatch` | Tool-call de lanzamiento | Ojos miran a la derecha (+4 px) y vuelven | `agent` | Spring «lanzar» 380 ms | `dispatch` |
| `agent_working` | ≥1 agente trabajando y EzAgent en reposo | Ojos `idle` + punto de 3 px orbitando | punto `agent` | Órbita 2,4 s; varios agentes = varios puntos (máx. 3) | — |
| `agent_done` | Un agente terminó | Arcos ^ ^ | `signal` | Dos saltitos (spring) | `done` |
| `agent_blocked` | Un agente pide intervención | Ojos entornados (8×5) | `attention` | Un pulso lento de opacidad 0,7↔1 cada 1,6 s hasta que el usuario lo atiende | `blocked` |
| `mute` | Micrófono silenciado | Raya 11×3 | `text-3` | — | — |
| `error` | Fallo | Cápsula 8×12 inclinada ±18° | `fault` | Una sacudida | `error` |

Prioridad: `error` > `speak` > `think` > `listen` > `agent_blocked` >
`agent_done` > `dispatch` > `mute` > `agent_working` > `idle`. Permanencia
mínima: 120 ms. `agent_done` dura 1,2 s y vuelve al estado base.

El micrófono silenciado se sigue indicando con el icono tachado en `fault`; los
ojos no se ponen rojos por el mute.

## 6. Movimiento

### Springs físicos (sustituyen a bezier con overshoot)

Integrador semi-implícito de Euler a paso fijo de 1/120 s, con parámetros
(rigidez `k`, amortiguación `c`, masa 1):

| Spring | k | c | Uso | Rebote |
|---|---|---|---|---|
| `snappy` | 520 | 38 | Poses de ojos, thumbs | ~2 % |
| `launch` | 300 | 18 | `dispatch`, saltitos de `agent_done` | ~12 % |
| `panel` | 360 | 38 | Alto y ancho del menú | 0 % (críticamente amortiguado) |
| `gentle` | 180 | 26 | Deriva, órbitas que cambian de radio | 0 % |

Regla: nunca rebote en dimensiones que recortan contenido (alto del menú,
fila de confirmación).

### Duraciones para lo que no es spring

| Token | ms | Uso |
|---|---|---|
| `t-micro` | 100 | Hover, color de icono |
| `t-fast` | 160 | Cambio de color de estado, aparición de contenido |
| `t-base` | 220 | Giro de flecha, crossfades |
| `stagger` | 35 por grupo | Entrada del contenido del menú (máx. 5 escalones) |

### Coreografías

1. **Abrir menú:** ancho y alto con `panel`; la flecha gira 180° (`t-base`);
   el contenido entra con opacidad 0→1 y +6 px, escalonado 35 ms por grupo,
   empezando cuando el alto llega al 40 %.
2. **Cerrar menú:** contenido a 0 en 100 ms; luego alto y ancho con `panel`.
3. **Despachar:** `think` → al detectar tool-call `run_agent`: `dispatch`
   (mirada a la derecha + earcon) → acuse por TTS (`speak`) → `idle` + punto
   en órbita (`agent_working`).
4. **Agente terminó:** earcon `done` → `agent_done` (saltitos) → narración por
   TTS si la llamada está activa; si no, solo earcon + ojos.
5. **Confirmación:** la isla crece a 300×88 con `panel`; ojos en `attention`;
   botones No/Sí; resuelto → vuelve con `panel`.

### Movimiento reducido
Con `SPI_GETCLIENTAREAANIMATION = 0` (o `prefers-reduced-motion` en la UI
web): sin springs ni deriva, sin órbita (el punto se queda fijo), sin
saltitos; los cambios son fundidos de 120 ms. Los ojos siguen reaccionando al
nivel de voz porque es información.

## 7. Sonido de interfaz (earcons)

Sintetizados en código (`voice/earcons.py`), sin archivos de audio con
licencia. Senos con envolvente ADSR, −20 dBFS de pico, mono, 24 kHz.
Activables por separado en Ajustes (por defecto: `dispatch`, `done`,
`blocked` y `error` activos; `heard` desactivado).

| Earcon | Notas | Duración | Carácter |
|---|---|---|---|
| `heard` | E6 (1319 Hz) | 45 ms | Tic suave de «te escuché» |
| `dispatch` | A5 → E6 | 2 × 70 ms | Subida corta: «enviado» |
| `done` | C6 → E6 → G6 | 3 × 80 ms | Arpegio mayor breve |
| `blocked` | E5 → C5 | 2 × 120 ms | Descenso suave: «necesito algo» |
| `error` | A4 × 2 | 2 × 90 ms | Doble toque grave |

Los earcons suenan por el mismo `player` que el TTS (E2) y nunca se solapan
con la voz: si hay TTS en curso, se encolan delante de la siguiente frase.

## 8. Menú simplificado (E4 define el contenido; E7 el estilo)

```
┌ AJUSTES ──────────────────────────── × ┐
│ VOZ                                     │
│   Micrófono            [Auriculares ▾]  │
│   Medidor  ▁▂▃▅▆▇█    umbral ──●──      │
│   Voz                  [Lucía ▾]  ▶     │
│   Velocidad            ──●── 1,0×       │
│ MODELO                                  │
│   Proveedor            [OpenCode Go ▾]  │
│   Modelo               [deepseek… ▾]    │
│   Clave                [•••••••• ✓]     │
│ AGENTES                                 │
│   Agente por defecto   [OpenCode ▾]     │
│   Avisos con sonido    [●]              │
│   Abrir agents.toml                     │
│ ▸ AVANZADO                              │
├─────────────────────────────────────────┤
│ Todo guardado                 [Guardar] │
└─────────────────────────────────────────┘
```

«Avanzado» (plegado) contiene: VAD y silencio, pausas TTS, motor STT/Kokoro,
URL base, temperatura, tokens máximos, esfuerzo de razonamiento, atajos,
`system.md`, `voice.json` y tema.

## 9. Una sola fuente de tokens

- `design/tokens.json` (formato W3C Design Tokens): colores por tema, radios,
  espaciado, springs, duraciones, geometría de ojos por estado y earcons.
- Generador `scripts/gen_tokens.py` → `src/ezagent/notch/tokens_gen.py` y
  `ui-web/src/tokens.css` (si E6 elige WebView2). Ningún otro archivo define
  valores visuales; una prueba lo comprueba buscando literales hex fuera de los
  archivos generados.
- `docs/design/notch-animation.html` se regenera a partir de los tokens y
  sirve como página viva de referencia.

## 10. Accesibilidad
- Estado = forma + color + sonido; nunca solo color.
- Notch sin foco en reposo (`WS_EX_NOACTIVATE`); solo toma teclado al editar
  un campo.
- Todos los controles del menú tienen nombre accesible (UIA en Python; ARIA si
  es WebView2).
- Earcons y TTS respetan el volumen del sistema; existe un ajuste «solo
  visual» para entornos donde no se puede sonar.
