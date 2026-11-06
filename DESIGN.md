---
name: EzAgent
description: "Un instrumento de atención: grafito sólido, dos ojos mint y una sola señal viva."
colors:
  body: "#0E0F13"
  deep-well: "#08090C"
  group: "#1C1C1F"
  group-hover: "#26262A"
  track: "#2F2F34"
  surface-lift: "#3A3A41"
  text: "#F5F5F7"
  text-2: "#B6B6BF"
  text-3: "#94949E"
  text-4: "#85858F"
  line: "rgba(255,255,255,.09)"
  line-2: "rgba(255,255,255,.06)"
  line-strong: "rgba(255,255,255,.14)"
  line-bright: "rgba(255,255,255,.22)"
  mint: "#5EEAC4"
  mint-strong: "#7FF0D2"
  mint-ink: "#0B1220"
  mint-wash: "rgb(94 234 196 / 10%)"
  mint-line: "rgb(94 234 196 / 34%)"
  warm: "#FFC24A"
  warm-strong: "#FFD06E"
  warm-ink: "#1A1200"
  warm-wash: "rgb(255 194 74 / 12%)"
  warm-line: "rgb(255 194 74 / 36%)"
  err: "#FF453A"
  err-strong: "#FF6A61"
  err-ink: "#0E0F13"
  err-wash: "rgb(255 69 58 / 12%)"
  err-line: "rgb(255 69 58 / 36%)"
  eye-idle: "#3E927D"
  eye-mute: "#B78C3A"
typography:
  display:
    fontFamily: "Geist Variable, Geist, Segoe UI, system-ui, sans-serif"
    fontSize: "38px"
    fontWeight: 520
    lineHeight: 1.08
    letterSpacing: "-0.038em"
  headline:
    fontSize: "24px"
    fontWeight: 540
    lineHeight: 1.15
    letterSpacing: "-0.03em"
  title:
    fontSize: "16px"
    fontWeight: 600
    lineHeight: 1.2
    letterSpacing: "-0.012em"
  row:
    fontSize: "14px"
    fontWeight: 400
    lineHeight: 1.4
  body:
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.5
  body-small:
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.5
  control:
    fontSize: "12px"
    fontWeight: 600
    letterSpacing: "-0.005em"
  label:
    fontFamily: "Geist Mono Variable, Geist Mono, Cascadia Mono, Consolas, monospace"
    fontSize: "11px"
    fontWeight: 500
    letterSpacing: "0.1em"
  numeric:
    fontFamily: "Geist Mono Variable, Geist Mono, Cascadia Mono, Consolas, monospace"
    fontSize: "11px"
    fontWeight: 500
    fontFeature: "tnum"
rounded:
  xs: "6px"
  sm: "9px"
  md: "12px"
  lg: "18px"
  pill: "999px"
components:
  button:
    backgroundColor: "transparent"
    textColor: "{colors.text-2}"
    rounded: "{rounded.pill}"
    padding: "0 14px"
    height: "32px"
  button-primary:
    backgroundColor: "{colors.mint}"
    textColor: "{colors.mint-ink}"
    rounded: "{rounded.pill}"
    height: "32px"
  transport:
    backgroundColor: "{colors.mint}"
    textColor: "{colors.mint-ink}"
    rounded: "{rounded.pill}"
    height: "44px"
  group:
    backgroundColor: "{colors.group}"
    rounded: "{rounded.md}"
  field:
    backgroundColor: "{colors.deep-well}"
    textColor: "{colors.text}"
    rounded: "{rounded.sm}"
    height: "34px"
  switch:
    backgroundColor: "{colors.track}"
    rounded: "{rounded.pill}"
    width: "36px"
    height: "20px"
  slug:
    backgroundColor: "{colors.group}"
    textColor: "{colors.text}"
    rounded: "{rounded.md}"
---

# Sistema de diseño: EzAgent

## Resumen

> **Alcance vigente (6 nov 2026):** solo el notch y su menú desplegable. La app Tauri/React, el dashboard y el panel de llamada fueron eliminados. Cualquier referencia residual a una app de escritorio es histórica y no se implementa.

**Estrella polar: «El instrumento que te mira».**

EzAgent es un asistente de voz local. No es un chat ni un panel de analítica: es un instrumento que escucha, piensa y habla, y que siempre deja claro en qué estado está. La marca entera se apoya en un solo gesto: **dos cápsulas verticales que son los ojos del agente**. Los ojos no son decoración; son la representación de su atención. La onda de cinco cápsulas solo existe cuando el agente habla: es el sonido que sale de él.

El notch es todo el producto y la referencia visual: la misma materia, la misma luz, el mismo acento y el mismo movimiento en la isla y en su menú. Todo lo que aparece en el menú debe poder explicarse con las reglas de este documento.

**Características clave**

- Cuerpo grafito sólido (`#0E0F13`), grupos (`#1C1C1F`), sin bordes decorativos; la forma se define con luz y hairlines.
- Un solo acento: mint (`#5EEAC4`) para reposo, escucha y habla. Ámbar (`#FFC24A`) para procesar o pausar. Rojo (`#FF453A`) para error. Nada más.
- Un solo símbolo: los ojos. Aparecen en el notch, el icono y el menú. La onda es una lectura de señal, no una segunda marca.
- Nunca solo color: cada estado cambia también la forma (ancho, alto, giro). La isla no lleva texto; el menú conserva las etiquetas de sus controles.
- Tipografía Geist para la voz; Geist Mono solo para medidas, tiempos, valores y micro-etiquetas.
- Forma capsular en controles, grupos de 12 px y superficies flotantes de 18 px. Sin tarjetas anidadas.
- Movimiento corto y preciso: 120/180/240/320 ms. Muelle solo en poses de ojos, apertura de la isla y thumbs; nunca en la altura del menú.
- La isla respeta contraste AA y movimiento reducido. El notch nunca roba el foco.
- Español de España, factual y sereno. Sin emojis, sin exclamaciones decorativas, sin promesas inventadas.

**Anti-referencias confirmadas:** dashboard de IA genérico, neón y glow decorativo, degradados de texto, glassmorphism, tarjetas gigantes redondeadas, rejillas de iconos, tooltips flotantes en el notch, ventanas secundarias para ajustes rápidos.

**Decisiones posteriores a la spec original (v1.0).** Estas decisiones de producto sustituyen la especificación histórica donde entran en conflicto y son la referencia vigente:

- La isla cerrada es **169×36** en horizontal o **36×186** en un lateral, no 320×36. Al abrir el menú se ensancha con él hasta 320×596 (356×560 en lateral) y comparte una sola silueta continua.
- La isla **no dibuja texto**: ni palabra de estado, ni pistas, ni tooltips.
- No hay **botón de llamada**: se inicia/termina con `Ctrl+Alt+Space`; el mensajero arranca escuchando.
- El micrófono y la flecha son **iconos desnudos**: sin discos, cápsulas, bordes ni fondos. El mute es el micrófono tachado en rojo, sin wash.
- La flecha es un **chevrón de 14 px** a la derecha; gira 180° y pasa a mint al abrir. No hay tirador inferior.
- El menú se abre hacia el lado con más espacio y la isla se acopla arriba/abajo o flota como píldora.
- Los selectores de micrófono y voz **no agrandan** la ventana: usan la zona con scroll de C-11 dentro de los 560 px para no saltar de tamaño.
- Los iconos usan Segoe Fluent si está disponible y el respaldo vectorial propio si falta; el resultado visual es el mismo glifo.
- Las claves y rangos reales del menú son los de `settings.py`/`config.py` (RG-3); la tabla 7.4 de la spec es orientativa.
- **Flujo directo:** una orden explícita se ejecuta sin confirmación; la respuesta es una frase corta con qué se abre y dónde. `confirm_actions` queda desactivado por defecto.

---

## Colores

### Superficies y tinta

| Token | Valor | Uso |
|---|---|---|
| `BODY` | `#0E0F13` | Cuerpo de la isla y del menú |
| `DEEP_WELL` | `#08090C` | Pozos de entrada, selects, registros |
| `GROUP` | `#1C1C1F` | Grupos de filas, paneles, tarjetas de instrumento |
| `GROUP_HOVER` | `#26262A` | Fila con el puntero, superficie levantada |
| `TRACK` | `#2F2F34` | Pista de slider, toggle apagado, elementos resaltados |
| `SURFACE_LIFT` | `#3A3A41` | Scrollbar y elevación mínima |
| `TEXT` | `#F5F5F7` | Texto principal |
| `TEXT_2` | `#B6B6BF` | Texto secundario, valores, etiquetas de control |
| `TEXT_3` | `#94949E` | Ayudas, etiquetas de sección, texto terciario |
| `TEXT_4` | `#85858F` | Micro-etiquetas, timestamps, placeholders |
| `LINE` | `rgba(255,255,255,.09)` | Hairline de reposo y anillo del tema claro |
| `LINE_2` | `rgba(255,255,255,.06)` | Separadores entre filas |
| `LINE_STRONG` | `rgba(255,255,255,.14)` | Bordes de campos y botones outline |
| `LINE_BRIGHT` | `rgba(255,255,255,.22)` | Hover de bordes y borde de diálogo |

### Acento y estados

| Token | Valor | Uso |
|---|---|---|
| `MINT` | `#5EEAC4` | Ojos activos, foco, selección, acción principal |
| `MINT_STRONG` | `#7FF0D2` | Hover de controles rellenos de mint |
| `MINT_INK` | `#0B1220` | Texto y glifos sobre mint |
| `MINT_WASH` | `rgb(94 234 196 / 10%)` | Fondos de estado mint |
| `MINT_LINE` | `rgb(94 234 196 / 34%)` | Bordes de estado mint |
| `WARM` | `#FFC24A` | Ojos pensando, pausa, atención, cambios sin guardar |
| `WARM_STRONG` | `#FFD06E` | Hover de relleno ámbar |
| `WARM_INK` | `#1A1200` | Texto sobre ámbar |
| `WARM_WASH` | `rgb(255 194 74 / 12%)` | Fondos de atención |
| `WARM_LINE` | `rgb(255 194 74 / 36%)` | Bordes de atención |
| `ERR` | `#FF453A` | Error, ojos de error, micrófono muteado |
| `ERR_STRONG` | `#FF6A61` | Hover de error |
| `ERR_INK` | `#0E0F13` | Glifo sobre fondos de error |
| `ERR_WASH` | `rgb(255 69 58 / 12%)` | Fondos de error |
| `ERR_LINE` | `rgb(255 69 58 / 36%)` | Bordes de error |
| `EYE_IDLE` | `#3E927D` | Ojos en ESPERA (mint al 60 % sobre BODY); en claro se ajusta a `#2F7F74` por AA |
| `EYE_MUTE` | `#B78C3A` | Ojos en PAUSA (ámbar al 70 % sobre BODY); en claro se ajusta a `#A06412` por AA |

**Regla de rareza del mint.** El mint se reserva para el estado vivo, la siguiente acción y la selección actual. Debe ocupar muy poco de la pantalla; si aparece en dos funciones no relacionadas en la misma superficie, una está mal.

**Regla de tres temperaturas.** Ámbar significa atención o una acción reversible pendiente; rojo significa fallo. Ninguno se usa para énfasis ordinario ni para selección. La selección se expresa con tono de superficie y hairline.

**Regla del hairline.** La separación es una línea de 1 px al 9 %. 14 % es el borde de reposo más fuerte; 22 % es el máximo en hover. No existe un borde decorativo de 2 px.

### Tema claro (solo notch)

El notch sigue el tema de Windows (`auto`), o `dark`/`light` desde `voice.json`. En claro: `BODY #FFFFFF`, `GROUP #F5F5F7`, `GROUP_HOVER #EBEBEF`, `TRACK #D1D1D6`, `TEXT #1D1D1F`, `TEXT_2 #52525A`, `TEXT_3 #6E6E73`, `MINT #0C917E`, `WARM #C2760A`, `ERR #D70015`, `MINT_BTN #0A7A6A`, `MINT_INK #FFFFFF`, `WARM_BTN #B25000`, `WARM_INK #FFFFFF`, `EYE_IDLE #2F7F74`, `EYE_MUTE #A06412`. Sin hombros, canto superior recto, redondeo solo abajo, anillo hairline `LINE` y **glow 0**. La isla no dibuja texto en ningún tema; el menú conserva etiquetas y valores.

---

## Tipografía

**Display:** Geist Variable (fallback Geist, Segoe UI, system-ui).
**Cuerpo:** Geist Variable.
**Etiquetas y medidas:** Geist Mono Variable (fallback Geist Mono, Cascadia Mono, Consolas).

Geist es neutral y deja respirar la conversación. Geist Mono solo mide: contadores, dB, tiempos, valores de slider, rutas, micro-etiquetas. No se usa para prosa ni como disfraz técnico.

### Escala

| Estilo | Tamaño / peso | Uso |
|---|---|---|
| **Display** | 38 px · 520 · −0.038 em | Titular del estado en la conversación; baja a 27 px a ≤720 y 24 px a ≤480 |
| **Headline** | 24 px · 540 · −0.03 em | `h1` de Ajustes; baja a 21 px a ≤720 |
| **Title** | 16 px · 600 | Títulos de sección; el diálogo usa 15 px |
| **Row** | 14 px · 400 | Nombre de fila del menú del notch |
| **Body** | 13 px · 400 | Texto de ayuda, listas, cuerpo pequeño |
| **Control** | 12 px · 600 | Botones, menús, controles del dock |
| **Micro** | 11 px · 500 mono · 0.1 em · MAYÚSCULAS | Palabra de estado, etiquetas de sección, medidas, valores |
| **Numeric** | 11 px · 500 mono · tabular | Contadores, dB, tiempos, valores numéricos |

- El notch y su menú usan **solo** Row, Body, Control y Micro (Title queda reservado). Los valores numéricos usan `FS_VALUE`, una variante de Micro sin mayúsculas, para conservar unidades como `1,6 s` y `0,95×`; no añade un estilo nuevo a la jerarquía.

**Palabra de estado.** La isla no dibuja la palabra: el estado se lee por la forma y el color de los ojos. La isla tampoco tiene tooltips ni pistas de texto: los botones son iconos.

- Tamaños fijos en px; sin `clamp()` fluido. El display nunca supera 38 px.
- No hay texto con degradado, ni serif decorativa, ni mono para prosa.

**Regla del instrumento.** Si un número no se mide, no lleva Geist Mono. Si una etiqueta no nombra una medida, una sección o un estado, no lleva mono.

**Regla de la etiqueta de lectura.** La micro-etiqueta va pegada a lo que nombra. No se usa como kicker decorativo encima de un titular.

---

## Espaciado, radios y forma

- Escala: 2, 4, 6, 8, 12, 16, 20, 24 px. El ritmo recurrente es 4/6/8/10/12/14/18/22/36; los medidores y reglas son de 1 o 2 px.
- `R_ISLAND = 14` cerrada, `R_MENU = 18` abierta; el radio inferior interpola con la altura.
- `R_GROUP = 12` para grupos, paneles y tarjetas de instrumento.
- `R_CAPSULE = alto / 2` para botones, sliders, toggles y pills.
- Campos y selects: 9 px. Superficies flotantes: 18 px. Elementos pequeños de menú: 6 px.
- **Regla de radio contenido.** Controles a 9–12 px, superficies flotantes a 18 px, cápsulas solo para controles y estados. Un radio de 16+ px en un panel de escritorio se lee como tarjeta de marketing y queda fuera del mundo.
- **Regla de no anidar.** Una superficie por frontera. Una tarjeta dentro de otra, un panel dentro de otro o un campo con borde dentro de otro son defectos. Usa hairline o un paso de tono.
- La isla cerrada mide **169×36** en horizontal o **36×186** en vertical: el ancho justo para micrófono, ojos y flecha. No cambia entre estados; cambia la cara. Flota como píldora de radio 18 o se acopla a un borde visible con el cuello girado 90° en los laterales, hombros cóncavos de 7 px (oscuro) y redondeo solo en el lado libre; en claro, el canto acoplado es recto.
- El menú crece desde la isla en la **misma ventana**, hacia el lado con más espacio. El cuerpo mide 320 px para que los ajustes respiren; la pestaña compacta queda centrada sobre él. No hay costura, tooltips, popups ni segundas ventanas.

---

## Movimiento

| Token | Valor | Uso |
|---|---|---|
| `T_MICRO` | 120 ms | Hover, cambios de contenido y fades pequeños |
| `T_FAST` | 180 ms | Color, aparición de contenido, cambios de estado |
| `T_BASE` | 240 ms | Poses de ojos, giro de flecha y transiciones base |
| `T_SLOW` | 320 ms | Apertura/cierre del menú, onda de habla |
| `T_OPEN` | 380 ms | Despertar de la isla |
| `T_CLOSE_EYES` | 200 ms | Cierre de ojos |
| `T_CLOSE_RETRACT` | 260 ms | Retirada de la isla |
| `VOICE_ATTACK` | 45 ms | Subida del nivel de voz |
| `VOICE_RELEASE` | 200 ms | Caída del nivel de voz |
| `EASE_OUT` | `cubic-bezier(.2,.8,.2,1)` | Todo lo que aparece o se expande |
| `EASE_IN` | `cubic-bezier(.5,0,.9,.4)` | Todo lo que se va o se recoge |
| `EASE_INOUT` | `cubic-bezier(.4,0,.2,1)` | Bucles (deriva, barrido) |
| `EASE_SPRING` | `cubic-bezier(.34,1.56,.64,1)` | Poses de ojos, apertura, thumbs |

**Regla de uso.** Muelle solo para poses de ojos, apertura de la isla y thumbs de toggle/slider al soltar. `EASE_OUT` para aparecer; `EASE_IN` para retirarse. **Nunca** muelle en la altura del menú: recortaría el contenido.

**Regla de la voz.** Ataque rápido, caída suave: 45 ms / 200 ms. La isla responde a la voz; no hace una animación decorativa.

**Regla del reposo vivo.** Parpadeo cada 4,5–6,5 s y deriva de ±2 px. Solo se mueve un grupo a la vez. Si se reduce el movimiento, se conserva el estado y se apaga la coreografía.

### Movimiento reducido

Con `SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION) = 0`: sin muelle, sin deslizamientos, sin escalonado, sin parpadeo, sin deriva, sin barrido, sin saltitos ni sacudida. Las poses y los colores cambian con un fundido de 120 ms `EASE_OUT`. El menú aparece y desaparece cambiando la altura en un fotograma, con el contenido en fundido de 120 ms. Los ojos de ESCUCHO y HABLO siguen reaccionando a la voz (es información, no decoración), pero sin glow. 

---

## Componentes

### Marca

Los ojos, dos cápsulas verticales, son la marca completa. En la cabecera viven dentro de un cuadrado de 30 px con radio 9 y fondo `GROUP`, sin borde. En el icono y el favicon, el mismo cuadrado grafito con los ojos mint. La marca no usa onda, ni degradados, ni sombras.

- Marca en reposo: ojos `EYE_IDLE` o mint, según el tamaño; a 16 px se lee como dos puntos verticales.
- Marca activa: ojos mint. Nunca se anima dentro de la cabecera; el reposo vivo pertenece al notch.
- Tamaño mínimo: 16 px.

### Botones

Capsulares y contenidos. Altura 32 px (28 pequeño, 44 transporte), padding `0 14px`, texto 12/600, iconos de 15–16 px, transición 120 ms `EASE_OUT`, pulsación con escala 0.96 y vuelta en 120 ms.

- **Fantasma:** transparente, `TEXT_2`; hover `GROUP_HOVER` y `TEXT`.
- **Contorno:** `GROUP` con hairline `LINE_STRONG`; hover `GROUP_HOVER` y `LINE_BRIGHT`.
- **Primario:** `MINT` con `MINT_INK`; hover `MINT_STRONG`. Exactamente un botón relleno de mint por región.
- **Ámbar:** `WARM` con `WARM_INK`; solo para atención o guardado con cambios.
- **Peligro:** `ERR_WASH`, borde `ERR_LINE`, texto `ERR`; hover sube el wash.
- **Deshabilitado:** 40–45 % de opacidad, sin hover.

Foco global: outline de 2 px `MINT` con offset 2. No se diseña un foco distinto por botón.

### Campos y controles

- Campos de 34 px, radio 9, fondo `DEEP_WELL`, borde `LINE_STRONG`, texto 13 px, caret mint. Hover a `LINE_BRIGHT`; foco con borde `MINT_LINE` y anillo de 3 px al 8 %.
- Textareas de 104 px mínimo, padding 9×10, línea 1.55.
- Selects con el mismo cuerpo; abierto con el mismo foco.
- Sliders: pista de 4 px `TRACK`, relleno mint hasta el thumb, thumb de 14 px `TEXT` (16 en hover, 18 al arrastrar), valor en mono tabular de 11 px. El cambio se redondea al paso.
- Toggles: 36×20, thumb de 16 px con 2 px de margen; apagado `TRACK` + `TEXT_2`; encendido `MINT_BTN` + `MINT_INK`; 180 ms con muelle en el thumb.
- Chips: píldoras de 25 px, mono 10 px, fondo `GROUP`, hairline; hover/selección con wash y línea mint.

### Grupos de ajustes (firma)

La unidad de densidad del producto. Etiqueta de sección fuera, en Micro (`TEXT_3`). Contenedor `GROUP` con radio 12 y sin borde; filas de 36 o 44 px separadas por hairline `LINE_2` insertada 12 px a cada lado. En tema claro, además, anillo `LINE` de 1 px. Hover de fila a `GROUP_HOVER` en 120 ms. Primera y última fila conservan las esquinas del grupo.

En el notch, el grupo AUDIO mide 196 px (selector 36 + medidor 72 + 2 sliders 44), MOTOR 72 px, TTS 168 px, VAD 176 px, WHISPER · STT 160 px, LLM · API 328 px y CONVERSACIÓN 116 px. El menú visible sigue midiendo 560 px y el contenido que no cabe se desplaza con rueda. Al abrirse, la pestaña se ensancha con el menú hasta 320×596 y comparte la misma silueta. Acoplada a un lateral, la pieza mide 36×186 cerrada y crece hacia dentro hasta 356×560 (rail de controles de 36 + panel de 320). La isla cerrada no reserva ese ancho: mide 169×36 en horizontal o 36×186 en vertical.

### Menú desplegable del notch (firma)

Vive en la misma ventana layered. Cabecera de 28 px con «AJUSTES» y botón de ocultar; grupos AUDIO, MOTOR, TTS, VAD, WHISPER · STT, LLM · API y CONVERSACIÓN; pie con estado de guardado y botón Guardar. Selectores que se despliegan dentro del grupo (32 px por elemento, máximo 5 visibles, uno solo abierto), un medidor de entrada con nivel RMS en vivo y marca de umbral arrastrable, sliders con arrastre fuera de la ventana, filas de texto editable, toggles, botón de prueba de voz y scroll con pulgar visible. Los modelos de Whisper y LLM son filas de selección, no campos de texto.

- Abrir: altura 36→596 en 320 ms `EASE_OUT`; el ancho crece de 169 a 320 px a la vez; contenido con opacidad y +6 px, escalonado 60/100/140/180/220 ms; flecha 0→180° en 240 ms. La isla se queda en su ancla: el menú se despliega debajo o encima según el espacio y la ventana crece alrededor de la flecha.
- Cerrar: contenido a 0 en 120 ms; altura 596→36 en 260 ms `EASE_IN` desde 60 ms; flecha 180→0°.
- Guardar: botón ámbar con cambios, check de 600 ms, «TODO GUARDADO» en mint 800 ms.
- Si no cabe en pantalla, la zona entre cabecera y pie hace scroll con rueda de 40 px y barra de 3 px.

### Ojos y estados (firma)

| Estado | Palabra | Forma | Color | Movimiento |
|---|---|---|---|---|
| `off` | — | — | — | Isla retirada |
| `open` | ABRE | raya 11×3 | `EYE_IDLE` | Transición de despertar |
| `idle` | ESPERA | cápsula 7×11, gap 14 | `EYE_IDLE` | Parpadeo + deriva |
| `listen` | ESCUCHO | cápsula 8×(15+nivel·7), gap 16 | `MINT` | Parpadeo + voz |
| `think` | PIENSO | cápsula 8×8, gap 14 | `WARM` | Barrido horizontal |
| `speak` | HABLO | cápsula 8×(10+nivel·3) + onda 27 | `MINT` | Voz |
| `notice` | AVISO | arco ^ ^ 11×6, trazo 3 | `MINT` | Dos saltitos |
| `mute` | PAUSA | raya 11×3, gap 14 | `EYE_MUTE` | Ninguno |
| `error` | ERROR | cápsula 8×12, ±18° | `ERR` | Una sacudida |
| `close` | CIERRA | raya 11×3 | `EYE_IDLE` | Cierre |

- La columna **Palabra** es el nombre canónico del estado, no texto dibujado en la isla: el notch no tiene texto ni tooltips.
- La onda de 5 cápsulas (3 px de ancho, 3 de separación, 27 en total) solo existe en HABLO; el hueco entre ojo y onda es de 10 px.
- **Mute del micrófono:** el icono pasa a `microphone-slash` en `ERR`, con la diagonal visible y sin fondo. Los ojos conservan `EYE_MUTE`; el rojo pertenece al control que el usuario acaba de apagar, no al estado de la conversación.
- Glow aditivo máximo 0,10 en oscuro y 0 en claro; solo ojos activos. Sin sombra en botones, filas ni texto.
- El estado mostrado es una función pura de `visible`, `error_active`, `base` y `muted`; `speak`, `think` y `notice` tienen prioridad sobre `mute`.
- Permanencia mínima de 120 ms; `vad_end` con gracia de 250 ms; `notice` 1000 ms y vuelta a `idle` si no llega TTS en 1500 ms.

### Nombres de estado

Los nombres canónicos (`ESPERA`, `ESCUCHO`, `PIENSO`, `HABLO`, `AVISO`, `PAUSA`, `ERROR`) sirven de referencia interna; el notch no dibuja texto y el menú usa etiquetas de control.

### Ayuda y copy

- Español de España, directo, factual y tranquilo. Sin emojis, sin exclamaciones decorativas, sin jerga innecesaria.
- Cada control tiene etiqueta visible. La ayuda aparece solo cuando evita un error o explica **cuándo** se aplica un cambio; una idea por ayuda, 1–2 líneas.
- Se dice siempre la verdad del sistema: si un ajuste se aplica al reconectar, se dice; si el motor está parado, se dice; si el dato va a la nube, se dice.
- Los errores explican qué pasó, por qué y qué hacer. Nunca se muestra un mensaje en inglés crudo.
- La isla no tiene texto ni tooltips: los botones son iconos vectoriales y el estado se lee en los ojos. El menú desplegable sí etiqueta cada control.
- No se inventan métricas, latencias ni capacidades. Las cifras salen del README o de una medición real.

---

## Layout

- Notch cerrado: pestaña de 169×36 en la misma ventana layered. Controles: mic 19–47, ojos en `cx=86,5`, flecha 126–150; margen de 19 px a cada lado. Iconos desnudos: sin discos, cápsulas, bordes ni fondos. Ningún rectángulo se solapa; el grupo de ojos más ancho (63 px) queda entre 55 y 118. No hay botón de llamada: la conversación se inicia o termina con `Ctrl+Alt+Space`.
- **Arranque en escucha.** Al abrir el mensajero, la llamada se inicia sola a los 600 ms; el atajo queda para colgar o retomar. La isla se muestra aunque estuviera oculta en la sesión anterior.
- **Flecha del menú.** Chevrón de 14 px sin cápsula, borde ni fondo, a la derecha de la isla. Gira 180° y pasa a mint cuando el menú está abierto; en hover solo cambia el color del glifo. El punto ámbar de cambios sin guardar vive junto a su esquina superior derecha.
- **Pieza continua.** Cerrada, la isla es una píldora compacta. Al desplegar el menú, el ancho crece de 169 a 320 px junto con el alto; la barra y el panel se dibujan como una sola silueta, sin costura ni escalón. Al cerrar, ambos vuelven juntos a la píldora. En el acople lateral, la píldora de 36×186 crece hacia dentro hasta 356×560, conservando el cuello girado y un rail de 36 px para los controles.
- Notch abierto: la pestaña se ensancha **con** el menú hasta 320 px y comparte una sola silueta continua, sin escalón entre barra y panel. Los controles se reparten: mic 19–47, ojos en `cx=160` y flecha 277–301, todos como iconos desnudos. La ventana mide 320×596 y el ancla es `(centro, borde superior)`; el menú se despliega encima o debajo y la isla no se mueve. En lateral, el panel mide 356×560, con un rail de 36 px para micrófono, ojos y flecha, y el menú crece hacia dentro.
- Arrastre: el cuerpo de la pestaña (no los botones) mueve la isla. Lejos de los bordes es una píldora flotante; a ≤12 px del borde superior se acopla con el cuello hacia arriba; a ≤12 px del borde inferior útil se acopla con el cuello hacia abajo; a ≤12 px de un lateral se acopla como píldora vertical de 36×186 con el cuello girado y los controles apilados. La barra de tareas cuenta como borde inferior; la isla nunca sale del monitor y los laterales dejan 16 px a las esquinas.
- Menú: cuerpo útil x 7–313; contenido x 19–301; filas x 31–289; etiquetas x 23. Cabecera 44–72, grupos según 7.3, pie 540–584, relleno inferior 12.
- DPI: px lógicos a 96 DPI; `scale = dpi/96`; anti-alias de 1 px lógico; buffers y cachés se vacían en `WM_DPICHANGED`.

**Regla de la isla arrastrable.** La isla cerrada es un mando, no un mueble: se agarra por su cuerpo y se coloca en cualquier punto. La forma sigue al puntero: dentro de la franja de 12 px de cualquiera de los cuatro bordes, el cuello mira a ese borde; en medio, es una píldora. El menú sigue el mismo principio: se abre hacia el lado que tiene espacio (en los laterales, hacia el interior) y nunca tapa la barra de tareas.

---

## Elevación y profundidad

El sistema es plano por defecto. La profundidad se expresa con pasos de tono y hairlines: `DEEP_WELL` (pozo) → `BODY` (página) → `GROUP` (dock, grupos, píldoras) → `GROUP_HOVER` (popover, diálogo) → `TRACK` (highlight) → `SURFACE_LIFT` (scrollbar). Las superficies en reposo no proyectan sombra.

- **Popover/select:** sombra suave `0 18px 50px rgb(0 0 0 / 45%), 0 2px 8px rgb(0 0 0 / 30%)`, hairline y radio 18.
- **Toast:** `0 16px 44px rgb(0 0 0 / 40%)`.
- **Diálogo:** `0 24px 70px rgb(0 0 0 / 50%)`; único modal.
- **Halo de estado:** solo en el punto de motor y en los ojos activos; nunca en botones.
- **Blur:** el menú es opaco; no hay tarjetas de cristal ni paneles difuminados.

**Regla del campo plano.** Las superficies en reposo son planas. La sombra pertenece solo a lo que flota: popover, select, toast, tooltip, diálogo y halos de estado. Una sombra en una tarjeta, un campo o un botón en reposo es un defecto.

**Regla del blur funcional.** El blur separa, no decora.

---

## Accesibilidad

- Texto sobre `BODY`: `TEXT_3` ≥ 4,5:1; `TEXT_2` ≥ 7:1; `MINT` ≥ 10:1. Los pares exactos se comprueban con la función de contraste del proyecto.
- Foco visible global: outline de 2 px `MINT`, offset 2. Campos: borde `MINT_LINE` y anillo de 3 px al 8 %. Slider: anillo de 4 px al 16 %.
- Todo control tiene etiqueta o nombre accesible; los switches exponen `role="switch"` y `aria-checked`; los estados se anuncian con `role="status"`/`aria-live="polite"`; los errores con `role="alert"`.
- El notch no se activa al cerrarse ni al navegar por el menú; solo pide el teclado mientras se edita una fila de texto. Al confirmar, cancelar o pulsar fuera, devuelve el foco a la ventana anterior y vuelve a `WS_EX_NOACTIVATE`. El menú se abre y cierra con la flecha o `Ctrl+Alt+O`.
- El estado nunca depende solo del color: forma + color en el notch y etiquetas en el menú.
- Movimiento reducido en ambos sistemas.
- El notch no roba el foco en reposo: `WS_EX_NOACTIVATE`, `MA_NOACTIVATE` y `HTTRANSPARENT` en píxeles transparentes. Solo la edición de texto activa la ventana de forma temporal para recibir teclado.

---

## Do's y Don'ts

### Do

- Usa `BODY` como página y construye profundidad con `GROUP`, `GROUP_HOVER` y hairlines del 9 %.
- Reserva el mint para el estado vivo, la siguiente acción y la selección; un solo botón mint por región.
- Usa ámbar para atención o trabajo pendiente y rojo para fallo; en el notch, forma y color.
- Usa Geist Mono solo para medidas y micro-etiquetas, con cifras tabulares.
- Mantén el transporte de 44 px y el dock fijo; conserva el orden micrófono → transporte → silencio/medidor.
- Mantén los ojos como la única marca; la onda es señal, no marca.
- Usa grupos de 12 px y filas de 36/44 con hairlines `LINE_2`.

- Usa el diálogo nativo para la única interrupción y enfoca «Seguir editando».
- Respeta el movimiento reducido y el foco mint visible.
- Escribe español de España, factual y sereno; cita el estado real.
- Explica el efecto y el momento de aplicación de cada ajuste en una línea.

### Don't

- No construyas un dashboard de chat + ajustes en panel dividido.
- No reintroduzcas la barra de título nativa, ni dejes que el dock toque el borde, ni encierres el stage en una columna de 1000 px con franjas negras.
- No añadas neón, glow decorativo, degradados de texto o bordes, tarjetas de cristal, paneles difuminados, tarjetas gigantes redondeadas ni rejillas de iconos.
- No pongas sombras en superficies en reposo ni uses blur decorativo.
- No anides tarjetas ni paneles; no uses radios de 16+ px en paneles de escritorio.
- No uses mint de forma decorativa ni introduzcas un segundo acento; no uses ámbar o rojo para selección.
- No uses Geist Mono para prosa ni para etiquetas de botón; no sustituyas la escala fija por `clamp()`.
- No añadas un segundo modal, apiles diálogos ni autoguardes ajustes.
- No publiques un control solo-icono sin nombre accesible ni un estado que dependa solo del color.
- No dibujes la onda como ilustración o ecualizador decorativo; debe ser señal real.
- No inventes datos, benchmarks ni lenguaje de nube; no uses emojis ni exclamaciones decorativas.
- No uses tooltips ni ventanas secundarias para el menú del notch; el menú crece dentro de la isla.
