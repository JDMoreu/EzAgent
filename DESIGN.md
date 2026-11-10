---
name: EzAgent
description: "Marea: una forma nítida con un gradiente orgánico que responde a la voz y superficies coherentes en ajustes y chat."
colors:
  card-body-light: "#FAFDFC"
  card-group-light: "#EDF6F4"
  card-text-light: "#102226"
  card-accent-light: "#0C7075"
  card-body-dark: "#0D161A"
  card-group-dark: "#142126"
  card-text-dark: "#E4F2F0"
  card-accent-dark: "#5EE2C4"
  signal-light: "#0C7075"
  signal-dark: "#5EE2C4"
  error-light: "#C8241A"
  error-dark: "#FF5D52"
typography:
  title: { fontFamily: Geist, fontSize: 16px, fontWeight: 600 }
  row: { fontFamily: Geist, fontSize: 14px, fontWeight: 400 }
  body: { fontFamily: Geist, fontSize: 13px, fontWeight: 400 }
  secondary: { fontFamily: Geist, fontSize: 12px, fontWeight: 600 }
  micro: { fontFamily: Geist Mono, fontSize: 11px, fontWeight: 500, letterSpacing: 0.1em }
---

# Sistema de diseño: EzAgent

La fuente de verdad es el código: `src/ezagent/voice/notch/` (`drop.py`,
`flow.py`, `bar_motion.py`, `bar_theme.py`, `tokens.py`, `menu.py`). Este
documento resume sus reglas; si algo no coincide, manda el código.

## Resumen

EzAgent es una **barra flotante sin texto**. Una forma de contorno fijo y
nítido contiene el gradiente orgánico Marea. El estado se lee por el color y el
movimiento del fluido, nunca por palabras en la barra. Los ajustes viven en una
tarjeta propia; el chat escrito se abre desde la forma y comparte su identidad.

- **Una sola forma.** El contorno nunca se deforma; lo que se mueve es el
  interior.
- **Sin texto en la barra.** Ni palabra de estado, ni pistas, ni tooltips. Los
  botones son iconos vectoriales.
- **Los botones se ganan.** Solo aparecen al acercar el puntero.
- **Nunca solo color.** Cada pose cambia también el movimiento del fluido.
- **No roba el foco.** La barra y la tarjeta no toman el teclado salvo mientras
  se edita una fila de texto.

## La barra

Dos disposiciones, elegibles en Ajustes → Aspecto, con doble clic sobre la
forma o con «Cambiar forma» en la bandeja (`ui_layout` en `voice.json`):

| Disposición | Forma | Botones |
|---|---|---|
| `pill` | Cápsula de 108×38 | Ajustes y chat a la izquierda; micrófono y llamada a la derecha |
| `circle` | Esfera de 58 de diámetro | Botones de 36 px junto a la esfera; posición según el anclaje |

### Tamaño de la barra

`ui_scale` (60 %–200 %, pasos de 5 %, 100 % por defecto) agranda o encoge la
barra entera —forma, botones, márgenes y ventana— alrededor del centro de la
forma de color, que no se mueve. Se ajusta con el slider **Tamaño** (Ajustes →
Aspecto) o con **Ctrl + rueda** sobre la barra. Solo escala la barra: la
ventana de ajustes sigue la escala de pantalla de Windows.

- Botones redondos de **36 px**, separados **8 px** (`BTN`, `GAP`). Margen de
  12 px dentro de la ventana para sombras.
- Los controles son ajustes, chat, micrófono y llamada. Sin llamada, el
  micrófono se pliega y ✕ se convierte en el botón de llamar.
- El micrófono tiene tres estados: escuchando, en pausa (tachado; solo atiende a
  la frase de reactivación) y colgado.
- Una confirmación pendiente oculta ☰, convierte 🎤 en ✓ y deja ✕ como «no».
  El resumen se lee en voz alta; la barra no muestra texto.
- El recorte de clics es por geometría: todo lo que no es forma o botón deja
  pasar el ratón.
- Se puede arrastrar por la forma. La posición se guarda por disposición
  (`drop_x`, `drop_y`, `drop_layout`).

## Color

### Fluido Marea (según estado y tema)

Dentro de la forma fluye un gradiente orgánico. Cada pose tiene una paleta
propia en `flow.py`, para claro y oscuro. Escuchar acelera el fluido desde abajo;
hablar produce pulsos de luz y pensar mantiene un giro lento.

| Pose | Estado | Carácter |
|---|---|---|
| `idle` | Espera / colgado | Marea en calma |
| `listen` | Escuchando | Fluido más activo desde abajo |
| `think` | Piensa | Giro lento |
| `speak` | Habla | Pulsos de luz al ritmo de la voz |
| `ask` | Pregunta (confirmación) | Ámbar |
| `error` | Error | Rojo |
| `mute` | Pausa | Gris apagado |

### Superficies de los ajustes

Una paleta clara y otra oscura en `tokens.py`, compartidas por la barra,
los ajustes y el chat. `bar_theme.py` entrega copias para las superficies:

| Token | Claro | Oscuro |
|---|---|---|
| `BODY` | `#FAFDFC` | `#0D161A` |
| `GROUP` | `#EDF6F4` | `#142126` |
| `GROUP_HOVER` | `#E0EFEC` | `#1C2D33` |
| `TRACK` | `#C7E0DB` | `#2C464C` |
| `TEXT` | `#102226` | `#E4F2F0` |
| `TEXT_2` | `#385256` | `#ACC4C2` |
| `TEXT_3` | `#4E666A` | `#8AA4A3` |
| `MINT` / `MINT_BTN` (acento) | `#0C7075` | `#5EE2C4` |
| `MINT_INK` (texto sobre acento) | `#FFFFFF` | `#06221E` |

El cuerpo lleva un degradado vertical suave, un poco más menta arriba
(`#EEF9F6` claro, `#111E22` oscuro). Ámbar y rojo conservan su significado
de confirmación y error; no se utilizan como acentos decorativos.

### Tema

`theme` en `voice.json`: `auto` (sigue a Windows), `light` o `dark`.

## Tipografía

**Geist** y **Geist Mono**, empaquetadas en `src/ezagent/voice/assets/fonts`.

| Token | Familia | Tamaño | Peso | Uso |
|---|---|---|---|---|
| `FS_TITLE` | Geist | 16 | 600 | Títulos |
| `FS_ROW` | Geist | 14 | 400 | Filas de ajustes |
| `FS_BODY` | Geist | 13 | 400 | Texto de apoyo |
| `FS_SECONDARY` | Geist | 12 | 600 | Etiquetas secundarias |
| `FS_MICRO` | Geist Mono | 11 | 500 | Etiquetas de grupo (mayúsculas, tracking 0,1 em) |

Respaldo en Windows: Segoe UI.

El texto se rasteriza al tamaño físico de pantalla. La medición, la alineación
y los cambios de color reutilizan una sola máscara por texto, estilo y DPI;
no se agrandan imágenes de texto para simular alta definición.

## Espaciado y forma

- Escala de espaciado: 2 · 4 · 6 · 8 · 12 · 16 · 20 · 24 px (`SP_1`…`SP_8`).
- Ventana de ajustes de **760×620** de fábrica (mínimo 560×400, máximo el área
  de trabajo); barra lateral de **184 px**; el menú ocupa el alto de la ventana, con
  desplazamiento por rueda y pulgar visible cuando el contenido no cabe.
- Radios: tarjeta **22**, grupos **12** (`CARD_RADIUS`, `R_GROUP`).

## Movimiento

Coreografía de los botones (`bar_motion.py`):

- **Entrada:** la forma crece un 4 % y los botones salen desde detrás de ella, a
  escala 0,6 y transparentes, con un muelle (~8 % de rebote) y 40 ms de
  escalonado, del más cercano al más lejano.
- **Salida:** orden inverso, 25 ms de escalonado, amortiguación crítica (sin
  rebote).
- **Puntero:** el botón bajo el puntero crece a 1,08; al pulsarlo, 0,94.
- **Fluido:** responde con inercia al nivel de voz y vuelve suavemente al reposo.
- **Continuidad:** los resortes tienen solución analítica; mantienen el mismo
  recorrido a 30, 60 y 144 fps y conservan posición y velocidad al invertirlos.

Duraciones del menú: 120 / 180 / 240 / 320 ms. Muelle solo en los botones y los
controles al soltar; **nunca** en la altura del menú, porque recortaría el
contenido. Voz: ataque 45 ms, caída 200 ms. Cuadros: 60 fps mientras algo se
mueve y 30 en reposo.

### Movimiento reducido

`ui_motion` en `voice.json`: `system` (sigue a Windows), `full` o `reduced`. En
reducido no hay desplazamiento, escala ni rebote: solo fundidos lineales de
120 ms.

## Ventana de ajustes

Una ventana propia, movible y redimensionable, con **cinco secciones** en una
barra lateral con icono (Aspecto, Escucha, Voz, Modelos, Sistema). Cada sección
agrupa tarjetas con etiqueta `FS_MICRO`; el título de la sección (`FS_TITLE`) y
un ✕ forman la cabecera.

| Sección | Grupos |
|---|---|
| Aspecto | Forma y tamaño · Tema |
| Escucha | Micrófono · Fin de turno · Reactivación |
| Voz | Voz · Conversación · Acciones |
| Modelos | Transcripción y diccionario · Proveedor, conexión y configuraciones guardadas |
| Sistema | Atajos · Avanzado |

- **Opciones avanzadas plegadas.** Ajustes finos (VAD, pausas del TTS, motor de
  Whisper, muestreo) solo se ven con «Mostrar opciones
  avanzadas» (`show_advanced`, Sistema → Avanzado). Siguen aplicándose igual.
- **Mover.** Se arrastra desde cualquier zona sin controles (cabecera, barra
  lateral, huecos entre tarjetas). Siempre queda a la vista una franja de 96 px.
- **Redimensionar.** Desde los cuatro bordes y las cuatro esquinas (cursores de
  redimensión; marca de agarre abajo a la derecha). El contenido se estira con
  el ancho y muestra más filas con el alto.
- **Restablecer.** Doble clic en la cabecera: 760×620 y centrada.
- **Memoria.** `settings_x/y/w/h` en `voice.json` se guardan al soltar.
- **Ritmo de filas.** Una línea: 44 px; dos líneas (etiqueta + control): 52 px;
  paletas: 68 px. Todos los controles comparten los márgenes interiores de 12 px
  (`ROW_X0`), el mismo pulgar de 14 px, la misma pista de 4 px y el mismo acento.
- **Controles:** selectores (lista con la opción activa en acento), sliders y
  medidor con pulgar, interruptores, botones capsulares y filas de texto
  editables (la clave del LLM se enmascara).
- **Pie:** muestra `TODO GUARDADO` o `● CAMBIOS SIN GUARDAR`. Guardar usa el
  acento cuando hay cambios y confirma con un check.
- **Iconos:** vectoriales, rejilla de 24 px, trazo 1,8 (1,6 por debajo de 18 px),
  rasterizados con supermuestreo ×4.

### Modelos y configuraciones guardadas

«Proveedor» elige una conexión de fábrica o personalizada. Modelo, lista de
modelos de la API, compatibilidad, URL y clave se editan en Modelos, sin activar
opciones avanzadas. Las claves permanecen enmascaradas.

«Guardar perfil como» asigna un nombre a esa conexión y modelo. El botón
Guardar persiste la configuración en `llm.json`; «Configuración guardada» la
recupera. Nombres diferentes conservan modelos distintos del mismo proveedor;
guardar con el mismo nombre actualiza esa configuración. Los cambios pendientes
siguen indicándose en el pie.

### Diccionario de transcripción

En Modelos → Transcripción, «Correcciones» admite pares como
`open code → OpenCode; cloud → Claude`. También acepta `=>` y reglas pegadas
desde varias líneas. Enter confirma la edición; Guardar conserva las reglas.
Vaciar el campo desactiva las correcciones. Los errores se muestran junto al
campo sin descartar lo escrito ni guardar una regla inválida.

Las sustituciones se aplican localmente después de Whisper, por palabras o
frases completas, sin distinguir mayúsculas. Las frases más largas tienen
prioridad y cada texto se procesa una sola vez para evitar sustituciones en
cadena. No se entrena Whisper ni se añade una llamada de red. Límite: 100 reglas.

## Chat escrito

- **Cabecera:** título y estado a la izquierda; ajustes, historial, nueva
  conversación y minimizar (barra horizontal) a la derecha. El estado se recorta al espacio disponible.
- **Composición:** micrófono y respuesta por voz a la izquierda del campo, en la misma fila.
  Desactivar audio usa un estado neutro; el rojo se reserva para errores.
- **Campo:** borde discreto en reposo, acento al escribir, enviar/detener en
  la misma posición. Sin indicaciones de Enter debajo; los atajos siguen funcionando.
- **Mensajes:** copiar aparece dentro de cada tarjeta, en la esquina inferior derecha,
  con espacio reservado para no tapar contenido. No se muestran etiquetas «por voz».
- **Movimiento:** apertura de 280 ms, cierre de 200 ms; con movimiento reducido,
  fundido de 120 ms y línea de estado sin desplazamiento.
- **Streaming:** el primer fragmento y el resultado final se pintan sin esperar
  el intervalo de agrupación. Los siguientes fragmentos se agrupan cada 50 ms
  para limitar la recomposición de Markdown. Esto no cambia la latencia del LLM.

## Accesibilidad

- Contraste WCAG: texto y texto de acento ≥ 4,5:1 sobre las superficies de la
  tarjeta; botones de acento con su tinta ≥ 4,5:1. Lo comprueban
  `tests/test_settings_window.py` y el validador `notch/a11y.py`.
- Nunca solo color: la pose cambia también el movimiento.
- Movimiento reducido respetado (ver arriba).
- Las acciones principales tienen atajo global (`Ctrl+Alt+Space`,
  `Ctrl+Alt+O`, …); la tarjeta toma el teclado solo al editar texto y lo
  devuelve con `Enter`, `Esc` o un clic fuera.

## Do y Don't

**Do**
- Mantener el contorno fijo y mover solo el interior.
- Comunicar el estado con color y movimiento a la vez.
- Mantener la barra sin texto.
- Usar los colores semánticos de `tokens.py` y las paletas del fluido de `flow.py`.

**Don't**
- No deformar la forma ni añadir segundas ventanas, tooltips o popups.
- No usar muelle en la altura del menú.
- No comunicar un estado solo con color.
- No añadir una paleta, un acento o una tipografía nuevos sin actualizar este
  documento y los tests de contraste.
