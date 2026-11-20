# DX-1g — La ventana del chat tiene varias sombras, una sobre otra

**Lee `AGENTS.md`, `rust-migration/reports/DX-1b-r1.md`, `DX-1c-r1.md` y tu informe de
DX-1f.** Rama `work/DX-1f` (sigue sobre tu entrega de DX-1f; informe aparte).

## Lo que ve la persona (19-nov noche, app con DX-1b + DX-1d)
«La ventana del chat tiene como varias sombras una sobre otra; la idea es que sea una
sola sombra.» El diseño es **una** sombra suave alrededor del panel del chat.

## Sospecha (compruébala, no la des por cierta)
Desde DX-1b el chat se compone por capas (marco/fondo y contenido, más el parche del
cursor de DX-1d): si más de una capa dibuja su propia sombra, o la sombra se compone sobre
un búfer que no se limpia entre cuadros (ventana *layered* con alfa premultiplicado), las
sombras se suman y se ven apiladas o más oscuras. Compara con la app de antes de DX-1b
(`acacccd^`).

## Qué se hace
1. **Diagnóstico con evidencia**: un PNG del chat abierto (con el arnés de DX-1d/DX-1c, sin
   tocar la app de la persona) antes de DX-1b, en `eco/main` y con tu arreglo; y qué capas
   dibujan sombra y cuántas veces por cuadro. Mide el alfa del borde exterior (debe ser el
   del diseño, no la suma de varias).
2. **Arreglo**: una sola sombra, dibujada una vez, igual en la apertura animada (DX-1f), el
   cierre, el redimensionado y con el chat quieto.
3. **Prueba**: la paridad visual del chat (`hud_chat_parity`, umbrales sin tocar) debe
   seguir en verde; añade un caso o una aserción que falle si la sombra se compone más de
   una vez (alfa del borde por encima del de una sola sombra).
4. Humo `chat`, `real`, `big` y `anim` en release: vivos y sin `PÁNICO`.

## Entrega
fmt, clippy y test release de `ez-hud` y `ez-app` en verde; los PNG (en
`rust-migration/reports/DX-1g/`) y las medidas. Informe `rust-migration/reports/DX-1g-r1.md`,
commit en `work/DX-1f`. Compila en primer plano. No toques la app de la persona.
