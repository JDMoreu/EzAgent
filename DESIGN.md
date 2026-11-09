---
name: EzAgent
description: "Una barra de cielo: una forma fija con nubes dentro que responden a la voz, y una tarjeta de ajustes fresca y silenciosa."
colors:
  card-body-light: "#FAFCFF"
  card-group-light: "#EEF4FD"
  card-text-light: "#161E34"
  card-accent-light: "#2F5FD0"
  card-body-dark: "#111626"
  card-group-dark: "#1A2136"
  card-text-dark: "#E8EEFA"
  card-accent-dark: "#8DB4FF"
  signal-light: "#2F5FD0"
  signal-dark: "#8DB4FF"
  error-light: "#D6333D"
  error-dark: "#FF7876"
typography:
  title: { fontFamily: Geist, fontSize: 16px, fontWeight: 600 }
  row: { fontFamily: Geist, fontSize: 14px, fontWeight: 400 }
  body: { fontFamily: Geist, fontSize: 13px, fontWeight: 400 }
  secondary: { fontFamily: Geist, fontSize: 12px, fontWeight: 600 }
  micro: { fontFamily: Geist Mono, fontSize: 11px, fontWeight: 500, letterSpacing: 0.1em }
---

# Sistema de diseño: EzAgent

La fuente de verdad es el código: `src/ezagent/voice/notch/` (`drop.py`,
`clouds.py`, `bar_motion.py`, `bar_theme.py`, `tokens.py`, `menu.py`). Este
documento resume sus reglas; si algo no coincide, manda el código.

## Resumen

EzAgent es una **barra flotante sin texto**. Una forma de contorno fijo y
nítido contiene un cielo con nubes. El estado se lee por el color y el
movimiento del cielo, nunca por palabras. Los ajustes viven en una tarjeta
propia que se despliega desde ☰.

- **Una sola forma.** El contorno nunca se deforma; lo que se mueve es el
  interior.
- **Sin texto en la barra.** Ni palabra de estado, ni pistas, ni tooltips. Los
  botones son iconos vectoriales.
- **Los botones se ganan.** Solo aparecen al acercar el puntero.
- **Nunca solo color.** Cada pose cambia también el movimiento de las nubes.
- **No roba el foco.** La barra y la tarjeta no toman el teclado salvo mientras
  se edita una fila de texto.

## La barra

Dos disposiciones, elegibles en Ajustes → Apariencia, con doble clic sobre la
forma o con «Cambiar forma» en la bandeja (`ui_layout` en `voice.json`):

| Disposición | Forma | Botones |
|---|---|---|
| `pill` | Cápsula de 148×38 | ☰ a la izquierda; 🎤 y ✕ a la derecha |
| `circle` | Círculo de 58 de diámetro | Fila de 36 px bajo el círculo, con los mismos botones |

- Botones redondos de **36 px**, separados **8 px** (`BTN`, `GAP`). Margen de
  12 px dentro de la ventana para sombras.
- Los tres botones son ☰ (ajustes), 🎤 (micrófono) y ✕ (colgar). Sin llamada, el
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

### Cielo y nubes (según estado y tema)

Dentro de la forma hay cielo despejado arriba y un banco de nubes abajo. Cada
pose tiene su propia paleta (cielo arriba, cielo abajo, nube, sombra de nube,
brillo de aire) en `clouds.py`, para claro y oscuro.

| Pose | Estado | Carácter |
|---|---|---|
| `idle` | Espera / colgado | Azul en calma |
| `listen` | Escuchando | Azul más vivo; el aire entra por abajo |
| `think` | Piensa | Violeta; las nubes giran |
| `speak` | Habla | Azul intenso; un oleaje recorre el banco al ritmo de la voz |
| `ask` | Pregunta (confirmación) | Ámbar |
| `error` | Error | Rojo |
| `mute` | Pausa | Gris apagado |

### Tarjeta de ajustes

Una paleta clara y otra oscura (`bar_theme.py`), con un acento azul cielo en
ambas:

| Token | Claro | Oscuro |
|---|---|---|
| `BODY` | `#FAFCFF` | `#111626` |
| `GROUP` | `#EEF4FD` | `#1A2136` |
| `GROUP_HOVER` | `#E3ECFB` | `#232C46` |
| `TRACK` | `#CDDAF0` | `#344062` |
| `TEXT` | `#161E34` | `#E8EEFA` |
| `TEXT_2` | `#424E6C` | `#B0BCD6` |
| `TEXT_3` | `#626E8C` | `#8C99B8` |
| `MINT` / `MINT_BTN` (acento) | `#255AD2` / `#2F5FD0` | `#8DB4FF` |
| `MINT_INK` (texto sobre acento) | `#FFFFFF` | `#0E162C` |

(Los nombres `MINT*` se conservan por compatibilidad del código; el color es
azul cielo.) El cuerpo lleva un degradado vertical muy suave, un poco más
cielo arriba (`#F1F6FF` claro, `#161D32` oscuro).

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

## Espaciado y forma

- Escala de espaciado: 2 · 4 · 6 · 8 · 12 · 16 · 20 · 24 px (`SP_1`…`SP_8`).
- Tarjeta de **320 px** de ancho; menú de hasta **560 px** de alto visible, con
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
- **Nubes:** tienen inercia. Cuando hablas entra aire por abajo y las empuja e
  hincha; al callar vuelven despacio con un vaivén amortiguado.

Duraciones del menú: 120 / 180 / 240 / 320 ms. Muelle solo en los botones y los
controles al soltar; **nunca** en la altura del menú, porque recortaría el
contenido. Voz: ataque 45 ms, caída 200 ms. Cuadros: 60 fps mientras algo se
mueve y 30 en reposo.

### Movimiento reducido

`ui_motion` en `voice.json`: `system` (sigue a Windows), `full` o `reduced`. En
reducido no hay desplazamiento, escala ni rebote: solo fundidos lineales de
120 ms.

## Componentes de la tarjeta

- **Grupos de ajustes:** AUDIO, MOTOR, TTS, VAD, WHISPER · STT, LLM · API y
  CONVERSACIÓN. Cada grupo es una superficie `GROUP` con etiqueta `FS_MICRO`.
- **Controles:** selectores, sliders con pulgar, interruptores y filas de texto
  editables (la clave del LLM se enmascara).
- **Pie:** muestra `TODO GUARDADO` o `● CAMBIOS SIN GUARDAR`. Guardar usa el
  acento cuando hay cambios y confirma con un check.
- **Iconos:** vectoriales, rejilla de 24 px, trazo 1,8 (1,6 por debajo de 18 px),
  rasterizados con supermuestreo ×4.

## Accesibilidad

- Contraste WCAG: texto y texto de acento ≥ 4,5:1 sobre las superficies de la
  tarjeta; botones de acento con su tinta ≥ 4,5:1. Lo comprueban
  `tests/test_menu_appearance.py` y `tests/test_notch_ui.py`.
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
- Usar los colores de `bar_theme.py` y `clouds.py`, no valores sueltos.

**Don't**
- No deformar la forma ni añadir segundas ventanas, tooltips o popups.
- No usar muelle en la altura del menú.
- No comunicar un estado solo con color.
- No añadir una paleta, un acento o una tipografía nuevos sin actualizar este
  documento y los tests de contraste.
