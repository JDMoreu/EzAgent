# 07 · Marca y dirección de arte: el hilo

Estado: **propuesta pendiente de aprobación** (8 nov 2026). Sustituye a la
dirección «la mirada que delega» de `03-diseno-y-movimiento.md` si se aprueba.
Mientras tanto, E7 no empieza (necesita esta decisión y la de E6).

**Prototipo vivo:** [`docs/design/hilo.html`](../design/hilo.html). Incluye el hilo
con física real, la alternativa «Gota» para comparar, la personalidad, el
sistema visual y la marca.

## Por qué cambiar
El notch y la píldora obligaban a resolver cómo meter ojos y onda en una caja, y
la animación de hablar se sentía añadida (una onda pegada a un lado, con ruido
aleatorio). Cualquier asistente tiene una caja o un orbe; EzAgent hace otra
cosa: **une lo que dices con lo que hacen tus agentes**. Esa unión es el símbolo.

## Idea: tu voz es un hilo hacia tus agentes
Tú a la izquierda (un punto), tus agentes a la derecha (puntos violeta) y
EzAgent en medio como una cuerda. Sin marco ni caja: un lienzo transparente de
320×120 px y un trazo de 2 px que flota donde lo dejes.

| Estado | Qué hace el hilo |
|---|---|
| Espera | Respira: una onda lenta y mínima de izquierda a derecha |
| Te escucha | Tu punto late con tu voz, manda ondas por el hilo y emite anillos |
| Piensa | Se aquieta; una chispa ámbar lo recorre en bucle |
| **Habla** | **La voz excita los primeros modos de la cuerda (fundamental y dos armónicos): vibra limpia, con sílabas y pausas; la amplitud sigue el audio** |
| Despacha | Una chispa violeta viaja hasta el final y nace el punto de un agente |
| Agente trabaja | Su punto respira; el extremo del hilo toma tinte violeta |
| Agente terminó | El punto se enciende en lima y el hilo suena como cuerda pulsada |
| Agente bloqueado | Punto ámbar que tira del hilo cada 1,6 s hasta que se atiende |
| Silenciado | Hilo punteado y quieto, con el punto tachado |
| Error | El hilo se parte un instante en rojo y se une |

### Por qué «habla» funciona ahora
La primera versión excitaba la cuerda con impulsos sueltos y salía un garabato de
alta frecuencia (se vio en una captura real). Excitar solo los primeros modos
da la curva limpia de una cuerda que suena. Amplitud medida con la envolvente de
voz del prototipo: mediana 6,9 px, P95 13 px, máximo 15,8 px sobre el lienzo.

## Marca
- **Símbolo:** una «e» dibujada de un solo trazo que no se cierra: su trazo sale
  como un hilo y termina en un punto violeta (el agente). Mismo gesto que el
  hilo en pantalla. Funciona a 16 px.
- **Paleta:** tinta, una señal (lima), un violeta que solo existe para agentes,
  ámbar para atención y rojo para falla. Valores en `03-diseno-y-movimiento.md`.
- **Tipografía:** Geist y Geist Mono.
- **Personalidad: «el relevo».** Recibe tu encargo y se lo pasa a quien toca, sin
  protagonismo. Ágil, sereno, directo, cálido y discreto. Tabla de «suena así /
  nunca así» en el prototipo y en `01-producto-y-marca.md` §3.

## Lo que implica construirlo
1. **Render:** una cuerda de 96 nodos (ecuación de onda 1D, paso fijo de 1/240 s)
   más una suma de tres modos para la voz. Coste trivial en CPU; cabe en un
   lienzo transparente. Es un argumento a favor de **WebView2/Tauri en E6**
   (Canvas 2D, ventana transparente, sin Pillow por fotograma).
2. **Ajustes:** al abrirlos, una tarjeta de 18 px de radio se condensa del hilo
   (muelle `panel`, sin rebote). Contenido de E4 sin cambios.
3. **Confirmación:** la misma tarjeta, con No/Sí.
4. **Posición:** libre en pantalla, arrastrable; acoplado opcional al borde
   superior como línea fina.
5. **Icono:** la «e» con punto, en tile tinta y en variante lima.
6. **Tokens:** el hilo usa `signal`, `agent`, `attention`, `fault` de
   `design/tokens.json` (E7); sin valores propios.

## Decisiones abiertas (del usuario)
1. ¿Hilo o Gota (o ninguno)? Recomendación: **Hilo**. «Gota» es fluida, pero se
   parece a cualquier orbe de IA.
2. ¿Se confirma la «e» con punto como marca y la personalidad «el relevo»?
3. ¿Nombre? Sigue pendiente para E9 (`01-producto-y-marca.md` §5).

---

## Decisión del usuario (8 nov 2026): se prefiere la Gota

La dirección elegida es **la gota** (no el hilo). El resto de la marca (la «e» con
punto, la personalidad «el relevo», la paleta) se mantiene. El hilo queda como
alternativa descartada, conservada en `docs/design/hilo.html`.

**Prototipo en Windows:** [`docs/design/gota-windows.html`](../design/gota-windows.html).

### Cómo se interactúa sin iconos fijos
| Acción | Puntero | Teclado | Bandeja | Voz |
|---|---|---|---|---|
| Silenciar / reactivar | Acercarse: brotan dos gotas pequeñas; la izquierda (micrófono) | Ctrl+Alt+M | Menú → Silenciar | «silencia el micrófono» |
| Abrir ajustes | La gota derecha (engranaje) | Ctrl+Alt+O | Menú o clic en el icono «e» | «abre los ajustes» |
| Mover | Arrastrar la gota (imán a bordes, se recuerda por monitor) | — | — | — |
| Ocultar | — | — | Menú → Ocultar la gota | — |

Las gotas pequeñas brotan con el muelle `launch` y se retiran 0,7 s después de
quitar el puntero. Son botones con nombre accesible. La tarjeta de ajustes se
condensa desde la gota (escala y radio, muelle `panel`, sin rebote).

### Integración con Windows
- Ventana transparente siempre encima, sin foco, fuera de Alt+Tab y de la barra
  de tareas. Una sola ventana cubre la gota y su tarjeta; solo el círculo de
  ~96 px (y las gotas pequeñas visibles) recibe clics, el resto atraviesa.
- Posición inicial: abajo a la derecha, 16 px sobre la barra de tareas, junto a la
  bandeja. Arrastrable; se recuerda por monitor; escalado por monitor.
- Tamaño: 28 px en reposo (70 % de opacidad) y hasta 44 px al escuchar.
- Se oculta con aplicaciones a pantalla completa; solo muestra una notificación
  de Windows si hay un aviso de agente pendiente y la gota está oculta.
- Icono «e» monocromo en la bandeja, con punto rojo si el micrófono está
  silenciado, y menú contextual con las mismas acciones.
- Alto contraste de Windows y movimiento reducido soportados (solo fundidos).

### Pendiente de medir en E6
Que el recorte de clics por región funcione igual de bien en WebView2 que en
Win32 (es lo primero que debe comprobar el spike).
